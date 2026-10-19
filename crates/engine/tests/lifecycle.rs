// SPDX-License-Identifier: GPL-3.0-or-later
use engine::{Engine, Event, Limit, Score};
use std::path::PathBuf;
use std::time::{Duration, Instant};
fn fixture() -> PathBuf {
    env!("CARGO_BIN_EXE_uci-fixture").into()
}
fn until(engine: &Engine, predicate: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(event) = engine.try_recv() {
            if predicate(&event) {
                return event;
            }
            assert!(!matches!(event, Event::Error { .. }), "{event:?}");
        }
        assert!(Instant::now() < deadline, "No expected engine event");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn handshake_search_and_white_scores() {
    let engine = Engine::new();
    engine.analyze(1, fixture(), "normal".into(), false, Limit::Depth(4));
    until(&engine, |event| matches!(event, Event::Ready { id: 1 }));
    let info = until(&engine, |event| matches!(event, Event::Info { id: 1, .. }));
    assert!(matches!(info, Event::Info { info, .. } if info.score == Some(Score::Cp(-42))));
    assert_eq!(
        until(&engine, |event| matches!(event, Event::BestMove { .. })),
        Event::BestMove {
            id: 1,
            best_move: Some("e2e4".into())
        }
    );
}
#[test]
fn missing_executable_and_command_injection() {
    let engine = Engine::new();
    engine.analyze(
        2,
        PathBuf::from("/nonexistent/chess-coach-engine"),
        "normal".into(),
        true,
        Limit::Depth(2),
    );
    until(&engine, |event| matches!(event, Event::Error { id: 2, .. }));
    engine.analyze(
        3,
        fixture(),
        "normal\ngo infinite".into(),
        true,
        Limit::Depth(2),
    );
    until(&engine, |event| matches!(event, Event::Error { id: 3, .. }));
}
#[test]
fn stop_and_replacement_discard_old_events() {
    let engine = Engine::new();
    engine.analyze(10, fixture(), "slow".into(), true, Limit::Depth(20));
    until(&engine, |event| matches!(event, Event::Info { id: 10, .. }));
    engine.stop();
    assert_eq!(engine.try_recv(), Some(Event::Stopped { id: 10 }));
    engine.analyze(11, fixture(), "slow".into(), true, Limit::Depth(20));
    until(&engine, |event| matches!(event, Event::Info { id: 11, .. }));
    engine.analyze(12, fixture(), "normal".into(), true, Limit::TimeMs(20));
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(event) = engine.try_recv() {
            assert!(
                matches!(
                    event,
                    Event::Ready { id: 12 }
                        | Event::Info { id: 12, .. }
                        | Event::BestMove { id: 12, .. }
                ),
                "{event:?}"
            );
            if matches!(event, Event::BestMove { .. }) {
                break;
            }
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn drop_reaps_active_process() {
    let path = std::env::temp_dir().join(format!("chess-coach-engine-pid-{}", std::process::id()));
    let engine = Engine::new();
    engine.analyze(
        21,
        fixture(),
        format!("pid:{}", path.display()),
        true,
        Limit::Depth(30),
    );
    until(&engine, |event| matches!(event, Event::Info { id: 21, .. }));
    let pid = std::fs::read_to_string(&path).unwrap();
    let start = Instant::now();
    drop(engine);
    assert!(start.elapsed() < Duration::from_secs(2));
    #[cfg(target_os = "linux")]
    assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
    std::fs::remove_file(path).unwrap();
}
#[test]
fn cancel_unresponsive_handshake() {
    let directory =
        std::env::temp_dir().join(format!("chess-coach-engine-silent-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("silent{}", std::env::consts::EXE_SUFFIX));
    std::fs::copy(fixture(), &path).unwrap();
    let engine = Engine::new();
    engine.analyze(31, path, "normal".into(), true, Limit::Depth(2));
    std::thread::sleep(Duration::from_millis(50));
    let start = Instant::now();
    drop(engine);
    assert!(start.elapsed() < Duration::from_secs(2));
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn stockfish_opt_in() {
    let Some(path) = std::env::var_os("STOCKFISH_EXECUTABLE") else {
        eprintln!("SKIP real Stockfish: set STOCKFISH_EXECUTABLE to opt in");
        return;
    };
    let engine = Engine::new();
    let path: PathBuf = path.into();
    let options = engine::EngineOptions {
        threads: 2,
        hash_mib: 32,
    };
    engine.analyze_with_options(
        41,
        path.clone(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
        true,
        Limit::Depth(5),
        options,
    );
    until(&engine, |event| {
        matches!(
            event,
            Event::BestMove {
                id: 41,
                best_move: Some(_)
            }
        )
    });
    engine.analyze_with_options(
        42,
        path,
        "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2".into(),
        true,
        Limit::Depth(5),
        options,
    );
    until(&engine, |event| {
        matches!(
            event,
            Event::BestMove {
                id: 42,
                best_move: Some(_)
            }
        )
    });
}

#[test]
fn terminal_move_survives_output_flood() {
    let engine = Engine::new();
    engine.analyze(51, fixture(), "flood".into(), true, Limit::TimeMs(20));
    until(&engine, |event| {
        matches!(
            event,
            Event::BestMove {
                id: 51,
                best_move: Some(_)
            }
        )
    });
}

struct FixtureDirectory(PathBuf);
impl FixtureDirectory {
    fn new(label: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "chess-coach-{label}-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn executable(&self, mode: &str) -> PathBuf {
        let path = self
            .0
            .join(format!("{mode}{}", std::env::consts::EXE_SUFFIX));
        // Reusing the built inode avoids executing a freshly written file on
        // filesystems that briefly retain a write lease after copy completes.
        if std::fs::hard_link(fixture(), &path).is_err() {
            std::fs::copy(fixture(), &path).unwrap();
        }
        path
    }
    fn position(&self, name: &str, behavior: &str) -> String {
        format!("record:{}|{behavior}", self.0.join(name).display())
    }
    fn pid(&self, name: &str) -> String {
        std::fs::read_to_string(self.0.join(name)).unwrap()
    }
}
impl Drop for FixtureDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn best(engine: &Engine, id: u64) {
    until(
        engine,
        |event| matches!(event, Event::BestMove { id: actual, best_move: Some(value) } if *actual == id && value == "e2e4"),
    );
}
fn gone(pid: &str) {
    #[cfg(target_os = "linux")]
    {
        let deadline = Instant::now() + Duration::from_secs(2);
        while std::path::Path::new(&format!("/proc/{pid}")).exists() {
            assert!(Instant::now() < deadline, "Process {pid} still alive");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = pid;
}
#[test]
fn completed_and_cancelled_searches_reuse_process_and_discard_late_results() {
    let directory = FixtureDirectory::new("reuse");
    let engine = Engine::new();
    engine.analyze(
        60,
        fixture(),
        directory.position("first", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 60);
    engine.analyze(
        61,
        fixture(),
        directory.position("second", "late"),
        true,
        Limit::Depth(2),
    );
    until(&engine, |event| matches!(event, Event::Info { id: 61, .. }));
    engine.stop();
    engine.analyze(
        62,
        fixture(),
        directory.position("third", "normal"),
        true,
        Limit::Depth(2),
    );
    loop {
        match until(&engine, |_| true) {
            Event::Ready { id: 62 } => {}
            Event::Info { id: 62, info } => assert_eq!(info.score, Some(Score::Cp(42))),
            Event::BestMove { id: 62, best_move } => {
                assert_eq!(best_move.as_deref(), Some("e2e4"));
                break;
            }
            event => panic!("Unexpected old search event {event:?}"),
        }
    }
    assert_eq!(directory.pid("first"), directory.pid("second"));
    assert_eq!(directory.pid("first"), directory.pid("third"));
    let pid = directory.pid("third");
    std::thread::sleep(Duration::from_millis(50));
    drop(engine);
    gone(&pid);
}
#[test]
fn options_are_ordered_and_reapplied_only_after_restart() {
    use engine::EngineOptions;
    let directory = FixtureDirectory::new("options");
    let executable = directory.executable("optionflood-audit");
    let engine = Engine::new();
    let options = EngineOptions {
        threads: 8,
        hash_mib: 256,
    };
    engine.analyze_with_options(
        70,
        executable.clone(),
        directory.position("first", "normal"),
        true,
        Limit::Depth(2),
        options,
    );
    best(&engine, 70);
    engine.analyze_with_options(
        71,
        executable.clone(),
        directory.position("second", "normal"),
        true,
        Limit::Depth(2),
        options,
    );
    best(&engine, 71);
    assert_eq!(directory.pid("first"), directory.pid("second"));
    let commands = std::fs::read_to_string(executable.with_extension("commands")).unwrap();
    assert!(
        commands.starts_with(
            "uci\nsetoption name Threads value 8\nsetoption name Hash value 256\nisready\n"
        ),
        "{commands}"
    );
    assert_eq!(commands.matches("setoption name").count(), 2);
    assert!(!commands.contains("ucinewgame"));
    engine.analyze_with_options(
        72,
        executable,
        directory.position("third", "normal"),
        true,
        Limit::Depth(2),
        EngineOptions {
            threads: 2,
            hash_mib: 32,
        },
    );
    best(&engine, 72);
    assert_ne!(directory.pid("first"), directory.pid("third"));
    gone(&directory.pid("first"));
}
#[test]
fn option_validation_and_generic_legacy_defaults() {
    use engine::EngineOptions;
    assert!(EngineOptions::default().validate().is_ok());
    assert!(
        EngineOptions {
            threads: 1024,
            hash_mib: 4096
        }
        .validate()
        .is_ok()
    );
    let directory = FixtureDirectory::new("validation");
    let engine = Engine::new();
    let cases = [
        (
            EngineOptions {
                threads: 0,
                hash_mib: 16,
            },
            fixture(),
            "Threads must",
        ),
        (
            EngineOptions {
                threads: 1025,
                hash_mib: 16,
            },
            fixture(),
            "Threads must",
        ),
        (
            EngineOptions {
                threads: 1,
                hash_mib: 0,
            },
            fixture(),
            "Hash must",
        ),
        (
            EngineOptions {
                threads: 1,
                hash_mib: 4097,
            },
            fixture(),
            "Hash must",
        ),
        (
            EngineOptions {
                threads: 3,
                hash_mib: 16,
            },
            directory.executable("limited-threads"),
            "Threads supports",
        ),
        (
            EngineOptions {
                threads: 1,
                hash_mib: 33,
            },
            directory.executable("limited-hash"),
            "Hash supports",
        ),
        (
            EngineOptions {
                threads: 2,
                hash_mib: 16,
            },
            directory.executable("nooptions-threads"),
            "does not advertise Threads",
        ),
        (
            EngineOptions {
                threads: 1,
                hash_mib: 32,
            },
            directory.executable("nooptions-hash"),
            "does not advertise Hash",
        ),
        (
            EngineOptions::default(),
            directory.executable("malformed"),
            "invalid Threads spin",
        ),
    ];
    for (index, (options, path, message)) in cases.into_iter().enumerate() {
        let id = 80 + index as u64;
        engine.analyze_with_options(id, path, "normal".into(), true, Limit::Depth(2), options);
        let event = until(&engine, |event| matches!(event, Event::Error { .. }));
        assert!(matches!(event, Event::Error { message: value, .. } if value.contains(message)));
    }
    engine.analyze(
        99,
        directory.executable("nooptions-default"),
        "normal".into(),
        true,
        Limit::Depth(2),
    );
    best(&engine, 99);
}
#[test]
fn unresponsive_stop_restarts_and_reaps_process() {
    let directory = FixtureDirectory::new("unresponsive");
    let engine = Engine::new();
    engine.analyze(
        100,
        fixture(),
        directory.position("first", "unresponsive"),
        true,
        Limit::Depth(2),
    );
    until(&engine, |event| {
        matches!(event, Event::Info { id: 100, .. })
    });
    let start = Instant::now();
    engine.analyze(
        101,
        fixture(),
        directory.position("second", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 101);
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_ne!(directory.pid("first"), directory.pid("second"));
    gone(&directory.pid("first"));
}
#[test]
fn reset_releases_idle_hash_and_next_request_starts_fresh() {
    let directory = FixtureDirectory::new("reset");
    let engine = Engine::new();
    engine.analyze(
        110,
        fixture(),
        directory.position("first", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 110);
    engine.reset();
    gone(&directory.pid("first"));
    engine.analyze(
        111,
        fixture(),
        directory.position("second", "slow"),
        true,
        Limit::Depth(2),
    );
    until(&engine, |event| {
        matches!(event, Event::Info { id: 111, .. })
    });
    engine.reset();
    engine.analyze(
        112,
        fixture(),
        directory.position("third", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 112);
    assert_ne!(directory.pid("first"), directory.pid("second"));
    assert_ne!(directory.pid("second"), directory.pid("third"));
    gone(&directory.pid("second"));
}
#[test]
fn idle_exit_and_idle_output_do_not_prevent_next_search() {
    let directory = FixtureDirectory::new("idle");
    let engine = Engine::new();
    engine.analyze(
        120,
        fixture(),
        directory.position("first", "exit"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 120);
    std::thread::sleep(Duration::from_millis(100));
    engine.analyze(
        121,
        fixture(),
        directory.position("second", "idleflood"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 121);
    assert_ne!(directory.pid("first"), directory.pid("second"));
    std::thread::sleep(Duration::from_millis(100));
    engine.analyze(
        122,
        fixture(),
        directory.position("third", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 122);
    assert_eq!(directory.pid("second"), directory.pid("third"));
}
#[cfg(unix)]
#[test]
fn inherited_stdout_does_not_hold_worker_open() {
    let directory = FixtureDirectory::new("wrapper");
    let engine = Engine::new();
    use std::os::unix::fs::PermissionsExt;
    let wrapper = directory.0.join("inherited");
    std::fs::write(&wrapper, "#!/bin/sh\nsleep 3 &\nexit 0\n").unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    engine.analyze(130, wrapper, "normal".into(), true, Limit::Depth(2));
    std::thread::sleep(Duration::from_millis(80));
    let start = Instant::now();
    drop(engine);
    assert!(start.elapsed() < Duration::from_secs(1));
}
