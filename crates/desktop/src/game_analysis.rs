// SPDX-License-Identifier: GPL-3.0-or-later
//! Sequential pre-analysis of the selected imported main line, sharing one UCI worker.
use crate::{
    App,
    analysis_cache::{Key, Result},
};

pub(super) use crate::analysis_plan::GameAnalysis;
use crate::analysis_plan::same_configuration;
pub(super) struct Background {
    pub id: u64,
    key: Key,
    info: engine::Info,
}
fn merge(target: &mut engine::Info, info: engine::Info) {
    if info.bound != engine::Bound::Exact {
        return;
    }
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
impl App {
    // Navigation must not throw away a search that belongs to the imported game.
    // Its result is routed by FEN, independently of the currently displayed revision.
    pub(super) fn retain_game_search(&mut self) {
        let retain = self.auto_analysis
            && self.active_key.as_ref().is_some_and(|key| {
                self.game_analysis
                    .as_ref()
                    .is_some_and(|batch| same_configuration(&batch.configuration, key))
            });
        if retain {
            let (id, _) = self.active.take().expect("active search key");
            self.background = Some(Background {
                id,
                key: self.active_key.take().unwrap(),
                info: self.info.clone(),
            });
            self.analyzing = false;
        } else if self.active.is_some() {
            self.stop_analysis();
        }
    }
    pub(super) fn begin_game_analysis(&mut self) {
        // Import validation bounds this storage to 2049 positions in one selected game.
        let configuration = Key {
            fen: self.game.fen(),
            position: {
                let (fen, moves) = self.game.analysis_position();
                engine::Position { fen, moves }
            },
            engine_path: self.engine_path.trim().into(),
            engine_identity: engine::executable_identity(std::path::Path::new(
                self.engine_path.trim(),
            )),
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
        if self.library_pending.is_some()
            || !self.auto_analysis
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
        let key = match self.analysis_key() {
            Ok(key) => key,
            Err(error) => {
                self.pause_game_analysis_error(error);
                return;
            }
        };
        if self.game_analysis.as_mut().unwrap().configure(&key) {
            self.schedule_analysis();
            self.load_saved_analysis();
            return;
        }
        loop {
            let batch = self.game_analysis.as_ref().expect("selected game");
            let Some((key, white)) = batch.next() else {
                return;
            };
            if let Some(result) = self.analysis_cache.get(&key) {
                self.game_analysis
                    .as_mut()
                    .unwrap()
                    .record(&key, Some(result.clone()));
                self.save_analysis_result(&key, &result);
                continue;
            }
            self.next_request = self.next_request.wrapping_add(1);
            self.background = Some(Background {
                id: self.next_request,
                key: key.clone(),
                info: engine::Info::default(),
            });
            self.engine.analyze_position_with_options(
                self.next_request,
                key.engine_path.into(),
                key.position,
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
                if result.info.score.is_none() {
                    if let Some(batch) = &mut self.game_analysis {
                        batch.record(&request.key, None);
                    }
                    return;
                }
                if let Some(batch) = &mut self.game_analysis {
                    batch.record(&request.key, Some(result.clone()));
                }
                self.save_analysis_result(&request.key, &result);
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
            search_limit: "100".into(),
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
                bound: engine::Bound::Exact,
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
                bound: engine::Bound::Exact,
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
    fn navigation_finishes_inflight_search_then_prioritizes_the_open_position() {
        let mut app = app();
        complete_current(&mut app, 35);
        app.start_background_analysis();
        let id = app.background.as_ref().unwrap().id;
        for ply in [3, 2, 0, 1, 3] {
            send(&mut app, Message::Seek(ply));
            assert_eq!(app.background.as_ref().unwrap().id, id);
            app.analysis_due = Some(std::time::Instant::now());
            app.engine.reset(); // This test injects UCI events explicitly.
            send(&mut app, Message::Poll);
            assert_eq!(app.background.as_ref().unwrap().id, id);
        }
        let key = finish_background(&mut app, -40);
        assert!(app.game_analysis.as_ref().unwrap().get(&key).is_some());
        assert!(app.info.score.is_none()); // This is a different position.
        app.engine.reset();
        send(&mut app, Message::Poll);
        assert_eq!(app.active_key.as_ref().unwrap().fen, app.game.fen());
        let foreground = app.active.unwrap().0;
        app.receive(engine::Event::Info {
            id: foreground,
            info: engine::Info {
                score: Some(engine::Score::Cp(70)),
                ..Default::default()
            },
        });
        send(&mut app, Message::Seek(0));
        assert_eq!(app.info.score, Some(engine::Score::Cp(35)));
        assert_eq!(app.background.as_ref().unwrap().id, foreground);
        app.receive(engine::Event::BestMove {
            id: foreground,
            best_move: Some("b8c6".into()),
        });
        send(&mut app, Message::Seek(3));
        assert_eq!(app.info.score, Some(engine::Score::Cp(70)));
        assert!(app.analysis_due.is_none());
        app.start_background_analysis();
        finish_background(&mut app, 90);
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(true)
                .contains("4/4 positions · finished")
        );
    }
    #[test]
    fn editing_limit_cancels_foreground_without_caching_cleared_info() {
        let mut app = app();
        app.start_analysis(false);
        let id = app.active.unwrap().0;
        app.receive(engine::Event::Info {
            id,
            info: engine::Info {
                score: Some(engine::Score::Cp(35)),
                ..Default::default()
            },
        });
        send(&mut app, Message::SearchLimit(String::new()));
        assert!(app.active.is_none() && app.background.is_none());
        app.receive(engine::Event::BestMove {
            id,
            best_move: Some("e2e4".into()),
        });
        send(&mut app, Message::SearchLimit("1000".into()));
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .get(&app.analysis_key().unwrap())
                .is_none()
        );
        assert!(app.analysis_due.is_some());
    }
    #[test]
    fn resource_changes_retain_completed_scores_and_resume_only_pending_positions() {
        let mut app = app();
        complete_current(&mut app, 35);
        app.start_background_analysis();
        finish_background(&mut app, -40);
        app.start_background_analysis();
        let old = app.background.as_ref().unwrap().id;
        app.appearance.engine_threads = 2;
        app.appearance.engine_hash_mib = 128;
        app.engine.reset();
        let path = app.engine_path.clone();
        send(&mut app, Message::EnginePath(path));
        assert_eq!(app.info.score, Some(engine::Score::Cp(35)));
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(true)
                .contains("2/4")
        );
        assert!(app.analysis_due.is_none());
        app.receive(engine::Event::BestMove {
            id: old,
            best_move: None,
        });
        app.start_background_analysis();
        let request = app.background.as_ref().unwrap();
        assert_eq!(request.key.options, app.appearance.engine_options());
        assert_eq!(
            request.key.fen,
            app.game_analysis.as_ref().unwrap().positions[2].fen
        );
        finish_background(&mut app, 70);
        app.start_background_analysis();
        finish_background(&mut app, 90);
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(true)
                .contains("4/4")
        );
        send(&mut app, Message::Seek(1));
        assert_eq!(app.info.score, Some(engine::Score::Cp(-40)));
        send(&mut app, Message::SearchMode(crate::SearchMode::Depth));
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .positions
                .iter()
                .all(|p| p.result.is_none())
        );
        assert!(app.analysis_due.is_some());
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
    fn long_game_stockfish_opt_in_completes_despite_navigation_and_lru_eviction() {
        let Some(path) = std::env::var_os("STOCKFISH_EXECUTABLE") else {
            return;
        };
        let mut pgn = String::new();
        for pair in 0..75 {
            pgn.push_str(&format!(
                "{}. Nf3 Nf6 {}. Ng1 Ng8 ",
                pair * 2 + 1,
                pair * 2 + 2
            ));
        }
        pgn.push('*');
        let game = chess_core::pgn::parse_pgn(pgn.as_bytes())
            .unwrap()
            .remove(0)
            .game;
        let mut app = App {
            engine_path: path.to_string_lossy().into(),
            search_limit: "10".into(),
            ..App::default()
        };
        app.game.load_line(&game);
        app.begin_game_analysis();
        app.start_analysis(false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        let mut navigation = 0;
        loop {
            send(&mut app, Message::Seek(navigation % 301));
            navigation += 37;
            send(&mut app, Message::Poll);
            let batch = app.game_analysis.as_ref().unwrap();
            if batch.positions.iter().all(|p| p.result.is_some()) {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{}",
                batch.status(true)
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(app.game_analysis.as_ref().unwrap().positions.len(), 301);
        let requests = app.next_request;
        for ply in 0..301 {
            send(&mut app, Message::Seek(ply));
            assert!(app.info.score.is_some(), "missing score at ply {ply}");
            assert!(app.analysis_due.is_none());
        }
        assert_eq!(app.next_request, requests);
        assert_eq!(app.game.line().len(), 300);
    }
    #[test]
    fn report_scores_keep_gaps_and_white_relative_mate_signs() {
        let mut app = app();
        complete_current(&mut app, 125);
        let batch = app.game_analysis.as_mut().unwrap();
        for (index, score) in [
            (1, engine::Score::Mate(0)),
            (2, engine::Score::Mate(0)),
            (3, engine::Score::Mate(-3)),
        ] {
            let mut key = batch.configuration.clone();
            key.fen = batch.positions[index].fen.clone();
            key.position = batch.positions[index].position.clone();
            batch.record(
                &key,
                Some(Result {
                    info: engine::Info {
                        score: Some(score),
                        ..Default::default()
                    },
                    best_move: None,
                }),
            );
        }
        let (scores, selected) = batch.report_data(&app.game.fen());
        assert_eq!(
            scores,
            vec![Some(1.25), Some(10.0), Some(-10.0), Some(-10.0)]
        );
        assert_eq!(selected, Some(0));
        batch.positions[2].result = None;
        assert_eq!(batch.report_data("different working branch").0[2], None);
        assert_eq!(batch.report_data("different working branch").1, None);
    }
    #[test]
    fn progress_counts_only_completed_positions_and_keeps_pause_error_and_gaps_visible() {
        let mut app = app();
        assert_eq!(app.game_analysis.as_ref().unwrap().progress(), (0, 0, 4));
        complete_current(&mut app, 35);
        assert_eq!(app.game_analysis.as_ref().unwrap().progress(), (1, 0, 4));
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(false)
                .contains("paused")
        );
        app.start_background_analysis();
        let id = app.background.as_ref().unwrap().id;
        app.receive(engine::Event::Incomplete {
            id,
            best_move: None,
        });
        assert_eq!(app.game_analysis.as_ref().unwrap().progress(), (1, 1, 4));
        app.pause_game_analysis_error("fixture failure".into());
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(true)
                .contains("fixture failure")
        );
        app.resume_game_analysis();
        for cp in [40, 50, 60] {
            app.start_background_analysis();
            finish_background(&mut app, cp);
        }
        let batch = app.game_analysis.as_ref().unwrap();
        assert_eq!(batch.progress(), (4, 0, 4));
        assert!(batch.status(true).contains("finished"));
        send(&mut app, Message::SearchLimit("2000".into()));
        assert_eq!(app.game_analysis.as_ref().unwrap().progress(), (0, 0, 8));
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
