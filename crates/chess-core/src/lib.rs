// SPDX-License-Identifier: GPL-3.0-or-later
//! Transactional standard-chess game state. Rules belong to shakmaty.
use shakmaty::{
    CastlingMode, Chess, EnPassantMode, Position, fen::Fen, san::SanPlus, uci::UciMove,
};
pub use shakmaty::{Color, Piece, Role, Square};

pub mod pgn;

#[derive(Clone, Debug)]
pub struct PlayedMove {
    pub san: String,
    pub uci: String,
    pub from: Square,
    pub to: Square,
    pub mover: Color,
    pub number: u32,
}

#[derive(Clone, Debug)]
pub struct Game {
    positions: Vec<Chess>,
    cursor: usize,
    history: Vec<PlayedMove>,
    revision: u64,
}

impl Default for Game {
    fn default() -> Self {
        Self {
            positions: vec![Chess::default()],
            cursor: 0,
            history: Vec::new(),
            revision: 0,
        }
    }
}
impl Game {
    fn position(&self) -> &Chess {
        &self.positions[self.cursor]
    }
    /// Complete working line, including moves after the current review cursor.
    pub fn line(&self) -> &[PlayedMove] {
        &self.history
    }
    pub fn ply(&self) -> usize {
        self.cursor
    }
    /// Navigate without destroying the continuation. Invalid seeks are transactional.
    pub fn seek(&mut self, ply: usize) -> Result<bool, String> {
        if ply >= self.positions.len() {
            return Err("Move index is outside the current game".into());
        }
        if ply == self.cursor {
            return Ok(false);
        }
        self.cursor = ply;
        self.revision = self.revision.wrapping_add(1);
        Ok(true)
    }
    /// Load an independently retained line, preserving this game's revision sequence.
    pub fn load_line(&mut self, line: &Game) {
        let revision = self.revision.wrapping_add(1);
        *self = line.clone();
        self.revision = revision;
    }
    pub fn fen(&self) -> String {
        Fen::from_position(self.position(), EnPassantMode::Legal).to_string()
    }
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn turn(&self) -> Color {
        self.position().turn()
    }
    pub fn piece_at(&self, square: Square) -> Option<Piece> {
        self.position().board().piece_at(square)
    }
    pub fn history(&self) -> &[PlayedMove] {
        &self.history[..self.cursor]
    }
    pub fn last_move(&self) -> Option<&PlayedMove> {
        self.history().last()
    }
    pub fn in_check(&self) -> bool {
        self.position().is_check()
    }
    pub fn status(&self) -> &'static str {
        if self.position().is_checkmate() {
            "Checkmate"
        } else if self.position().is_stalemate() {
            "Stalemate"
        } else if self.position().is_insufficient_material() {
            "Draw: insufficient material"
        } else if self.position().is_check() {
            "Check"
        } else {
            "Ready"
        }
    }
    pub fn destinations(&self, from: Square) -> Vec<Square> {
        self.position()
            .legal_moves()
            .iter()
            .filter_map(|m| {
                let uci = UciMove::from_standard(*m);
                (uci.from() == Some(from)).then(|| uci.to()).flatten()
            })
            .collect()
    }
    pub fn needs_promotion(&self, from: Square, to: Square) -> bool {
        self.position().legal_moves().iter().any(|m| {
            let uci = UciMove::from_standard(*m);
            uci.from() == Some(from) && uci.to() == Some(to) && m.promotion().is_some()
        })
    }
    pub fn play(
        &mut self,
        from: Square,
        to: Square,
        promotion: Option<Role>,
    ) -> Result<(), String> {
        let suffix = promotion
            .map(|role| role.char().to_string())
            .unwrap_or_default();
        self.play_uci(&format!("{from}{to}{suffix}"))
    }
    pub fn play_uci(&mut self, input: &str) -> Result<(), String> {
        let uci: UciMove = input
            .parse()
            .map_err(|_| "Invalid move notation".to_string())?;
        let m = uci
            .to_move(self.position())
            .map_err(|_| "Illegal move: position unchanged".to_string())?;
        let from = uci.from().ok_or("Null moves are not supported")?;
        let to = uci.to().ok_or("Null moves are not supported")?;
        let played = PlayedMove {
            san: SanPlus::from_move(self.position().clone(), m).to_string(),
            uci: input.to_string(),
            from,
            to,
            mover: self.turn(),
            number: self.position().fullmoves().get(),
        };
        let mut next = self.position().clone();
        next.play_unchecked(m);
        self.positions.truncate(self.cursor + 1);
        self.history.truncate(self.cursor);
        self.positions.push(next);
        self.history.push(played);
        self.cursor += 1;
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
    pub fn load_fen(&mut self, input: &str) -> Result<(), String> {
        let fen: Fen = input.parse().map_err(|err| format!("Invalid FEN: {err}"))?;
        let next = fen
            .into_position(CastlingMode::Standard)
            .map_err(|err| format!("Invalid position: {err}"))?;
        self.positions = vec![next];
        self.cursor = 0;
        self.history.clear();
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }
    /// Step back, retaining the line for forward navigation until another move is played.
    pub fn undo(&mut self) -> bool {
        self.cursor > 0 && self.seek(self.cursor - 1).expect("valid previous ply")
    }
    pub fn reset(&mut self) {
        let revision = self.revision.wrapping_add(1);
        *self = Self::default();
        self.revision = revision;
    }
}

/// Screen cell (row zero at top) to a square, for either orientation.
pub fn square_at(row: usize, col: usize, flipped: bool) -> Option<Square> {
    if row >= 8 || col >= 8 {
        return None;
    }
    let index = if flipped {
        row * 8 + 7 - col
    } else {
        (7 - row) * 8 + col
    };
    Some(Square::new(index as u32))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fen_game(fen: &str) -> Game {
        let mut g = Game::default();
        g.load_fen(fen).unwrap();
        g
    }
    #[test]
    fn initial_and_illegal_are_transactional() {
        let mut g = Game::default();
        let start = g.fen();
        assert_eq!(g.position().legal_moves().len(), 20);
        assert!(g.play_uci("e2e5").is_err());
        assert!(g.play_uci("0000").is_err());
        assert_eq!(g.fen(), start);
        assert_eq!(g.revision(), 0);
        g.play_uci("e2e4").unwrap();
        assert_eq!(g.history()[0].san, "e4");
        assert!(g.undo());
        assert_eq!(g.fen(), start);
        assert!(!g.undo());
    }
    #[test]
    fn navigation_retains_line_and_branching_is_transactional() {
        let mut g = Game::default();
        let start = g.fen();
        for m in ["e2e4", "e7e5", "g1f3"] {
            g.play_uci(m).unwrap();
        }
        let end = g.fen();
        assert_eq!(g.ply(), 3);
        g.seek(0).unwrap();
        assert_eq!(g.fen(), start);
        assert_eq!(g.line().len(), 3);
        assert!(g.history().is_empty());
        let revision = g.revision();
        assert!(!g.seek(0).unwrap());
        assert_eq!(g.revision(), revision);
        assert!(g.seek(4).is_err());
        assert_eq!(g.revision(), revision);
        assert_eq!(g.fen(), start);
        assert!(g.play_uci("e2e5").is_err());
        assert_eq!(g.line().len(), 3);
        g.seek(3).unwrap();
        assert_eq!(g.fen(), end);
        assert_eq!(g.last_move().unwrap().san, "Nf3");
        assert!(g.undo());
        assert_eq!(g.ply(), 2);
        assert_eq!(g.line().len(), 3);
        g.play_uci("f1c4").unwrap();
        assert_eq!(g.line().len(), 3);
        assert_eq!(g.line()[2].san, "Bc4");
        let saved = g.clone();
        g.reset();
        let rev = g.revision();
        g.load_line(&saved);
        assert!(g.revision() > rev);
        assert_eq!(g.fen(), saved.fen());
    }
    #[test]
    fn castling() {
        let mut g = fen_game("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1");
        g.play_uci("e1g1").unwrap();
        assert_eq!(g.piece_at(Square::F1).unwrap().role, Role::Rook);
        assert_eq!(g.history()[0].san, "O-O");
        g.play_uci("e8c8").unwrap();
        assert_eq!(g.piece_at(Square::D8).unwrap().role, Role::Rook);
    }
    #[test]
    fn en_passant() {
        let mut g = Game::default();
        for m in ["e2e4", "a7a6", "e4e5", "d7d5", "e5d6"] {
            g.play_uci(m).unwrap();
        }
        assert!(g.piece_at(Square::D5).is_none());
        assert!(g.piece_at(Square::D6).is_some());
        assert!(g.undo());
        assert!(g.piece_at(Square::D5).is_some());
    }
    #[test]
    fn all_promotions() {
        for (suffix, role) in [
            ("q", Role::Queen),
            ("r", Role::Rook),
            ("b", Role::Bishop),
            ("n", Role::Knight),
        ] {
            let mut g = fen_game("7k/P7/8/8/8/8/8/7K w - - 0 1");
            assert!(g.needs_promotion(Square::A7, Square::A8));
            assert!(g.play_uci("a7a8").is_err());
            g.play_uci(&format!("a7a8{suffix}")).unwrap();
            assert_eq!(g.piece_at(Square::A8).unwrap().role, role);
        }
    }
    #[test]
    fn check_mate_and_stalemate() {
        let mut g = Game::default();
        for m in ["f2f3", "e7e5", "g2g4", "d8h4"] {
            g.play_uci(m).unwrap();
        }
        assert!(g.in_check());
        assert_eq!(g.status(), "Checkmate");
        let g = fen_game("7k/5Q2/6K1/8/8/8/8/8 b - - 0 1");
        assert_eq!(g.status(), "Stalemate");
        let g = fen_game("7k/7Q/6K1/8/8/8/8/8 b - - 0 1");
        assert!(g.in_check());
    }
    #[test]
    fn fen_errors_preserve_history() {
        let mut g = Game::default();
        g.play_uci("e2e4").unwrap();
        let old = g.fen();
        let rev = g.revision();
        for bad in [
            "garbage",
            "8/8/8/8/8/8/8/8 w - - 0 1",
            "8/8/8/8/8/8/4k3/4K3 w - - 0 1",
        ] {
            assert!(g.load_fen(bad).is_err());
            assert_eq!(g.fen(), old);
            assert_eq!(g.revision(), rev);
            assert_eq!(g.history().len(), 1);
        }
        g.load_fen(&old).unwrap();
        assert_eq!(g.fen(), old);
        assert!(g.history().is_empty());
        g.reset();
        assert_eq!(g.fen(), Game::default().fen());
    }
    #[test]
    fn coordinates_and_flip() {
        assert_eq!(square_at(0, 0, false), Some(Square::A8));
        assert_eq!(square_at(0, 0, true), Some(Square::H1));
        for row in 0..8 {
            for col in 0..8 {
                assert_eq!(
                    square_at(row, col, false),
                    square_at(7 - row, 7 - col, true)
                );
            }
        }
        assert_eq!(square_at(8, 0, false), None);
    }
}
