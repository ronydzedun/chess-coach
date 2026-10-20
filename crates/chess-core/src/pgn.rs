// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded, transactional PGN mainline import. Chess rules belong to shakmaty.
use std::{collections::BTreeMap, ops::ControlFlow};

use pgn_reader::{Outcome, RawTag, Reader, SanPlus, Skip, Visitor};
use shakmaty::{Position, uci::UciMove};

use crate::Game;

pub const MAX_PGN_BYTES: usize = 8 * 1024 * 1024;
const MAX_GAMES: usize = 256;
const MAX_PLIES: usize = 2048;
const MAX_TOTAL_PLIES: usize = 50_000;

#[derive(Clone, Debug)]
pub struct PgnGame {
    pub tags: BTreeMap<String, String>,
    /// Validated movetext termination, including when no Result tag was present.
    pub result: String,
    pub game: Game,
    pub skipped_variations: bool,
}

#[derive(Default)]
struct CheckedGame {
    start: usize,
    end: usize,
    plies: usize,
    tags: usize,
    variations: bool,
}

fn error(game: usize, ply: usize, message: impl std::fmt::Display) -> String {
    // Source-derived tokens and header values can approach the input byte limit.
    // Bound diagnostics in Unicode characters before the desktop renders them.
    let message = message.to_string();
    let mut chars = message.chars();
    let mut excerpt: String = chars.by_ref().take(240).collect();
    if chars.next().is_some() {
        excerpt.push_str("… [truncated]");
    }
    format!("PGN game {game}, ply {ply}: {excerpt}")
}

/// Import every game or return an error without exposing a partial import.
/// Returned games retain the complete mainline with the review cursor at ply zero.
/// Omitted SAN check suffixes are accepted; supplied `+` and `#` must match the
/// canonical move exactly, including `#` for checkmate. Explicit results must
/// agree with a final checkmate or stalemate; `*` remains valid for analysis.
pub fn parse_pgn(input: &[u8]) -> Result<Vec<PgnGame>, String> {
    if input.len() > MAX_PGN_BYTES {
        return Err(format!("PGN exceeds the {MAX_PGN_BYTES}-byte limit"));
    }
    std::str::from_utf8(input).map_err(|e| format!("PGN is not valid UTF-8: {e}"))?;
    let input = input.strip_prefix(b"\xef\xbb\xbf").unwrap_or(input);
    let (normalized, checked) = check_tokens(input)?;
    let mut games = Vec::with_capacity(checked.len());
    for (index, expected) in checked.into_iter().enumerate() {
        let mut reader = Reader::build(&normalized[expected.start..expected.end])
            .set_supported_tag_line_length(expected.end - expected.start)
            .finish();
        let mut visitor = Importer { number: index + 1 };
        let imported = reader
            .read_game(&mut visitor)
            .map_err(|e| error(index + 1, 0, e))?
            .ok_or_else(|| error(index + 1, 0, "missing game"))??;
        if imported.game.line().len() != expected.plies
            || imported.tags.len() != expected.tags
            || imported.skipped_variations != expected.variations
        {
            return Err(error(index + 1, 0, "unrecognized or misplaced PGN content"));
        }
        games.push(imported);
    }
    Ok(games)
}

// pgn-reader intentionally recovers from malformed tokens. This lexical guard
// rejects what it would silently drop; interpretation and chess legality remain
// with pgn-reader and shakmaty. Comments are blanked and whitespace normalized so
// blank lines cannot accidentally end a game or a skipped variation in Reader.
fn check_tokens(input: &[u8]) -> Result<(Vec<u8>, Vec<CheckedGame>), String> {
    let mut normalized = input.to_vec();
    let mut games = Vec::new();
    let mut current: Option<CheckedGame> = None;
    let mut depth = 0usize;
    let mut total = 0usize;
    let mut i = 0;
    while i < input.len() {
        let number = games.len() + 1;
        let ply = current.as_ref().map_or(1, |g| g.plies + 1);
        let fail = |message: &str| error(number, ply, message);
        let byte = input[i];
        if byte.is_ascii_whitespace() {
            normalized[i] = b' ';
            i += 1;
            continue;
        }
        if byte == b';' || (byte == b'%' && (i == 0 || input[i - 1] == b'\n')) {
            let start = i;
            while i < input.len() && input[i] != b'\n' {
                i += 1;
            }
            normalized[start..i].fill(b' ');
            continue;
        }
        if byte == b'{' {
            let start = i;
            i += 1;
            while i < input.len() && input[i] != b'}' {
                i += 1;
            }
            if i == input.len() {
                return Err(fail("unterminated brace comment"));
            }
            i += 1;
            normalized[start..i].fill(b' ');
            continue;
        }
        if current.is_none() && games.len() == MAX_GAMES {
            return Err(fail("exceeds the 256-game limit"));
        }
        let game = current.get_or_insert_with(|| CheckedGame {
            start: i,
            ..CheckedGame::default()
        });
        match byte {
            b'[' => {
                if depth != 0 || game.plies != 0 || game.variations {
                    return Err(fail(
                        "tag found inside movetext or before the previous game result",
                    ));
                }
                i = check_tag(input, i).map_err(fail)?;
                game.tags += 1;
            }
            b'(' => {
                if game.plies == 0 {
                    return Err(fail("variation without a preceding mainline move"));
                }
                depth += 1;
                game.variations = true;
                i += 1;
            }
            b')' => {
                depth = depth
                    .checked_sub(1)
                    .ok_or_else(|| fail("unmatched closing variation"))?;
                i += 1;
            }
            b'!' | b'?' => {
                if game.plies == 0 {
                    return Err(fail("annotation without a move"));
                }
                i += 1;
                if input.get(i).is_some_and(|b| matches!(b, b'!' | b'?')) {
                    i += 1;
                }
                if input.get(i).is_some_and(|b| matches!(b, b'!' | b'?')) {
                    return Err(fail("invalid annotation glyph"));
                }
            }
            b'$' => {
                if game.plies == 0 {
                    return Err(fail("annotation without a move"));
                }
                let start = i + 1;
                i = start;
                while input.get(i).is_some_and(u8::is_ascii_digit) {
                    i += 1;
                }
                if start == i
                    || std::str::from_utf8(&input[start..i])
                        .unwrap()
                        .parse::<u8>()
                        .is_err()
                {
                    return Err(fail("invalid numeric annotation glyph"));
                }
                if input.get(i).is_some_and(|b| !delimiter(*b)) {
                    return Err(fail("invalid numeric annotation glyph"));
                }
            }
            _ => {
                let start = i;
                while i < input.len() && !delimiter(input[i]) {
                    i += 1;
                }
                if i == start {
                    return Err(fail("unexpected PGN delimiter"));
                }
                let mut token = &input[start..i];
                if matches!(token, b"1-0" | b"0-1" | b"1/2-1/2" | b"*") {
                    if depth == 0 {
                        if game.plies == 0 && game.tags == 0 {
                            return Err(fail("empty game without tags or moves"));
                        }
                        game.end = i;
                        games.push(current.take().unwrap());
                    }
                    continue;
                }
                if token[0].is_ascii_digit() {
                    let digits = token.iter().take_while(|b| b.is_ascii_digit()).count();
                    let dots = token[digits..].iter().take_while(|b| **b == b'.').count();
                    if dots > 0 {
                        if !matches!(dots, 1 | 3)
                            || std::str::from_utf8(&token[..digits])
                                .unwrap()
                                .parse::<u32>()
                                .ok()
                                .is_none_or(|n| n == 0)
                        {
                            return Err(fail("invalid move number"));
                        }
                        token = &token[digits + dots..];
                        if token.is_empty() {
                            continue;
                        }
                    }
                }
                SanPlus::from_ascii(token).map_err(|_| {
                    error(
                        number,
                        ply,
                        format!(
                            "invalid SAN or unknown token {:?}",
                            String::from_utf8_lossy(token)
                        ),
                    )
                })?;
                if depth == 0 {
                    game.plies += 1;
                    total += 1;
                    if game.plies > MAX_PLIES {
                        return Err(fail("exceeds the 2048-ply game limit"));
                    }
                    if total > MAX_TOTAL_PLIES {
                        return Err(fail("exceeds the 50000-ply import limit"));
                    }
                }
            }
        }
    }
    if depth != 0 {
        return Err(error(
            games.len() + 1,
            current.as_ref().map_or(1, |g| g.plies + 1),
            "unterminated variation",
        ));
    }
    if let Some(game) = current {
        return Err(error(
            games.len() + 1,
            game.plies + 1,
            "missing termination result (1-0, 0-1, 1/2-1/2 or *)",
        ));
    }
    if games.is_empty() {
        return Err("PGN contains no games".into());
    }
    Ok((normalized, games))
}

fn delimiter(byte: u8) -> bool {
    byte.is_ascii_whitespace()
        || matches!(
            byte,
            b'[' | b']' | b'{' | b'}' | b'(' | b')' | b';' | b'$' | b'!' | b'?'
        )
}

fn check_tag(input: &[u8], mut i: usize) -> Result<usize, &'static str> {
    i += 1;
    let start = i;
    while input
        .get(i)
        .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
    {
        i += 1;
    }
    if i == start
        || !input[start].is_ascii_alphabetic()
        || !matches!(input.get(i), Some(b' ' | b'\t'))
    {
        return Err("invalid tag name or missing space before tag value");
    }
    while matches!(input.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    if input.get(i) != Some(&b'"') {
        return Err("tag value must be quoted");
    }
    i += 1;
    loop {
        match input.get(i) {
            Some(b'"') => {
                i += 1;
                break;
            }
            Some(b'\\') => {
                i += 1;
                if !matches!(input.get(i), Some(b'"' | b'\\')) {
                    return Err("invalid or truncated tag escape");
                }
            }
            Some(b'\r' | b'\n' | 0) | None => return Err("unterminated tag value"),
            _ => {}
        }
        i += 1;
    }
    while matches!(input.get(i), Some(b' ' | b'\t')) {
        i += 1;
    }
    if input.get(i) != Some(&b']') {
        return Err("missing closing tag bracket");
    }
    Ok(i + 1)
}

struct Importer {
    number: usize,
}
struct Movetext {
    imported: PgnGame,
    result: Option<String>,
}
impl Visitor for Importer {
    type Tags = BTreeMap<String, String>;
    type Movetext = Movetext;
    type Output = Result<PgnGame, String>;

    fn begin_tags(&mut self) -> ControlFlow<Self::Output, Self::Tags> {
        ControlFlow::Continue(BTreeMap::new())
    }
    fn tag(
        &mut self,
        tags: &mut Self::Tags,
        name: &[u8],
        value: RawTag<'_>,
    ) -> ControlFlow<Self::Output> {
        let name = std::str::from_utf8(name)
            .expect("guard checked UTF-8")
            .trim();
        let value = value
            .decode_utf8()
            .expect("guard checked UTF-8")
            .into_owned();
        if tags.insert(name.to_string(), value).is_some() {
            return ControlFlow::Break(Err(error(self.number, 0, format!("duplicate tag {name}"))));
        }
        ControlFlow::Continue(())
    }
    fn begin_movetext(&mut self, tags: Self::Tags) -> ControlFlow<Self::Output, Self::Movetext> {
        let make = || -> Result<Movetext, String> {
            if let Some(variant) = tags.get("Variant")
                && !variant.eq_ignore_ascii_case("Standard")
                && !variant.eq_ignore_ascii_case("Chess")
            {
                return Err(error(
                    self.number,
                    0,
                    format!("unsupported Variant: {variant}"),
                ));
            }
            if let Some(result) = tags.get("Result")
                && !matches!(result.as_str(), "1-0" | "0-1" | "1/2-1/2" | "*")
            {
                return Err(error(self.number, 0, "invalid Result tag"));
            }
            let mut game = Game::default();
            match (tags.get("SetUp").map(String::as_str), tags.get("FEN")) {
                (Some("1"), Some(fen)) => {
                    game.load_fen(fen).map_err(|e| error(self.number, 0, e))?
                }
                (None | Some("0"), None) => {}
                (Some("1"), None) => {
                    return Err(error(self.number, 0, "SetUp 1 requires a FEN tag"));
                }
                (None | Some("0"), Some(_)) => {
                    return Err(error(self.number, 0, "FEN requires SetUp 1"));
                }
                _ => return Err(error(self.number, 0, "SetUp must be 0 or 1")),
            }
            Ok(Movetext {
                imported: PgnGame {
                    tags,
                    result: String::new(),
                    game,
                    skipped_variations: false,
                },
                result: None,
            })
        };
        match make() {
            Ok(m) => ControlFlow::Continue(m),
            Err(e) => ControlFlow::Break(Err(e)),
        }
    }
    fn san(&mut self, movetext: &mut Self::Movetext, san: SanPlus) -> ControlFlow<Self::Output> {
        let game = &mut movetext.imported.game;
        let ply = game.line().len() + 1;
        let m = match san.san.to_move(game.position()) {
            Ok(m) => m,
            Err(e) => {
                return ControlFlow::Break(Err(error(
                    self.number,
                    ply,
                    format!("invalid or ambiguous move {san}: {e}"),
                )));
            }
        };
        if san.suffix.is_some()
            && san.suffix != SanPlus::from_move(game.position().clone(), m).suffix
        {
            return ControlFlow::Break(Err(error(
                self.number,
                ply,
                format!("incorrect check or mate suffix on {san}"),
            )));
        }
        let uci = UciMove::from_standard(m).to_string();
        if let Err(e) = game.play_uci(&uci) {
            return ControlFlow::Break(Err(error(self.number, ply, e)));
        }
        ControlFlow::Continue(())
    }
    fn begin_variation(
        &mut self,
        movetext: &mut Self::Movetext,
    ) -> ControlFlow<Self::Output, Skip> {
        movetext.imported.skipped_variations = true;
        ControlFlow::Continue(Skip(true))
    }
    fn outcome(
        &mut self,
        movetext: &mut Self::Movetext,
        outcome: Outcome,
    ) -> ControlFlow<Self::Output> {
        if movetext.result.replace(outcome.to_string()).is_some() {
            return ControlFlow::Break(Err(error(
                self.number,
                movetext.imported.game.line().len(),
                "multiple termination results",
            )));
        }
        ControlFlow::Continue(())
    }
    fn end_game(&mut self, mut movetext: Self::Movetext) -> Self::Output {
        let result = movetext.result.ok_or_else(|| {
            error(
                self.number,
                movetext.imported.game.line().len(),
                "missing termination result",
            )
        })?;
        if movetext
            .imported
            .tags
            .get("Result")
            .is_some_and(|tag| tag != &result)
        {
            return Err(error(
                self.number,
                movetext.imported.game.line().len(),
                "Result tag disagrees with movetext termination",
            ));
        }
        let game = &movetext.imported.game;
        let definite_result = if game.position().is_checkmate() {
            Some(if game.turn() == crate::Color::White {
                "0-1"
            } else {
                "1-0"
            })
        } else if game.position().is_stalemate() {
            Some("1/2-1/2")
        } else {
            None
        };
        if result != "*" && definite_result.is_some_and(|expected| result != expected) {
            return Err(error(
                self.number,
                game.line().len(),
                "termination result contradicts the final checkmate or stalemate position",
            ));
        }
        movetext.imported.result = result;
        movetext
            .imported
            .game
            .seek(0)
            .expect("initial position exists");
        Ok(movetext.imported)
    }
}
