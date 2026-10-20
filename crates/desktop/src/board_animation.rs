// SPDX-License-Identifier: GPL-3.0-or-later
//! Small, presentation-only flights. The chess position is committed immediately.
use chess_core::{Game, Piece, Role, Square};
use iced::Point;
use std::time::{Duration, Instant};

pub(super) const DURATION: Duration = Duration::from_millis(140);
pub(super) type Snapshot = [Option<Piece>; 64];
pub(super) fn snapshot(game: &Game) -> Snapshot {
    std::array::from_fn(|index| game.piece_at(Square::new(index as u32)))
}
#[derive(Clone, Debug)]
pub(super) struct Flight {
    pub from: Square,
    pub to: Square,
    pub piece: Piece,
    arrival: Piece,
}
#[derive(Clone, Debug)]
pub(super) struct Transition {
    pub revision: u64,
    pub flights: Vec<Flight>,
}
impl Transition {
    pub fn between(before: Snapshot, after: &Game, from: Square, to: Square) -> Option<Self> {
        let piece = before[from as usize]?;
        let arrival = after.piece_at(to)?;
        let mut flights = vec![Flight {
            from,
            to,
            piece,
            arrival,
        }];
        if piece.role == Role::King && (from.file() as i32 - to.file() as i32).abs() == 2 {
            let rook = Piece {
                color: piece.color,
                role: Role::Rook,
            };
            let source = (0..64)
                .map(Square::new)
                .find(|s| before[*s as usize] == Some(rook) && after.piece_at(*s) != Some(rook));
            let target = (0..64)
                .map(Square::new)
                .find(|s| before[*s as usize] != Some(rook) && after.piece_at(*s) == Some(rook));
            if let (Some(from), Some(to)) = (source, target) {
                flights.push(Flight {
                    from,
                    to,
                    piece: rook,
                    arrival: rook,
                });
            }
        }
        Some(Self {
            revision: after.revision(),
            flights,
        })
    }
}
fn grid(square: Square) -> Point {
    Point::new(square.file() as u8 as f32, square.rank() as u8 as f32)
}
#[derive(Clone)]
struct Tween {
    flight: Flight,
    origin: Point,
    started: Instant,
}
impl Tween {
    fn position(&self, now: Instant) -> Point {
        let t = (now.saturating_duration_since(self.started).as_secs_f32()
            / DURATION.as_secs_f32())
        .clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t).powi(3);
        let target = grid(self.flight.to);
        Point::new(
            self.origin.x + (target.x - self.origin.x) * eased,
            self.origin.y + (target.y - self.origin.y) * eased,
        )
    }
    fn unfinished(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) < DURATION
    }
}
#[derive(Default)]
pub(super) struct Animation {
    tweens: Vec<Tween>,
}
impl Animation {
    pub fn clear(&mut self) {
        self.tweens.clear();
    }
    pub fn advance(&mut self, now: Instant) -> bool {
        self.tweens.retain(|tween| tween.unfinished(now));
        !self.tweens.is_empty()
    }
    pub fn transition(
        &mut self,
        transition: &Transition,
        game: &Game,
        dropped: Option<(Square, Square)>,
        now: Instant,
    ) {
        let mut tweens = Vec::new();
        for flight in &transition.flights {
            if dropped == Some((flight.from, flight.to)) {
                continue;
            }
            let origin = self
                .tweens
                .iter()
                .find(|t| t.flight.to == flight.from && t.flight.arrival == flight.piece)
                .map(|t| t.position(now))
                .unwrap_or_else(|| grid(flight.from));
            tweens.push(Tween {
                flight: flight.clone(),
                origin,
                started: now,
            });
        }
        // Preserve unrelated in-flight pieces during rapid forward navigation.
        tweens.extend(
            self.tweens
                .iter()
                .filter(|t| {
                    t.unfinished(now)
                        && game.piece_at(t.flight.to) == Some(t.flight.arrival)
                        && !transition
                            .flights
                            .iter()
                            .any(|f| f.to == t.flight.to || f.from == t.flight.to)
                })
                .cloned(),
        );
        self.tweens = tweens;
    }
    pub fn contains(&self, square: Square) -> bool {
        self.tweens.iter().any(|t| t.flight.to == square)
    }
    pub fn pieces(&self, now: Instant) -> impl Iterator<Item = (Piece, Point)> + '_ {
        self.tweens
            .iter()
            .map(move |t| (t.flight.piece, t.position(now)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn play(game: &mut Game, uci: &str) -> Transition {
        let before = snapshot(game);
        game.play_uci(uci).unwrap();
        let m = game.last_move().unwrap();
        Transition::between(before, game, m.from, m.to).unwrap()
    }
    #[test]
    fn easing_reaches_destination_and_stops_after_short_duration() {
        let mut game = Game::default();
        let transition = play(&mut game, "e2e4");
        let now = Instant::now();
        let mut animation = Animation::default();
        animation.transition(&transition, &game, None, now);
        assert_eq!(animation.pieces(now).next().unwrap().1, grid(Square::E2));
        let middle = animation.pieces(now + DURATION / 2).next().unwrap().1;
        assert!(middle.y > 2.0 && middle.y < 3.0);
        assert_eq!(
            animation.pieces(now + DURATION).next().unwrap().1,
            grid(Square::E4)
        );
        assert!(!animation.advance(now + DURATION));
        assert!(!animation.contains(Square::E4));
    }
    #[test]
    fn castling_undo_and_promotion_use_the_actual_pieces() {
        let mut game = Game::default();
        game.load_fen("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1")
            .unwrap();
        let transition = play(&mut game, "e1g1");
        assert_eq!(transition.flights.len(), 2);
        assert_eq!(
            (transition.flights[1].from, transition.flights[1].to),
            (Square::H1, Square::F1)
        );
        let before = snapshot(&game);
        game.undo();
        let reverse = Transition::between(before, &game, Square::G1, Square::E1).unwrap();
        assert_eq!(
            (reverse.flights[1].from, reverse.flights[1].to),
            (Square::F1, Square::H1)
        );
        game.load_fen("7k/P7/8/8/8/8/8/7K w - - 0 1").unwrap();
        let promotion = play(&mut game, "a7a8n");
        assert_eq!(promotion.flights[0].piece.role, Role::Pawn);
        assert_eq!(promotion.flights[0].arrival.role, Role::Knight);
    }
    #[test]
    fn rapid_forward_moves_preserve_unrelated_flights() {
        let mut game = Game::default();
        let first = play(&mut game, "e2e4");
        let now = Instant::now();
        let mut animation = Animation::default();
        animation.transition(&first, &game, None, now);
        let second = play(&mut game, "e7e5");
        animation.transition(&second, &game, None, now + Duration::from_millis(20));
        assert!(animation.contains(Square::E4) && animation.contains(Square::E5));
        animation.advance(now + DURATION);
        assert!(!animation.contains(Square::E4) && animation.contains(Square::E5));
    }
    #[test]
    fn reversing_mid_flight_is_continuous_and_dragged_piece_does_not_replay() {
        let mut game = Game::default();
        let transition = play(&mut game, "e2e4");
        let now = Instant::now();
        let mut animation = Animation::default();
        animation.transition(&transition, &game, None, now);
        let halfway = now + DURATION / 2;
        let current = animation.pieces(halfway).next().unwrap().1;
        let before = snapshot(&game);
        game.undo();
        let reverse = Transition::between(before, &game, Square::E4, Square::E2).unwrap();
        animation.transition(&reverse, &game, None, halfway);
        assert_eq!(animation.pieces(halfway).next().unwrap().1, current);
        animation.clear();
        animation.transition(&transition, &game, Some((Square::E2, Square::E4)), now);
        assert!(!animation.contains(Square::E4));
    }
}
