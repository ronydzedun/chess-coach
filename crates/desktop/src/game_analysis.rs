// SPDX-License-Identifier: GPL-3.0-or-later
//! Sequential pre-analysis of the selected imported main line, sharing one UCI worker.
use crate::{
    App,
    analysis_cache::{Key, Result},
};
use chess_core::{Color, Game};

struct Position {
    fen: String,
    white: bool,
    result: Option<Result>,
    failed: bool,
}
pub(super) struct GameAnalysis {
    positions: Vec<Position>,
    configuration: Key,
    error: Option<String>,
}
pub(super) struct Background {
    pub id: u64,
    key: Key,
    info: engine::Info,
}
fn same_configuration(a: &Key, b: &Key) -> bool {
    a.engine_path == b.engine_path && a.options == b.options && a.limit == b.limit
}
fn merge(target: &mut engine::Info, info: engine::Info) {
    if info.depth.is_some() {
        target.depth = info.depth;
    }
    if info.score.is_some() {
        target.score = info.score;
    }
    if !info.pv.is_empty() {
        target.pv = info.pv;
    }
}
impl GameAnalysis {
    pub fn new(game: &Game, configuration: Key) -> Self {
        let mut line = game.clone();
        let positions = (0..=line.line().len())
            .map(|ply| {
                line.seek(ply).expect("validated imported line");
                Position {
                    fen: line.fen(),
                    white: line.turn() == Color::White,
                    result: None,
                    failed: false,
                }
            })
            .collect();
        Self {
            positions,
            configuration,
            error: None,
        }
    }
    pub fn configure(&mut self, key: &Key) -> bool {
        if same_configuration(&self.configuration, key) {
            return false;
        }
        self.configuration = key.clone();
        for position in &mut self.positions {
            position.result = None;
            position.failed = false;
        }
        self.error = None;
        true
    }
    pub fn resume(&mut self) {
        self.error = None;
        for position in &mut self.positions {
            position.failed = false;
        }
    }
    pub fn get(&self, key: &Key) -> Option<Result> {
        if !same_configuration(&self.configuration, key) {
            return None;
        }
        self.positions
            .iter()
            .find(|p| p.fen == key.fen)?
            .result
            .clone()
    }
    pub fn record(&mut self, key: &Key, result: Option<Result>) {
        if !same_configuration(&self.configuration, key) {
            return;
        }
        for position in self.positions.iter_mut().filter(|p| p.fen == key.fen) {
            // A failed manual refresh must not discard an earlier complete result.
            if result.is_some() || position.result.is_none() {
                position.failed = result.is_none();
                position.result = result.clone();
            }
        }
    }
    pub fn status(&self, enabled: bool) -> String {
        let completed = self.positions.iter().filter(|p| p.result.is_some()).count();
        let failed = self.positions.iter().filter(|p| p.failed).count();
        let state = if self.error.is_some() {
            "error"
        } else if completed + failed == self.positions.len() {
            "finished"
        } else if !enabled {
            "paused"
        } else {
            "running"
        };
        let mut status = format!(
            "Game analysis: {completed}/{} positions · {state}",
            self.positions.len()
        );
        if failed > 0 {
            status.push_str(&format!(" · {failed} incomplete"));
        }
        if let Some(error) = &self.error {
            status.push_str(&format!(" · {error}"));
        }
        status
    }
}
impl App {
    pub(super) fn begin_game_analysis(&mut self) {
        // Import validation bounds this storage to 2049 positions in one selected game.
        let configuration = Key {
            fen: self.game.fen(),
            engine_path: self.engine_path.trim().into(),
            options: self.appearance.engine_options(),
            limit: self
                .analysis_key()
                .map(|key| key.limit)
                .unwrap_or(engine::Limit::TimeMs(1000)),
        };
        self.game_analysis = Some(GameAnalysis::new(&self.game, configuration));
    }
    pub(super) fn resume_game_analysis(&mut self) {
        if let Some(batch) = &mut self.game_analysis {
            batch.resume();
        }
    }
    pub(super) fn pause_game_analysis_error(&mut self, error: String) {
        if let Some(batch) = &mut self.game_analysis {
            batch.error = Some(error);
        }
    }
    pub(super) fn start_background_analysis(&mut self) {
        if !self.auto_analysis
            || self.active.is_some()
            || self.background.is_some()
            || self.analysis_due.is_some()
        {
            return;
        }
        let Some(batch) = &self.game_analysis else {
            return;
        };
        if batch.error.is_some() {
            return;
        }
        if let Err(error) = self.analysis_key() {
            self.pause_game_analysis_error(error);
            return;
        }
        loop {
            let batch = self.game_analysis.as_ref().expect("selected game");
            let Some(position) = batch
                .positions
                .iter()
                .find(|p| p.result.is_none() && !p.failed)
            else {
                return;
            };
            let mut key = batch.configuration.clone();
            key.fen = position.fen.clone();
            let white = position.white;
            if let Some(result) = self.analysis_cache.get(&key) {
                self.game_analysis
                    .as_mut()
                    .unwrap()
                    .record(&key, Some(result));
                continue;
            }
            self.next_request = self.next_request.wrapping_add(1);
            self.background = Some(Background {
                id: self.next_request,
                key: key.clone(),
                info: engine::Info::default(),
            });
            self.engine.analyze_with_options(
                self.next_request,
                key.engine_path.into(),
                key.fen,
                white,
                key.limit,
                key.options,
            );
            return;
        }
    }
    pub(super) fn receive_background(&mut self, event: engine::Event) {
        match event {
            engine::Event::Info { info, .. } => {
                merge(&mut self.background.as_mut().unwrap().info, info)
            }
            engine::Event::Ready { .. } => {}
            engine::Event::BestMove { best_move, .. } => {
                let request = self.background.take().unwrap();
                let result = Result {
                    info: request.info,
                    best_move,
                };
                if let Some(batch) = &mut self.game_analysis {
                    batch.record(&request.key, Some(result.clone()));
                }
                self.analysis_cache.insert(request.key, result);
                // Never replace the open position's score with a different position.
                if self.active.is_none() {
                    self.restore_cached_analysis();
                }
            }
            engine::Event::Incomplete { .. } => {
                let request = self.background.take().unwrap();
                if let Some(batch) = &mut self.game_analysis {
                    batch.record(&request.key, None);
                }
            }
            engine::Event::Error { message, .. } => {
                self.background = None;
                self.pause_game_analysis_error(message);
            }
            engine::Event::Stopped { .. } => {
                self.background = None;
                self.pause_game_analysis_error("Search stopped; toggle analysis to retry".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Message;
    fn send(app: &mut App, message: Message) {
        let _ = app.update(message);
    }
    fn app() -> App {
        let mut app = App {
            engine_path: "/nonexistent/game-queue-fixture".into(),
            ..App::default()
        };
        let game = chess_core::pgn::parse_pgn(b"1. e4 e5 2. Nf3 *")
            .unwrap()
            .remove(0)
            .game;
        app.game.load_line(&game);
        app.begin_game_analysis();
        app.analysis_due = None;
        app
    }
    fn complete_current(app: &mut App, cp: i32) {
        app.next_request += 1;
        let id = app.next_request;
        app.active = Some((id, app.game.revision()));
        app.active_key = Some(app.analysis_key().unwrap());
        app.receive(engine::Event::Info {
            id,
            info: engine::Info {
                score: Some(engine::Score::Cp(cp)),
                depth: Some(12),
                pv: vec!["e2e4".into()],
            },
        });
        app.receive(engine::Event::BestMove {
            id,
            best_move: Some("e2e4".into()),
        });
    }
    fn finish_background(app: &mut App, cp: i32) -> Key {
        let request = app.background.as_ref().expect("background request");
        let id = request.id;
        let key = request.key.clone();
        app.receive(engine::Event::Info {
            id,
            info: engine::Info {
                score: Some(engine::Score::Cp(cp)),
                depth: Some(13),
                pv: vec!["e7e5".into()],
            },
        });
        app.receive(engine::Event::BestMove {
            id,
            best_move: Some("e7e5".into()),
        });
        key
    }
    #[test]
    fn whole_line_is_analyzed_without_moving_board_or_replacing_visible_score() {
        let mut app = app();
        let original = app.game.fen();
        complete_current(&mut app, 35);
        for cp in [-80, 90, 110] {
            app.start_background_analysis();
            let key = finish_background(&mut app, cp);
            assert_eq!(app.game.fen(), original);
            assert_eq!(app.game.ply(), 0);
            assert_eq!(app.info.score, Some(engine::Score::Cp(35)));
            assert_eq!(
                app.game_analysis
                    .as_ref()
                    .unwrap()
                    .get(&key)
                    .unwrap()
                    .info
                    .score,
                Some(engine::Score::Cp(cp))
            );
        }
        app.start_background_analysis();
        assert!(app.background.is_none());
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(true)
                .contains("4/4 positions · finished")
        );
        app.analysis_cache = crate::analysis_cache::Cache::default(); // Results survive LRU eviction.
        let requests = app.next_request;
        send(&mut app, Message::Seek(3));
        assert_eq!(app.info.score, Some(engine::Score::Cp(110)));
        assert_eq!(app.next_request, requests);
        assert!(app.analysis_due.is_none());
    }
    #[test]
    fn foreground_has_priority_and_old_background_events_cannot_contaminate_it() {
        let mut app = app();
        complete_current(&mut app, 35);
        app.start_background_analysis();
        let old = app.background.as_ref().unwrap().id;
        send(&mut app, Message::Seek(3));
        assert!(app.background.is_none());
        assert!(app.analysis_due.is_some());
        app.start_background_analysis();
        assert!(app.background.is_none());
        app.receive(engine::Event::BestMove {
            id: old,
            best_move: Some("a2a4".into()),
        });
        assert!(app.info.score.is_none());
        app.analysis_due = None;
        complete_current(&mut app, 70);
        app.start_background_analysis();
        assert_ne!(app.background.as_ref().unwrap().id, old);
        assert!(app.background.as_ref().unwrap().key.fen.contains(" b "));
        let active = app.background.as_ref().unwrap().id;
        send(&mut app, Message::Seek(0)); // Cached navigation leaves background search running.
        assert_eq!(app.background.as_ref().unwrap().id, active);
        assert_eq!(app.info.score, Some(engine::Score::Cp(35)));
    }
    #[test]
    fn off_settings_and_new_game_pause_restart_or_clear_the_queue() {
        let mut app = app();
        complete_current(&mut app, 35);
        app.start_background_analysis();
        send(&mut app, Message::ToggleAnalysis);
        assert!(app.background.is_none());
        app.start_background_analysis();
        assert!(app.background.is_none());
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(false)
                .contains("paused")
        );
        send(&mut app, Message::ToggleAnalysis);
        app.start_background_analysis();
        assert!(app.background.is_some());
        send(&mut app, Message::SearchLimit("2000".into()));
        assert!(app.background.is_none());
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .positions
                .iter()
                .all(|p| p.result.is_none())
        );
        assert_eq!(
            app.game_analysis.as_ref().unwrap().configuration.limit,
            engine::Limit::TimeMs(2000)
        );
        send(&mut app, Message::New);
        assert!(app.game_analysis.is_none() && app.background.is_none());
    }
    #[test]
    fn errors_pause_and_incomplete_results_are_not_cached_or_retried_in_a_loop() {
        let mut app = app();
        complete_current(&mut app, 35);
        app.start_background_analysis();
        let request = app.background.as_ref().unwrap();
        let (id, key) = (request.id, request.key.clone());
        app.receive(engine::Event::Incomplete {
            id,
            best_move: None,
        });
        assert!(app.game_analysis.as_ref().unwrap().get(&key).is_none());
        assert!(app.analysis_cache.get(&key).is_none());
        app.start_background_analysis();
        assert_ne!(app.background.as_ref().unwrap().key.fen, key.fen);
        let id = app.background.as_ref().unwrap().id;
        app.receive(engine::Event::Error {
            id,
            message: "Engine disappeared".into(),
        });
        let requests = app.next_request;
        for _ in 0..3 {
            app.start_background_analysis();
        }
        assert_eq!(app.next_request, requests);
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(true)
                .contains("Engine disappeared")
        );
        app.resume_game_analysis();
        app.start_background_analysis();
        assert_eq!(app.background.as_ref().unwrap().key.fen, key.fen);
    }
    #[test]
    fn incomplete_manual_refresh_keeps_the_previous_complete_game_result() {
        let mut app = app();
        complete_current(&mut app, 35);
        let key = app.analysis_key().unwrap();
        app.next_request += 1;
        let id = app.next_request;
        app.active = Some((id, app.game.revision()));
        app.active_key = Some(key.clone());
        app.receive(engine::Event::Incomplete {
            id,
            best_move: None,
        });
        assert_eq!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .get(&key)
                .unwrap()
                .info
                .score,
            Some(engine::Score::Cp(35))
        );
    }
    #[test]
    fn cached_reset_destination_cannot_leave_a_dead_background_request() {
        for load_fen in [false, true] {
            let mut app = app();
            complete_current(&mut app, 35);
            app.start_background_analysis();
            let old = app.background.as_ref().unwrap().id;
            if load_fen {
                app.fen_input = app.game.fen();
                send(&mut app, Message::LoadFen);
            } else {
                send(&mut app, Message::New);
            }
            assert!(app.background.is_none() && app.game_analysis.is_none());
            assert_eq!(app.info.score, Some(engine::Score::Cp(35)));
            assert!(app.analysis_due.is_none());
            app.receive(engine::Event::BestMove {
                id: old,
                best_move: Some("bad".into()),
            });
            assert_eq!(app.best_move.as_deref(), Some("e2e4"));
        }
    }
    #[test]
    fn setup_fen_preserves_side_and_position_count_is_bounded_by_line() {
        let games = chess_core::pgn::parse_pgn(
            b"[SetUp \"1\"]\n[FEN \"7k/8/8/8/8/8/p7/7K b - - 0 8\"]\n\n8... a1=Q+ *",
        )
        .unwrap();
        let app = app();
        let batch = GameAnalysis::new(&games[0].game, app.analysis_key().unwrap());
        assert_eq!(batch.positions.len(), 2);
        assert!(!batch.positions[0].white && batch.positions[1].white);
        assert!(batch.positions[1].fen.starts_with("7k/8/8/8/8/8/8/q6K w"));
    }
}
