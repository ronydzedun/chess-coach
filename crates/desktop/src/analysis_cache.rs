// SPDX-License-Identifier: GPL-3.0-or-later
//! Completed position results, separate from Stockfish's internal search hash.
use std::collections::VecDeque;

pub(super) const CAPACITY: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Key {
    pub fen: String,
    pub engine_path: String,
    pub options: engine::EngineOptions,
    pub limit: engine::Limit,
}
impl Key {
    // Resource allocation changes execution, not the requested analysis criterion.
    pub fn same_analysis(&self, other: &Self) -> bool {
        self.fen == other.fen && self.engine_path == other.engine_path && self.limit == other.limit
    }
}
#[derive(Debug, Clone)]
pub(super) struct Result {
    pub info: engine::Info,
    pub best_move: Option<String>,
}
#[derive(Default)]
pub(super) struct Cache {
    // Oldest at the front; a hit refreshes the entry's recency.
    entries: VecDeque<(Key, Result)>,
}
impl Cache {
    pub fn get(&mut self, key: &Key) -> Option<Result> {
        let index = self
            .entries
            .iter()
            .position(|(entry, _)| entry.same_analysis(key))?;
        let entry = self.entries.remove(index)?;
        let result = entry.1.clone();
        self.entries.push_back(entry);
        Some(result)
    }
    pub fn insert(&mut self, key: Key, result: Result) {
        if let Some(index) = self
            .entries
            .iter()
            .position(|(entry, _)| entry.same_analysis(&key))
        {
            self.entries.remove(index);
        }
        if self.entries.len() == CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back((key, result));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn key(fen: &str) -> Key {
        Key {
            fen: fen.into(),
            engine_path: "stockfish".into(),
            options: engine::EngineOptions::default(),
            limit: engine::Limit::TimeMs(1000),
        }
    }
    fn result(cp: i32) -> Result {
        Result {
            info: engine::Info {
                score: Some(engine::Score::Cp(cp)),
                ..Default::default()
            },
            best_move: Some("e2e4".into()),
        }
    }
    #[test]
    fn lru_is_bounded_and_hits_refresh_recency() {
        let mut cache = Cache::default();
        for index in 0..CAPACITY {
            cache.insert(key(&index.to_string()), result(index as i32));
        }
        assert!(cache.get(&key("0")).is_some());
        cache.insert(key("new"), result(999));
        assert!(cache.get(&key("1")).is_none());
        assert!(cache.get(&key("0")).is_some());
        assert_eq!(cache.entries.len(), CAPACITY);
        cache.insert(key("0"), result(42));
        assert_eq!(cache.entries.len(), CAPACITY);
        assert_eq!(
            cache.get(&key("0")).unwrap().info.score,
            Some(engine::Score::Cp(42))
        );
    }
    #[test]
    fn position_engine_and_search_limit_define_identity_but_resources_do_not() {
        let mut cache = Cache::default();
        let original = key("position w - - 0 1");
        cache.insert(original.clone(), result(12));
        let mut variants = vec![original.clone(); 7];
        variants[0].fen = "position b - - 0 1".into();
        variants[1].engine_path = "other-stockfish".into();
        variants[2].options.threads = 2;
        variants[3].options.hash_mib = 128;
        variants[4].limit = engine::Limit::Depth(20);
        variants[5].limit = engine::Limit::TimeMs(2000);
        variants[6].fen = "position w - - 1 2".into();
        for (index, variant) in variants.into_iter().enumerate() {
            assert_eq!(cache.get(&variant).is_some(), matches!(index, 2 | 3));
        }
        assert!(cache.get(&original).is_some());
    }
}
