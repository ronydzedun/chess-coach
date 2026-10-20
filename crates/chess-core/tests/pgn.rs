// SPDX-License-Identifier: GPL-3.0-or-later
use chess_core::{
    Color, Game, Role, Square,
    pgn::{MAX_PGN_BYTES, parse_pgn},
};

fn parse(input: &str) -> Vec<chess_core::pgn::PgnGame> {
    parse_pgn(input.as_bytes()).unwrap_or_else(|e| panic!("{input:?}: {e}"))
}
fn rejects(input: &str, expected: &str) {
    let error = parse_pgn(input.as_bytes()).unwrap_err();
    assert!(
        error.contains(expected),
        "{input:?}: expected {expected:?} in {error:?}"
    );
}

#[test]
fn imports_complete_multi_game_mainlines_at_the_root() {
    let mut games = parse_pgn(include_bytes!("fixtures/synthetic_chess_com.pgn")).unwrap();
    assert_eq!(games.len(), 2);
    assert_eq!(games[0].tags["White"], "PracticeWhite");
    assert_eq!(games[0].game.line().len(), 16);
    assert_eq!(games[1].game.line().len(), 6);
    assert!(
        games
            .iter()
            .all(|g| g.game.ply() == 0 && g.game.history().is_empty())
    );
    assert_eq!(games[0].game.fen(), Game::default().fen());
    games[0].game.seek(16).unwrap();
    assert_eq!(games[0].game.piece_at(Square::G8).unwrap().role, Role::King);
    assert!(!games[0].skipped_variations);
}

#[test]
fn skips_annotations_comments_and_nested_variations_without_playing_them() {
    let games = parse(
        "% exporter escape\n1.e4!? {braces (and fake SAN) $99} $1 (1.d4 d5 (1...Nf6) 2.e4 e4)\n\n1...e5 ; inline ignored Qxg9\n2.Nf3 (2.Bc4 {x}) Nc6 *",
    );
    let imported = &games[0];
    assert!(imported.skipped_variations);
    assert_eq!(
        imported
            .game
            .line()
            .iter()
            .map(|m| m.san.as_str())
            .collect::<Vec<_>>(),
        ["e4", "e5", "Nf3", "Nc6"]
    );
    // Variation legality is intentionally not checked; its token syntax is.
    rejects("1.e4 (1.Qxg9) e5 *", "unknown token");
    rejects("1.e4 % not at column zero\ne5 *", "unknown token");
}

#[test]
fn decodes_bom_utf8_tags_quotes_and_backslashes() {
    let games = parse("\u{feff}[Event \"Турнир \\\"Слон\\\" \\\\ тест\"]\n[Result \"*\"]\n*");
    assert_eq!(games[0].tags["Event"], "Турнир \"Слон\" \\ тест");
    assert!(games[0].game.line().is_empty());
    assert!(
        parse_pgn(b"[Event \"\xff\"] *")
            .unwrap_err()
            .contains("UTF-8")
    );
}

#[test]
fn headerless_games_need_not_have_blank_line_separators() {
    let games = parse("1.e4 e5 * 1.d4 d5 *");
    assert_eq!(games.len(), 2);
    assert_eq!(games[1].game.line()[0].san, "d4");
}

#[test]
fn setup_fen_preserves_black_turn_and_move_numbers() {
    let mut imported = parse("[SetUp \"1\"] [FEN \"rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR b KQkq - 0 17\"] 17...e5 18.Nf3 *").remove(0);
    assert_eq!(imported.game.turn(), Color::Black);
    assert_eq!(imported.game.line()[0].number, 17);
    assert_eq!(imported.game.line()[1].number, 18);
    imported.game.seek(2).unwrap();
    assert_eq!(imported.game.turn(), Color::Black);
}

#[test]
fn castling_promotion_and_en_passant_use_san_legality() {
    let mut castles =
        parse("[SetUp \"1\"] [FEN \"r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1\"] 1.O-O O-O-O *")
            .remove(0)
            .game;
    castles.seek(2).unwrap();
    assert_eq!(castles.piece_at(Square::F1).unwrap().role, Role::Rook);
    assert_eq!(castles.piece_at(Square::D8).unwrap().role, Role::Rook);
    let mut promotes = parse("[SetUp \"1\"] [FEN \"7k/P7/8/8/8/8/8/7K w - - 0 1\"] 1.a8=N *")
        .remove(0)
        .game;
    promotes.seek(1).unwrap();
    assert_eq!(promotes.piece_at(Square::A8).unwrap().role, Role::Knight);
    let mut ep = parse("1.e4 a6 2.e5 d5 3.exd6 *").remove(0).game;
    ep.seek(5).unwrap();
    assert!(ep.piece_at(Square::D5).is_none());
    assert_eq!(ep.piece_at(Square::D6).unwrap().role, Role::Pawn);
}

#[test]
fn rejects_illegal_and_ambiguous_mainline_moves_with_location() {
    rejects("1.e4 e5 2.Bh6 *", "game 1, ply 3");
    rejects(
        "[SetUp \"1\"] [FEN \"7k/8/8/8/8/8/8/1N1N3K w - - 0 1\"] 1.Nc3 *",
        "ambiguous",
    );
    rejects("1.e4 * 1.d4 d5 2.Qh8 *", "game 2, ply 3");
}

#[test]
fn rejects_unknown_tokens_instead_of_silently_importing_a_prefix() {
    for input in [
        "1.e4 rubbish e5 *",
        "1.e4 e5 * garbage",
        "1.e4 123 e5 *",
        "1.e4 e5 @ *",
        "1.e4 e5 1-0suffix",
        "1.e4 e5 2..Nf3 *",
        "1.e4 e5 $256 *",
        "1.e4 e5 $a *",
        "1.e4 e5 !!! *",
    ] {
        assert!(parse_pgn(input.as_bytes()).is_err(), "accepted {input:?}");
    }
}

#[test]
fn rejects_malformed_truncated_and_unterminated_structures() {
    for input in [
        "",
        "   ",
        "{only a comment}",
        "*",
        "1.e4 e5",
        "[Event \"test\"]",
        "[Event test] *",
        "[Event \"test\" *",
        "[Event \"test] *",
        "[Event \"bad\\q\"] *",
        "[Event \"bad\nvalue\"] *",
        "1.e4 {unfinished *",
        "1.e4 (1.d4 *",
        "1.e4 ) e5 *",
        "1.e4 } e5 *",
        "1.e4 [Event \"another\"] *",
    ] {
        assert!(parse_pgn(input.as_bytes()).is_err(), "accepted {input:?}");
    }
}

#[test]
fn rejects_unsupported_variants_invalid_fen_and_conflicting_headers() {
    rejects("[Variant \"Chess960\"] 1.e4 *", "unsupported Variant");
    rejects("[SetUp \"1\"] [FEN \"not a fen\"] *", "Invalid FEN");
    rejects("[SetUp \"1\"] *", "requires a FEN");
    rejects("[FEN \"8/8/8/8/8/8/8/8 w - - 0 1\"] *", "requires SetUp");
    rejects("[SetUp \"2\"] *", "SetUp must");
    rejects("[Event \"one\"] [Event \"two\"] *", "duplicate tag");
    rejects("[Result \"1-0\"] 1.e4 0-1", "disagrees");
    rejects("[Result \"draw\"] *", "invalid Result");
    assert!(
        parse("[Variant \"Standard\"] [Result \"*\"] *")[0]
            .game
            .line()
            .is_empty()
    );
}

#[test]
fn enforces_byte_game_and_ply_bounds_before_chess_processing() {
    assert!(
        parse_pgn(&vec![b' '; MAX_PGN_BYTES + 1])
            .unwrap_err()
            .contains("byte limit")
    );
    let small = "[Event \"empty\"] *\n";
    assert_eq!(parse(&small.repeat(256)).len(), 256);
    rejects(&small.repeat(257), "256-game limit");
    rejects(&format!("{} *", "Nf3 ".repeat(2049)), "2048-ply");
    let many = format!("{} *\n", "Nf3 ".repeat(2000)).repeat(26);
    rejects(&many, "50000-ply");
}

#[test]
fn failed_multi_game_import_cannot_mutate_an_existing_game() {
    let mut existing = Game::default();
    existing.play_uci("d2d4").unwrap();
    let before = (
        existing.fen(),
        existing.revision(),
        existing.line().len(),
        existing.ply(),
    );
    let result = parse_pgn(b"1.e4 e5 * 1.d4 d5 2.Qh8 *");
    assert!(result.is_err());
    assert_eq!(
        before,
        (
            existing.fen(),
            existing.revision(),
            existing.line().len(),
            existing.ply()
        )
    );
}

#[test]
fn preserves_headerless_termination_without_inventing_tags() {
    let games = parse("1.e4 1-0 1.d4 0-1 1.Nf3 1/2-1/2 1.c4 *");
    assert_eq!(
        games
            .iter()
            .map(|game| game.result.as_str())
            .collect::<Vec<_>>(),
        ["1-0", "0-1", "1/2-1/2", "*"]
    );
    assert!(games.iter().all(|game| game.tags.is_empty()));
    let tagged = parse("[Result \"1-0\"] 1.e4 1-0");
    assert_eq!(tagged[0].result, tagged[0].tags["Result"]);
}

#[test]
fn validates_supplied_san_suffixes_but_accepts_omitted_suffixes() {
    rejects("1.e4+ *", "incorrect check or mate suffix");
    rejects("1.e4# *", "incorrect check or mate suffix");
    // A supplied suffix is precise: a mating move uses #, not +.
    rejects("1.f3 e5 2.g4 Qh4+ 0-1", "incorrect check or mate suffix");
    let games = parse("1.f3 e5 2.g4 Qh4# 0-1");
    assert_eq!(games[0].game.line()[3].san, "Qh4#");
    assert_eq!(parse("1.f3 e5 2.g4 Qh4 0-1")[0].game.line()[3].san, "Qh4#");
    let check = parse("1.e4 d5 2.Bb5+ *");
    assert_eq!(check[0].game.line()[2].san, "Bb5+");
}

#[test]
fn explicit_results_agree_with_definite_terminal_positions() {
    rejects("1.f3 e5 2.g4 Qh4# 1-0", "contradicts");
    rejects("1.f3 e5 2.g4 Qh4# 1/2-1/2", "contradicts");
    assert_eq!(parse("1.f3 e5 2.g4 Qh4# 0-1")[0].result, "0-1");
    assert_eq!(parse("1.f3 e5 2.g4 Qh4# *")[0].result, "*");
    let stalemate = "[SetUp \"1\"] [FEN \"7k/5K2/6Q1/8/8/8/8/8 b - - 0 1\"]";
    rejects(&format!("{stalemate} 1-0"), "contradicts");
    assert_eq!(parse(&format!("{stalemate} 1/2-1/2"))[0].result, "1/2-1/2");
    assert_eq!(parse(&format!("{stalemate} *"))[0].result, "*");
    // Nonterminal games can end by resignation, agreement, or an unfinished import.
    assert_eq!(parse("1.e4 1-0")[0].result, "1-0");
}

#[test]
fn bounds_source_derived_diagnostics_without_splitting_unicode() {
    let long_tag = "Tag".repeat(40_000);
    let cases = [
        (
            format!("{} *", "x".repeat(1_000_000)),
            "invalid SAN or unknown token",
            "ply 1",
        ),
        (
            format!("{} *", "ошибка".repeat(100_000)),
            "invalid SAN or unknown token",
            "ply 1",
        ),
        (
            format!("[Variant \"{}\"] 1.e4 *", "v".repeat(1_000_000)),
            "unsupported Variant",
            "ply 0",
        ),
        (
            format!("[{long_tag} \"one\"] [{long_tag} \"two\"] *"),
            "duplicate tag",
            "ply 0",
        ),
    ];
    for (input, expected_prefix, location) in cases {
        let error = parse_pgn(input.as_bytes()).unwrap_err();
        assert!(error.starts_with("PGN game 1,"), "missing game location");
        assert!(error.contains(location), "missing ply location: {error}");
        assert!(
            error.contains(expected_prefix),
            "missing error prefix: {error}"
        );
        assert!(
            error.ends_with("… [truncated]"),
            "missing truncation marker: {error}"
        );
        assert!(
            error.chars().count() <= 320,
            "diagnostic contains too many characters"
        );
    }
}
