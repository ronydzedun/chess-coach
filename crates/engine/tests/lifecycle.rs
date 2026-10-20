// SPDX-License-Identifier: GPL-3.0-or-later
use engine::{Bound, Engine, EngineOptions, Event, Limit, Position, Score};
use std::path::PathBuf;
use std::time::{Duration, Instant};
fn fixture() -> PathBuf {
    env!("CARGO_BIN_EXE_uci-fixture").into()
}
fn install_fixture(path: &std::path::Path) {
    // Unix exec can see ETXTBSY if a concurrent child briefly inherits a copy's
    // write descriptor. A symlink executes the already-built read-only binary,
    // even when the test directory is on a separate filesystem.
    #[cfg(unix)]
    std::os::unix::fs::symlink(fixture(), path).unwrap();
    #[cfg(not(unix))]
    if std::fs::hard_link(fixture(), path).is_err() {
        std::fs::copy(fixture(), path).unwrap();
    }
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
    install_fixture(&path);
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
        install_fixture(&path);
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

#[test]
fn watchdog_completion_is_marked_incomplete_and_session_remains_reusable() {
    let directory = FixtureDirectory::new("watchdog");
    let engine = Engine::new();
    engine.analyze(
        140,
        fixture(),
        directory.position("first", "slow"),
        true,
        Limit::TimeMs(1),
    );
    let event = until(&engine, |event| {
        matches!(event, Event::BestMove { .. } | Event::Incomplete { .. })
    });
    assert_eq!(
        event,
        Event::Incomplete {
            id: 140,
            best_move: None
        }
    );
    engine.analyze(
        141,
        fixture(),
        directory.position("second", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 141);
    assert_eq!(directory.pid("first"), directory.pid("second"));
}

#[test]
fn history_is_sent_from_root_without_resetting_the_session() {
    let directory = FixtureDirectory::new("history");
    let path = directory.executable("audit");
    let engine = Engine::new();
    let root = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";
    let moves: Vec<String> = ["g1f3", "g8f6", "f3g1", "f6g8"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    for id in [150, 151] {
        engine.analyze_position_with_options(
            id,
            path.clone(),
            Position {
                fen: root.into(),
                moves: moves.clone(),
            },
            true,
            Limit::Depth(2),
            EngineOptions::default(),
        );
        best(&engine, id);
    }
    drop(engine);
    let commands = std::fs::read_to_string(path.with_extension("commands")).unwrap();
    let expected = format!("position fen {root} moves {}", moves.join(" "));
    assert_eq!(commands.lines().filter(|line| *line == expected).count(), 2);
    assert_eq!(commands.lines().filter(|line| *line == "uci").count(), 1);
    assert!(!commands.contains("ucinewgame"));
}

#[test]
fn executable_identity_is_stable_and_tracks_length_and_modified_time() {
    let directory = FixtureDirectory::new("identity");
    let path = directory.0.join("file");
    assert_eq!(engine::executable_identity(&path), "v1:unavailable");
    std::fs::write(&path, b"first").unwrap();
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(100))
        .unwrap();
    assert_eq!(engine::executable_identity(&path), "v1:5:+100000000000");
    assert_eq!(
        engine::executable_identity(&path),
        engine::executable_identity(&path)
    );
    file.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(101))
        .unwrap();
    assert_eq!(engine::executable_identity(&path), "v1:5:+101000000000");
    file.set_len(6).unwrap();
    assert!(engine::executable_identity(&path).starts_with("v1:6:"));
}

#[cfg(unix)]
#[test]
fn replacing_executable_at_the_same_path_restarts_the_process() {
    let directory = FixtureDirectory::new("replace-executable");
    let path = directory.executable("audit");
    let engine = Engine::new();
    engine.analyze(
        152,
        path.clone(),
        directory.position("first", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 152);
    let before = engine::executable_identity(&path);
    let replacement = directory.0.join("replacement");
    std::fs::copy(fixture(), &replacement).unwrap();
    // Distinguish the replacement's metadata while retaining the executable's
    // complete contents and leaving the original build artifact untouched.
    std::fs::OpenOptions::new()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1))
        .unwrap();
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&replacement, &path).unwrap();
    assert_ne!(before, engine::executable_identity(&path));
    engine.analyze(
        153,
        path.clone(),
        directory.position("second", "normal"),
        true,
        Limit::Depth(2),
    );
    best(&engine, 153);
    assert_ne!(directory.pid("first"), directory.pid("second"));
    gone(&directory.pid("first"));
    drop(engine);
    let commands = std::fs::read_to_string(path.with_extension("commands")).unwrap();
    assert_eq!(commands.lines().filter(|line| *line == "uci").count(), 2);
}

#[cfg(unix)]
#[test]
fn bare_engine_name_tracks_replacements_without_mutating_global_path() {
    const CHILD_DIRECTORY: &str = "CHESS_COACH_BARE_ENGINE_TEST_DIRECTORY";
    if let Some(directory) = std::env::var_os(CHILD_DIRECTORY) {
        let directory = FixtureDirectory(PathBuf::from(directory));
        let installed = directory.executable("audit-bare-engine");
        let bare = PathBuf::from("audit-bare-engine");
        let engine = Engine::new();
        let before = engine::executable_identity(&bare);
        assert!(before.contains("v1:path:"));
        assert!(!before.contains("unavailable"));
        engine.analyze(
            154,
            bare.clone(),
            directory.position("first", "normal"),
            true,
            Limit::Depth(2),
        );
        best(&engine, 154);
        let replacement = directory.0.join("replacement");
        std::fs::copy(fixture(), &replacement).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_modified(std::time::UNIX_EPOCH + Duration::from_secs(1))
            .unwrap();
        std::fs::remove_file(&installed).unwrap();
        std::os::unix::fs::symlink(replacement, &installed).unwrap();
        assert_ne!(before, engine::executable_identity(&bare));
        engine.analyze(
            155,
            bare,
            directory.position("second", "normal"),
            true,
            Limit::Depth(2),
        );
        best(&engine, 155);
        assert_ne!(directory.pid("first"), directory.pid("second"));
        gone(&directory.pid("first"));
        return;
    }
    // A separate test process receives the custom PATH. Parallel tests and the
    // parent process retain their complete original environment.
    let directory = FixtureDirectory::new("bare-executable");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "bare_engine_name_tracks_replacements_without_mutating_global_path",
            "--nocapture",
        ])
        .env("PATH", &directory.0)
        .env(CHILD_DIRECTORY, &directory.0)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn invalid_history_does_not_reach_process_and_valid_request_recovers() {
    let directory = FixtureDirectory::new("injection");
    let path = directory.executable("audit");
    let engine = Engine::new();
    for (index, value) in ["e2e4\nquit", "e2e4 e7e5", "e7e8k"].into_iter().enumerate() {
        let id = 160 + index as u64;
        engine.analyze_position_with_options(
            id,
            path.clone(),
            Position {
                fen: "normal".into(),
                moves: vec![value.into()],
            },
            true,
            Limit::Depth(2),
            EngineOptions::default(),
        );
        until(
            &engine,
            |event| matches!(event, Event::Error { id: actual, .. } if *actual == id),
        );
    }
    assert!(!path.with_extension("commands").exists());
    engine.analyze(170, path.clone(), "normal".into(), true, Limit::Depth(2));
    best(&engine, 170);
    drop(engine);
    let commands = std::fs::read_to_string(path.with_extension("commands")).unwrap();
    assert!(!commands.contains("go infinite"));
    assert_eq!(commands.lines().filter(|line| *line == "quit").count(), 1);
}

#[test]
fn process_bounds_follow_white_score_perspective() {
    let directory = FixtureDirectory::new("bounds");
    for (name, white, expected) in [
        ("lowerbound", true, Bound::Lower),
        ("lowerbound", false, Bound::Upper),
        ("upperbound", true, Bound::Upper),
        ("upperbound", false, Bound::Lower),
    ] {
        let path = directory
            .0
            .join(format!("{name}-{white}{}", std::env::consts::EXE_SUFFIX));
        install_fixture(&path);
        let engine = Engine::new();
        engine.analyze(180, path, "normal".into(), white, Limit::Depth(2));
        let event = until(&engine, |event| {
            matches!(event, Event::Info { id: 180, .. })
        });
        let Event::Info { info, .. } = event else {
            unreachable!()
        };
        assert_eq!(info.bound, expected);
        assert_eq!(info.score, Some(Score::Cp(if white { 42 } else { -42 })));
        best(&engine, 180);
    }
}

#[test]
fn stockfish_history_recognizes_repetition_from_a_setup_fen() {
    let Some(path) = std::env::var_os("STOCKFISH_EXECUTABLE") else {
        eprintln!("SKIP real Stockfish history: set STOCKFISH_EXECUTABLE to opt in");
        return;
    };
    let engine = Engine::new();
    let path = PathBuf::from(path);
    let root = "6nk/8/8/8/8/8/8/KQ4N1 w - - 0 1";
    let history = Position {
        fen: root.into(),
        moves: ["g1f3", "g8f6", "f3g1", "f6g8", "g1f3", "g8f6", "f3g1"]
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    // Black can repeat f6g8 for a draw despite White's extra queen. The same
    // board without the played history is winning for White.
    let board_only = Position {
        fen: "7k/8/5n2/8/8/8/8/KQ4N1 b - - 7 4".into(),
        moves: Vec::new(),
    };
    let mut results = Vec::new();
    for (id, position) in [(190, history), (191, board_only)] {
        engine.analyze_position_with_options(
            id,
            path.clone(),
            position,
            false,
            Limit::Depth(10),
            EngineOptions::default(),
        );
        let deadline = Instant::now() + Duration::from_secs(8);
        let mut score = None;
        loop {
            if let Some(event) = engine.try_recv() {
                match event {
                    Event::Info { id: actual, info }
                        if actual == id && info.bound == Bound::Exact =>
                    {
                        if let Some(value) = info.score {
                            score = Some(value);
                        }
                    }
                    Event::BestMove {
                        id: actual,
                        best_move,
                    } if actual == id => {
                        assert!(best_move.is_some());
                        if id == 190 {
                            assert_eq!(best_move.as_deref(), Some("f6g8"));
                        }
                        results.push(score.expect("Stockfish produced an exact score"));
                        break;
                    }
                    Event::Error { message, .. } => panic!("{message}"),
                    _ => {}
                }
            }
            assert!(
                Instant::now() < deadline,
                "Stockfish history search timed out"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    assert_eq!(results[0], Score::Cp(0));
    assert!(
        matches!(results[1], Score::Cp(n) if n > 200)
            || matches!(results[1], Score::Mate(n) if n > 0)
    );
}
