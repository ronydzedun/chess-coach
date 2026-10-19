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
    engine.analyze(
        41,
        path.into(),
        "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1".into(),
        true,
        Limit::Depth(5),
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
