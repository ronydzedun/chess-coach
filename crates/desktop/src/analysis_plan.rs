// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure two-pass scheduling and retained scores; independent of Iced and process I/O.
use crate::analysis_cache::{Key, Result};
use chess_core::{Color, Game};
pub(super) struct Position {
    pub(super) fen: String,
    pub(super) position: engine::Position,
    pub(super) white: bool,
    pub(super) result: Option<Result>,
    pub(super) failed: bool,
    preview: Option<Result>,
    preview_failed: bool,
}
pub(super) struct GameAnalysis {
    pub(super) positions: Vec<Position>,
    pub(super) configuration: Key,
    pub(super) error: Option<String>,
}
pub(super) fn same_configuration(a: &Key, b: &Key) -> bool {
    a.engine_path == b.engine_path && a.engine_identity == b.engine_identity && a.limit == b.limit
}
impl GameAnalysis {
    pub fn new(game: &Game, configuration: Key) -> Self {
        let mut line = game.clone();
        let positions = (0..=line.line().len())
            .map(|ply| {
                line.seek(ply).expect("validated imported line");
                Position {
                    fen: line.fen(),
                    position: {
                        let (fen, moves) = line.analysis_position();
                        engine::Position { fen, moves }
                    },
                    white: line.turn() == Color::White,
                    result: None,
                    failed: false,
                    preview: None,
                    preview_failed: false,
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
            // Keep scores, but use updated resources for every subsequent search.
            self.configuration.options = key.options;
            return false;
        }
        self.configuration = key.clone();
        for position in &mut self.positions {
            position.result = None;
            position.failed = false;
            position.preview_failed = false;
            position.preview = None;
        }
        self.error = None;
        true
    }
    pub fn resume(&mut self) {
        self.error = None;
        for position in &mut self.positions {
            position.failed = false;
            position.preview_failed = false;
        }
    }
    pub fn get(&self, key: &Key) -> Option<Result> {
        if !same_configuration(&self.configuration, key) {
            return None;
        }
        self.positions
            .iter()
            .find(|p| p.fen == key.fen && p.position == key.position)?
            .result
            .clone()
    }
    pub fn record(&mut self, key: &Key, result: Option<Result>) {
        let preview = self.preview_limit() == Some(key.limit);
        if self.configuration.engine_path != key.engine_path
            || self.configuration.engine_identity != key.engine_identity
            || (!preview && self.configuration.limit != key.limit)
        {
            return;
        }
        for position in self
            .positions
            .iter_mut()
            .filter(|p| p.fen == key.fen && p.position == key.position)
        {
            if preview {
                position.preview_failed = result.is_none();
                if result.is_some() {
                    position.preview = result.clone();
                }
            } else if result.is_some() || position.result.is_none() {
                position.failed = result.is_none();
                position.result = result.clone();
            }
        }
    }
    pub fn preview_limit(&self) -> Option<engine::Limit> {
        match self.configuration.limit {
            engine::Limit::TimeMs(ms) if ms > 200 => Some(engine::Limit::TimeMs(200)),
            engine::Limit::Depth(depth) if depth > 8 => Some(engine::Limit::Depth(8)),
            _ => None,
        }
    }
    pub fn next(&self) -> Option<(Key, bool)> {
        if let Some(limit) = self.preview_limit()
            && let Some((ply, position)) = self.positions.iter().enumerate().find(|(_, p)| {
                p.result.is_none() && !p.failed && p.preview.is_none() && !p.preview_failed
            })
        {
            let mut key = self.key_at(ply)?;
            key.limit = limit;
            return Some((key, position.white));
        }
        // Revisit tactical swings first, then deepen all remaining positions to the requested limit.
        let urgency = |ply: usize| -> i64 {
            let score = |index: usize| {
                self.positions[index]
                    .preview
                    .as_ref()
                    .and_then(|r| r.info.score)
            };
            match (score(ply), ply.checked_sub(1).and_then(score)) {
                (Some(engine::Score::Mate(_)), _) => i64::MAX,
                (Some(engine::Score::Cp(a)), Some(engine::Score::Cp(b))) => {
                    (i64::from(a) - i64::from(b)).abs()
                }
                _ => 0,
            }
        };
        let (ply, position) = self
            .positions
            .iter()
            .enumerate()
            .filter(|(_, p)| p.result.is_none() && !p.failed)
            .max_by_key(|(ply, _)| (urgency(*ply), std::cmp::Reverse(*ply)))?;
        Some((self.key_at(ply)?, position.white))
    }
    pub fn key_at(&self, ply: usize) -> Option<Key> {
        let position = self.positions.get(ply)?;
        let mut key = self.configuration.clone();
        key.fen = position.fen.clone();
        key.position = position.position.clone();
        Some(key)
    }
    pub fn ply_for(&self, key: &Key) -> Option<usize> {
        if !same_configuration(&self.configuration, key) {
            return None;
        }
        self.positions
            .iter()
            .position(|p| p.fen == key.fen && p.position == key.position)
    }
    pub fn report_data(&self, current_fen: &str) -> (Vec<Option<f32>>, Option<usize>) {
        let scores = self
            .positions
            .iter()
            .map(|position| {
                position
                    .result
                    .as_ref()
                    .or(position.preview.as_ref())?
                    .info
                    .score
                    .map(|score| match score {
                        engine::Score::Cp(cp) => cp as f32 / 100.0,
                        engine::Score::Mate(0) => {
                            if position.white {
                                -10.0
                            } else {
                                10.0
                            }
                        }
                        engine::Score::Mate(n) => {
                            if n > 0 {
                                10.0
                            } else {
                                -10.0
                            }
                        }
                    })
            })
            .collect();
        (
            scores,
            self.positions.iter().position(|p| p.fen == current_fen),
        )
    }
    pub fn progress(&self) -> (usize, usize, usize) {
        let completed = self.positions.iter().filter(|p| p.result.is_some()).count();
        let failed = self.positions.iter().filter(|p| p.failed).count();
        if self.preview_limit().is_some() {
            let previews = self
                .positions
                .iter()
                .filter(|p| p.preview.is_some() || p.result.is_some())
                .count();
            (completed + previews, failed, self.positions.len() * 2)
        } else {
            (completed, failed, self.positions.len())
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
        if self.preview_limit().is_some() {
            let previews = self
                .positions
                .iter()
                .filter(|p| p.preview.is_some() || p.result.is_some())
                .count();
            status.push_str(&format!(
                " · quick pass {previews}/{}",
                self.positions.len()
            ));
        }
        if failed > 0 {
            status.push_str(&format!(" · {failed} incomplete"));
        }
        if let Some(error) = &self.error {
            status.push_str(&format!(" · {error}"));
        }
        status
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn result(cp: i32) -> Result {
        Result {
            info: engine::Info {
                score: Some(engine::Score::Cp(cp)),
                ..Default::default()
            },
            best_move: None,
        }
    }
    fn plan() -> GameAnalysis {
        let game = chess_core::pgn::parse_pgn(b"1. e4 e5 2. Nf3 *")
            .unwrap()
            .remove(0)
            .game;
        let (fen, moves) = game.analysis_position();
        GameAnalysis::new(
            &game,
            Key {
                fen: game.fen(),
                position: engine::Position { fen, moves },
                engine_path: "fixture".into(),
                engine_identity: "fixture".into(),
                options: engine::EngineOptions::default(),
                limit: engine::Limit::TimeMs(1000),
            },
        )
    }
    #[test]
    fn quick_pass_fills_graph_then_swing_is_deepened_first_without_claiming_final_cache_hit() {
        let mut plan = plan();
        for (ply, cp) in [0, 20, 900, 900].into_iter().enumerate() {
            let (key, _) = plan.next().unwrap();
            assert_eq!(key.position.moves.len(), ply);
            assert_eq!(key.limit, engine::Limit::TimeMs(200));
            plan.record(&key, Some(result(cp)));
        }
        assert_eq!(plan.progress(), (4, 0, 8));
        assert_eq!(
            plan.report_data(&plan.configuration.fen).0,
            vec![Some(0.0), Some(0.2), Some(9.0), Some(9.0)]
        );
        assert!(plan.get(&plan.key_at(2).unwrap()).is_none());
        let (key, _) = plan.next().unwrap();
        assert_eq!(key.position.moves.len(), 2);
        assert_eq!(key.limit, engine::Limit::TimeMs(1000));
        plan.record(&key, Some(result(700)));
        for _ in 0..3 {
            let (key, _) = plan.next().unwrap();
            plan.record(&key, Some(result(100)));
        }
        assert!(plan.next().is_none());
        assert_eq!(plan.progress(), (8, 0, 8));
        assert!(plan.status(true).contains("4/4 positions · finished"));
        let mut key = plan.configuration.clone();
        key.options.threads = 2;
        assert!(!plan.configure(&key));
        assert_eq!(plan.progress(), (8, 0, 8));
        key.limit = engine::Limit::Depth(18);
        assert!(plan.configure(&key));
        assert_eq!(plan.preview_limit(), Some(engine::Limit::Depth(8)));
        assert_eq!(plan.progress(), (0, 0, 8));
    }
    #[test]
    fn incomplete_preview_does_not_loop_and_can_still_receive_final_analysis() {
        let mut plan = plan();
        let (key, _) = plan.next().unwrap();
        plan.record(&key, None);
        for _ in 0..3 {
            let (key, _) = plan.next().unwrap();
            assert_ne!(key.fen, plan.positions[0].fen);
            plan.record(&key, Some(result(0)));
        }
        let (key, _) = plan.next().unwrap();
        assert_eq!(key.fen, plan.positions[0].fen);
        assert_eq!(key.limit, engine::Limit::TimeMs(1000));
    }
}
