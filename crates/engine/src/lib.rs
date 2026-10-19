// SPDX-License-Identifier: GPL-3.0-or-later
//! A bounded, asynchronous service for local UCI engines.
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

pub const PROTOCOL: &str = "UCI";
const HANDSHAKE: Duration = Duration::from_secs(5);
const POLL: Duration = Duration::from_millis(10);
const MAX_EVENTS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Depth(u8),
    TimeMs(u32),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    Cp(i32),
    Mate(i32),
}
impl Score {
    /// Convert the engine's side-to-move score to White's perspective.
    pub fn white_perspective(self, white_to_move: bool) -> Self {
        if white_to_move {
            self
        } else {
            match self {
                Self::Cp(n) => Self::Cp(n.saturating_neg()),
                Self::Mate(n) => Self::Mate(n.saturating_neg()),
            }
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Info {
    pub depth: Option<u32>,
    pub score: Option<Score>,
    pub pv: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    Ready { id: u64 },
    Info { id: u64, info: Info },
    BestMove { id: u64, best_move: Option<String> },
    Stopped { id: u64 },
    Error { id: u64, message: String },
}

/// Find an explicitly configured engine, then common Stockfish installations.
pub fn discover() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("STOCKFISH_EXECUTABLE") {
        let path = PathBuf::from(path);
        return path.is_file().then_some(path);
    }
    for directory in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        for name in ["stockfish", "stockfish.exe"] {
            let path = directory.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    let path = PathBuf::from("/usr/games/stockfish");
    path.is_file().then_some(path)
}

struct Request {
    token: u64,
    id: u64,
    path: PathBuf,
    fen: String,
    white_to_move: bool,
    limit: Limit,
}
#[derive(Default)]
struct Shared {
    token: AtomicU64,
    id: AtomicU64,
    shutdown: AtomicBool,
    pending: Mutex<Option<Request>>,
    wake: Condvar,
    events: Mutex<VecDeque<(u64, Event)>>,
}
impl Shared {
    fn cancelled(&self, token: u64) -> bool {
        self.shutdown.load(Ordering::Acquire) || self.token.load(Ordering::Acquire) != token
    }
    fn publish(&self, token: u64, event: Event) {
        let mut events = self.events.lock().unwrap();
        if self.cancelled(token) {
            return;
        }
        if events.len() == MAX_EVENTS {
            // Prefer retaining lifecycle events over intermediate analysis updates.
            if let Some(index) = events
                .iter()
                .position(|(_, e)| matches!(e, Event::Info { .. }))
            {
                events.remove(index);
            } else {
                events.pop_front();
            }
        }
        events.push_back((token, event));
    }
}

/// All process I/O and waiting happens on a dedicated worker.
pub struct Engine {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}
impl Default for Engine {
    fn default() -> Self {
        Self::new()
    }
}
impl Engine {
    pub fn new() -> Self {
        let shared = Arc::new(Shared::default());
        let state = shared.clone();
        let worker = thread::spawn(move || {
            loop {
                let request = {
                    let mut pending = state.pending.lock().unwrap();
                    while pending.is_none() && !state.shutdown.load(Ordering::Acquire) {
                        pending = state.wake.wait(pending).unwrap();
                    }
                    if state.shutdown.load(Ordering::Acquire) {
                        break;
                    }
                    pending.take().unwrap()
                };
                if state.cancelled(request.token) {
                    continue;
                }
                if let Err(message) = run(&state, &request) {
                    state.publish(
                        request.token,
                        Event::Error {
                            id: request.id,
                            message,
                        },
                    );
                }
            }
        });
        Self {
            shared,
            worker: Some(worker),
        }
    }
    pub fn analyze(&self, id: u64, path: PathBuf, fen: String, white_to_move: bool, limit: Limit) {
        let mut pending = self.shared.pending.lock().unwrap();
        let token = self.shared.token.fetch_add(1, Ordering::AcqRel) + 1;
        self.shared.id.store(id, Ordering::Release);
        self.shared.events.lock().unwrap().clear();
        *pending = Some(Request {
            token,
            id,
            path,
            fen,
            white_to_move,
            limit,
        });
        self.shared.wake.notify_one();
    }
    pub fn stop(&self) {
        let mut pending = self.shared.pending.lock().unwrap();
        let token = self.shared.token.fetch_add(1, Ordering::AcqRel) + 1;
        *pending = None;
        self.shared.events.lock().unwrap().clear();
        self.shared.publish(
            token,
            Event::Stopped {
                id: self.shared.id.load(Ordering::Acquire),
            },
        );
    }
    pub fn try_recv(&self) -> Option<Event> {
        let mut events = self.shared.events.lock().unwrap();
        while let Some((token, event)) = events.pop_front() {
            if !self.shared.cancelled(token) {
                return Some(event);
            }
        }
        None
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        // Hold the predicate mutex while notifying to avoid a lost wakeup.
        {
            let _pending = self.shared.pending.lock().unwrap();
            self.shared.shutdown.store(true, Ordering::Release);
            self.shared.wake.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

// A cancellable reader is necessary even after the direct child exits: wrappers
// can leave descendants holding a copy of the stdout pipe open.
fn read_output(output: &mut ChildStdout, chunk: &mut [u8]) -> std::io::Result<usize> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Pipes::PeekNamedPipe;
        let mut available = 0;
        // SAFETY: the handle is a live stdout pipe, available is writable, and
        // null optional pointers request only the count of currently ready bytes.
        let result = unsafe {
            PeekNamedPipe(
                output.as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if available == 0 {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        let count = chunk.len().min(available as usize);
        output.read(&mut chunk[..count])
    }
    #[cfg(unix)]
    {
        use std::os::fd::AsRawFd;
        let mut descriptor = libc::pollfd {
            fd: output.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: descriptor points to one initialized pollfd containing a live
        // pipe descriptor. A single reader owns the pipe, so ready data cannot
        // be consumed by another thread before this thread reads it.
        let result = unsafe { libc::poll(&mut descriptor, 1, 10) };
        if result < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if result == 0 {
            return Err(std::io::ErrorKind::WouldBlock.into());
        }
        output.read(chunk)
    }
    #[cfg(not(any(unix, windows)))]
    output.read(chunk)
}

struct Process {
    child: Child,
    input: ChildStdin,
    lines: mpsc::Receiver<String>,
    reader: Option<JoinHandle<()>>,
    reader_stop: Arc<AtomicBool>,
}
impl Process {
    fn spawn(path: &Path) -> Result<Self, String> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Cannot start {}: {e}", path.display()))?;
        let input = child.stdin.take().unwrap();
        let mut output = child.stdout.take().unwrap();
        let (sender, lines) = mpsc::sync_channel(128);
        let reader_stop = Arc::new(AtomicBool::new(false));
        let stop = reader_stop.clone();
        let reader = thread::spawn(move || {
            let mut chunk = [0; 4096];
            let mut line = Vec::with_capacity(1024);
            let mut overflow = false;
            loop {
                if stop.load(Ordering::Acquire) {
                    break;
                }
                let count = match read_output(&mut output, &mut chunk) {
                    Ok(0) => break,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(POLL);
                        continue;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                    Ok(count) => count,
                };
                for byte in &chunk[..count] {
                    if *byte == b'\n' {
                        if !overflow {
                            let mut value = String::from_utf8_lossy(&line).trim().to_owned();
                            let critical = matches!(
                                value.split_whitespace().next(),
                                Some("uciok" | "readyok" | "bestmove")
                            );
                            loop {
                                if stop.load(Ordering::Acquire) {
                                    return;
                                }
                                match sender.try_send(value) {
                                    Ok(()) => break,
                                    Err(mpsc::TrySendError::Disconnected(_)) => return,
                                    Err(mpsc::TrySendError::Full(_)) if !critical => break,
                                    Err(mpsc::TrySendError::Full(returned)) => {
                                        value = returned;
                                        thread::sleep(POLL);
                                    }
                                }
                            }
                        }
                        line.clear();
                        overflow = false;
                    } else if line.len() < 16384 {
                        line.push(*byte);
                    } else {
                        overflow = true;
                    }
                }
            }
        });
        Ok(Self {
            child,
            input,
            lines,
            reader: Some(reader),
            reader_stop,
        })
    }
    fn send(&mut self, command: &str) -> Result<(), String> {
        writeln!(self.input, "{command}")
            .and_then(|()| self.input.flush())
            .map_err(|e| format!("Engine input failed: {e}"))
    }
    fn wait_for(&mut self, shared: &Shared, token: u64, expected: &str) -> Result<bool, String> {
        let deadline = Instant::now() + HANDSHAKE;
        while Instant::now() < deadline {
            if shared.cancelled(token) {
                return Ok(false);
            }
            match self.lines.recv_timeout(POLL) {
                Ok(line) if line == expected => return Ok(true),
                Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => return Err(format!("Engine exited before {expected}")),
            }
        }
        Err(format!("Engine timed out waiting for {expected}"))
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        self.reader_stop.store(true, Ordering::Release);
        let _ = self.send("stop");
        let _ = self.send("quit");
        let deadline = Instant::now() + Duration::from_millis(100);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ if Instant::now() >= deadline => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
                _ => thread::sleep(POLL),
            }
        }
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

fn run(shared: &Shared, request: &Request) -> Result<(), String> {
    if request.fen.len() > 1024 || request.fen.contains(['\n', '\r']) {
        return Err("Invalid FEN command".into());
    }
    let mut process = Process::spawn(&request.path)?;
    process.send("uci")?;
    if !process.wait_for(shared, request.token, "uciok")? {
        return Ok(());
    }
    process.send("isready")?;
    if !process.wait_for(shared, request.token, "readyok")? {
        return Ok(());
    }
    shared.publish(request.token, Event::Ready { id: request.id });
    process.send(&format!("position fen {}", request.fen))?;
    let (command, duration) = match request.limit {
        Limit::Depth(depth) => (
            format!("go depth {}", depth.clamp(1, 128)),
            Duration::from_secs(60),
        ),
        Limit::TimeMs(ms) => {
            let ms = ms.clamp(1, 300_000);
            (
                format!("go movetime {ms}"),
                Duration::from_millis(u64::from(ms) + 2000),
            )
        }
    };
    process.send(&command)?;
    let deadline = Instant::now() + duration;
    let mut stop_deadline = None;
    loop {
        if shared.cancelled(request.token) {
            return Ok(());
        }
        if Instant::now() >= deadline && stop_deadline.is_none() {
            process.send("stop")?;
            stop_deadline = Some(Instant::now() + Duration::from_secs(1));
        }
        if stop_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return Err("Engine did not finish after stop".into());
        }
        match process.lines.recv_timeout(POLL) {
            Ok(line) => {
                if let Some(best_move) = parse_best_move(&line) {
                    shared.publish(
                        request.token,
                        Event::BestMove {
                            id: request.id,
                            best_move,
                        },
                    );
                    return Ok(());
                }
                if let Some(mut info) = parse_info(&line) {
                    info.score = info
                        .score
                        .map(|score| score.white_perspective(request.white_to_move));
                    shared.publish(
                        request.token,
                        Event::Info {
                            id: request.id,
                            info,
                        },
                    );
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(_) => return Err("Engine exited during analysis".into()),
        }
    }
}
fn is_move(value: &str) -> bool {
    let b = value.as_bytes();
    (b.len() == 4 || b.len() == 5)
        && (b'a'..=b'h').contains(&b[0])
        && (b'1'..=b'8').contains(&b[1])
        && (b'a'..=b'h').contains(&b[2])
        && (b'1'..=b'8').contains(&b[3])
        && (b.len() == 4 || b"qrbn".contains(&b[4]))
}
fn parse_best_move(line: &str) -> Option<Option<String>> {
    let mut words = line.split_whitespace();
    if words.next()? != "bestmove" {
        return None;
    }
    match words.next()? {
        "(none)" | "0000" => Some(None),
        value if is_move(value) => Some(Some(value.to_owned())),
        _ => None,
    }
}
fn parse_info(line: &str) -> Option<Info> {
    let mut words = line.split_whitespace();
    if words.next()? != "info" {
        return None;
    }
    let mut info = Info::default();
    while let Some(word) = words.next() {
        match word {
            "string" => break,
            "depth" => info.depth = words.next().and_then(|word| word.parse().ok()),
            "score" => {
                let kind = words.next();
                let value = words.next().and_then(|word| word.parse().ok());
                info.score = match (kind, value) {
                    (Some("cp"), Some(n)) => Some(Score::Cp(n)),
                    (Some("mate"), Some(n)) => Some(Score::Mate(n)),
                    _ => None,
                };
            }
            "pv" => {
                info.pv = words
                    .take(256)
                    .take_while(|word| is_move(word))
                    .map(str::to_owned)
                    .collect();
                break;
            }
            _ => {}
        }
    }
    (info.depth.is_some() || info.score.is_some() || !info.pv.is_empty()).then_some(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scores_and_parser() {
        assert_eq!(Score::Cp(20).white_perspective(false), Score::Cp(-20));
        assert_eq!(Score::Mate(-3).white_perspective(false), Score::Mate(3));
        assert_eq!(
            Score::Cp(i32::MIN).white_perspective(false),
            Score::Cp(i32::MAX)
        );
        assert_eq!(
            parse_info("info depth 12 score cp -25 nodes 44 pv e2e4 e7e5").unwrap(),
            Info {
                depth: Some(12),
                score: Some(Score::Cp(-25)),
                pv: vec!["e2e4".into(), "e7e5".into()]
            }
        );
        assert_eq!(
            parse_info("info score mate 4").unwrap().score,
            Some(Score::Mate(4))
        );
        assert!(parse_info("info score cp broken depth bad").is_none());
        assert!(parse_info("info string score mate 100").is_none());
        assert_eq!(
            parse_best_move("bestmove a7a8q ponder h8h7"),
            Some(Some("a7a8q".into()))
        );
        assert_eq!(parse_best_move("bestmove (none)"), Some(None));
        assert_eq!(parse_best_move("bestmove 0000"), Some(None));
        assert!(parse_best_move("bestmove a9a8").is_none());
        assert!(parse_best_move("bestmove").is_none());
    }
}
