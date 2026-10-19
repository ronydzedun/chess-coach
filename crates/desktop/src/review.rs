// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{App, Message, import, theme};
use chess_core::pgn::PgnGame;
use iced::widget::{button, column, container, pick_list, row, text, text_editor};
use iced::{Element, Length, Task};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameChoice {
    pub index: usize,
    label: String,
}
impl std::fmt::Display for GameChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Default)]
pub(super) struct Review {
    games: Arc<Vec<PgnGame>>,
    choices: Vec<GameChoice>,
    selected: Option<GameChoice>,
    source: String,
    modified: bool,
    pub(super) import_open: bool,
    editor: text_editor::Content,
    error: String,
    next_import: u64,
    pending: Option<u64>,
}
impl Review {
    pub(super) fn detach(&mut self) {
        self.selected = None;
        self.modified = false;
    }
    pub(super) fn mark_edited(&mut self) {
        self.modified = self.selected.is_some();
    }
    fn begin_import(&mut self) -> u64 {
        self.next_import = self.next_import.wrapping_add(1);
        self.pending = Some(self.next_import);
        self.error.clear();
        self.next_import
    }
}
impl App {
    pub(super) fn update_review(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenImport => {
                self.review.import_open = true;
                self.review.error.clear();
            }
            Message::CancelImport => {
                self.review.import_open = false;
                self.review.pending = None;
            }
            Message::PgnEdit(action) => {
                if self.review.pending.is_none() {
                    self.review.editor.perform(action);
                    self.review.error.clear();
                }
            }
            Message::ImportPasted => {
                if !self.review.import_open || self.review.pending.is_some() {
                    return Task::none();
                }
                let text = self.review.editor.text();
                let id = self.review.begin_import();
                return Task::perform(import::pasted(text), move |result| {
                    Message::PgnLoaded(id, result.map(Some))
                });
            }
            Message::OpenPgnFile => {
                if !self.review.import_open || self.review.pending.is_some() {
                    return Task::none();
                }
                let id = self.review.begin_import();
                // Construct the native dialog on the UI thread (important on macOS), then await it.
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title("Import PGN")
                    .add_filter("Chess games", &["pgn"])
                    .pick_file();
                return Task::perform(
                    async move {
                        match dialog.await {
                            Some(file) => import::file(file.path().to_path_buf()).await.map(Some),
                            None => Ok(None),
                        }
                    },
                    move |result| Message::PgnLoaded(id, result),
                );
            }
            Message::PgnLoaded(id, result) => {
                if !self.review.import_open || self.review.pending != Some(id) {
                    return Task::none();
                }
                self.review.pending = None;
                match result {
                    Ok(Some(loaded)) if !loaded.games.is_empty() => {
                        let choices = loaded
                            .games
                            .iter()
                            .enumerate()
                            .map(|(index, pgn)| {
                                let tag = |key: &str| {
                                    pgn.tags
                                        .get(key)
                                        .map(|s| s.chars().take(32).collect::<String>())
                                        .unwrap_or_else(|| "?".into())
                                };
                                GameChoice {
                                    index,
                                    label: format!(
                                        "{}. {} — {} ({})",
                                        index + 1,
                                        tag("White"),
                                        tag("Black"),
                                        pgn.result
                                    ),
                                }
                            })
                            .collect();
                        self.review.games = loaded.games;
                        self.review.source = loaded.source;
                        self.review.choices = choices;
                        self.review.import_open = false;
                        self.panel_tab = crate::PanelTab::Moves;
                        self.select_game(0);
                        self.notice = format!(
                            "Imported {} game(s). Select moves to review; original PGN is unchanged.",
                            self.review.games.len()
                        );
                    }
                    Ok(Some(_)) => self.review.error = "No games found in PGN".into(),
                    Ok(None) => {}
                    Err(error) => self.review.error = error,
                }
            }
            Message::SelectGame(choice) => {
                self.select_game(choice.index);
            }
            _ => {}
        }
        Task::none()
    }
    pub(super) fn seek_report_position(&mut self, ply: usize) {
        let Some(choice) = &self.review.selected else {
            return;
        };
        let Some(original) = self.review.games.get(choice.index) else {
            return;
        };
        if ply > original.game.line().len() {
            return;
        }
        if self.review.modified {
            // The report belongs to the imported main line, not a working branch.
            self.game.load_line(&original.game);
            self.game.seek(ply).expect("validated report index");
            self.review.modified = false;
            self.changed();
        } else {
            let _ = self.update(Message::Seek(ply));
        }
    }
    fn select_game(&mut self, index: usize) {
        let games = Arc::clone(&self.review.games);
        let Some(pgn) = games.get(index) else {
            self.notice = "Imported game is unavailable".into();
            return;
        };
        self.stop_analysis();
        self.engine.reset();
        self.game.load_line(&pgn.game);
        self.review.selected = self.review.choices.get(index).cloned();
        self.review.modified = false;
        self.begin_game_analysis();
        self.changed();
        if self.auto_analysis && self.analysis_due.is_some() {
            self.analysis_due = Some(std::time::Instant::now());
        }
        self.notice = format!("Game {} · starting position", index + 1);
    }
    pub(super) fn review_controls(&self) -> Element<'_, Message> {
        let ply = self.game.ply();
        let end = self.game.line().len();
        let mut content = column![text("REVIEW").size(12).color(theme::ACCENT)].spacing(8);
        if !self.review.choices.is_empty() {
            content = content
                .push(text(&self.review.source).size(12).color(theme::MUTED))
                .push(
                    pick_list(
                        self.review.choices.as_slice(),
                        self.review.selected.as_ref(),
                        Message::SelectGame,
                    )
                    .placeholder("Choose imported game")
                    .text_size(12)
                    .width(Length::Fill),
                );
            if let Some(choice) = &self.review.selected {
                content = content
                    .push(button("Reload original").on_press(Message::SelectGame(choice.clone())));
                if self.review.games[choice.index].skipped_variations {
                    content = content.push(
                        text("Main line only · side variations skipped")
                            .size(12)
                            .color(theme::MUTED),
                    );
                }
            }
            if self.review.modified {
                content = content.push(
                    text("Working line edited. Reload original to restore.")
                        .size(12)
                        .color(theme::SELECTED),
                );
            }
        }
        if let Some(batch) = &self.game_analysis {
            content = content.push(
                text(batch.status(self.auto_analysis))
                    .size(12)
                    .color(theme::MUTED),
            );
        }
        content
            .push(
                row![
                    button("Start").on_press_maybe((ply > 0).then_some(Message::Seek(0))),
                    button("Back").on_press_maybe(ply.checked_sub(1).map(Message::Seek)),
                    button("Next").on_press_maybe((ply < end).then_some(Message::Seek(ply + 1))),
                    button("End").on_press_maybe((ply < end).then_some(Message::Seek(end))),
                ]
                .spacing(6),
            )
            .push(
                text(format!("Position {ply} of {end}"))
                    .size(12)
                    .color(theme::MUTED),
            )
            .into()
    }
    pub(super) fn import_view(&self) -> Element<'_, Message> {
        let idle = self.review.pending.is_none();
        let mut editor = text_editor(&self.review.editor)
            .placeholder("Paste a PGN game or collection here…")
            .size(14)
            .height(Length::Fill);
        if idle {
            editor = editor.on_action(Message::PgnEdit);
        }
        let status = if !self.review.error.is_empty() {
            self.review.error.as_str()
        } else if !idle {
            "Importing… Cancel keeps your current position."
        } else {
            "Up to 8 MiB · 256 games · main lines only. Comments and side variations are not displayed."
        };
        container(column![
            text("Import PGN").size(26),
            text("Open a local file or paste PGN text. Your current game changes only after a successful import.").size(14).color(theme::MUTED),
            button("Open PGN file…").on_press_maybe(idle.then_some(Message::OpenPgnFile)),
            editor,
            text(status).size(13).color(if self.review.error.is_empty(){theme::MUTED}else{theme::SELECTED}),
            row![button("Import pasted PGN").on_press_maybe(idle.then_some(Message::ImportPasted)),button("Cancel").on_press(Message::CancelImport)].spacing(8),
        ].spacing(16)).padding(24).width(Length::Fill).height(Length::Fill).into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chess_core::{Game, Square, pgn::parse_pgn};
    fn send(app: &mut App, message: Message) {
        let _ = app.update(message);
    }
    fn loaded(text: &str) -> import::ImportedPgn {
        import::ImportedPgn {
            games: Arc::new(parse_pgn(text.as_bytes()).unwrap()),
            source: "Fixture".into(),
        }
    }
    fn apply(app: &mut App, text: &str) {
        send(app, Message::OpenImport);
        let id = app.review.begin_import();
        send(app, Message::PgnLoaded(id, Ok(Some(loaded(text)))));
    }
    #[test]
    fn imported_line_is_navigable_and_original_survives_branching() {
        let mut app = App::default();
        apply(&mut app, "1. e4 e5 2. Nf3 *");
        assert_eq!(app.game.ply(), 0);
        assert_eq!(app.game.line().len(), 3);
        assert!(app.game_analysis.is_some());
        assert!(
            app.analysis_due
                .is_some_and(|due| due <= std::time::Instant::now())
        );
        send(&mut app, Message::Seek(3));
        assert_eq!(app.game.history().len(), 3);
        send(&mut app, Message::Undo);
        assert_eq!(app.game.ply(), 2);
        assert_eq!(app.game.line().len(), 3);
        send(&mut app, Message::Seek(0));
        send(&mut app, Message::Square(Square::D2));
        send(&mut app, Message::Square(Square::D4));
        assert_eq!(app.game.line().len(), 1);
        assert!(app.review.modified);
        assert_eq!(app.review.games[0].game.line().len(), 3);
        let choice = app.review.choices[0].clone();
        send(&mut app, Message::SelectGame(choice));
        assert_eq!(app.game.ply(), 0);
        assert_eq!(app.game.line().len(), 3);
        assert!(!app.review.modified);
    }
    #[test]
    fn report_navigation_restores_original_line_without_replacing_analysis() {
        let mut app = App::default();
        apply(&mut app, "1. e4 e5 2. Nf3 *");
        let key = app.analysis_key().unwrap();
        let result = crate::analysis_cache::Result {
            info: engine::Info {
                score: Some(engine::Score::Cp(42)),
                ..Default::default()
            },
            best_move: Some("e2e4".into()),
        };
        app.game_analysis
            .as_mut()
            .unwrap()
            .record(&key, Some(result));
        send(&mut app, Message::Panel(crate::PanelTab::Report));
        send(&mut app, Message::Square(Square::D2));
        send(&mut app, Message::Square(Square::D4));
        assert!(app.review.modified);
        let branch = app.game.fen();
        send(&mut app, Message::ReportSeek(999));
        assert_eq!(app.game.fen(), branch);
        send(&mut app, Message::ReportSeek(3));
        assert!(!app.review.modified);
        assert_eq!(app.game.ply(), 3);
        assert_eq!(app.game.line()[0].uci, "e2e4");
        assert_eq!(app.game.line().len(), 3);
        assert!(app.game_analysis.as_ref().unwrap().get(&key).is_some());
        send(&mut app, Message::ReportSeek(0));
        assert_eq!(app.info.score, Some(engine::Score::Cp(42)));
        assert_eq!(app.panel_tab, crate::PanelTab::Report);
        let _ = app.view();
        send(&mut app, Message::New);
        let _ = app.view();
        assert!(app.game_analysis.is_none());
    }
    #[test]
    fn review_tab_and_headerless_result_are_preserved() {
        let mut app = App::default();
        apply(&mut app, "1.e4 1-0");
        assert_eq!(app.panel_tab, crate::PanelTab::Moves);
        assert!(app.review.choices[0].label.ends_with("(1-0)"));
        app.active = Some((8, app.game.revision()));
        let fen = app.game.fen();
        send(&mut app, Message::Panel(crate::PanelTab::Analysis));
        assert_eq!(app.active.unwrap().0, 8);
        assert_eq!(app.game.fen(), fen);
        send(&mut app, Message::Panel(crate::PanelTab::Moves));
        assert_eq!(app.game.line().len(), 1);
    }
    #[test]
    fn failed_cancelled_or_stale_import_keeps_current_game_and_analysis() {
        let mut app = App::default();
        app.game.play_uci("e2e4").unwrap();
        let old = app.game.fen();
        app.active = Some((44, app.game.revision()));
        app.info.score = Some(engine::Score::Cp(25));
        send(&mut app, Message::OpenImport);
        let id = app.review.begin_import();
        send(&mut app, Message::PgnLoaded(id, Err("Invalid PGN".into())));
        assert_eq!(app.game.fen(), old);
        assert!(app.review.import_open);
        assert_eq!(app.active.unwrap().0, 44);
        let id = app.review.begin_import();
        send(&mut app, Message::CancelImport);
        send(
            &mut app,
            Message::PgnLoaded(id, Ok(Some(loaded("1. d4 *")))),
        );
        assert_eq!(app.game.fen(), old);
        assert_eq!(app.info.score, Some(engine::Score::Cp(25)));
        send(&mut app, Message::OpenImport);
        let newer = app.review.begin_import();
        send(
            &mut app,
            Message::PgnLoaded(id, Ok(Some(loaded("1. d4 *")))),
        );
        assert_eq!(app.review.pending, Some(newer));
        assert_eq!(app.game.fen(), old);
    }
    #[test]
    fn switching_games_cancels_background_and_ignores_its_late_result() {
        let mut app = App {
            engine_path: "/nonexistent/queue-switch".into(),
            ..App::default()
        };
        apply(&mut app, "1. e4 *\n\n1. d4 *");
        app.analysis_due = None;
        app.next_request = 900;
        app.active = Some((900, app.game.revision()));
        app.active_key = Some(app.analysis_key().unwrap());
        app.receive(engine::Event::Info {
            id: 900,
            info: engine::Info {
                score: Some(engine::Score::Cp(25)),
                ..Default::default()
            },
        });
        app.receive(engine::Event::BestMove {
            id: 900,
            best_move: Some("e2e4".into()),
        });
        app.start_background_analysis();
        let old = app.background.as_ref().unwrap().id;
        let choice = app.review.choices[1].clone();
        send(&mut app, Message::SelectGame(choice));
        assert!(app.background.is_none());
        app.receive(engine::Event::Info {
            id: old,
            info: engine::Info {
                score: Some(engine::Score::Cp(999)),
                ..Default::default()
            },
        });
        app.receive(engine::Event::BestMove {
            id: old,
            best_move: Some("e2e4".into()),
        });
        assert_eq!(app.info.score, Some(engine::Score::Cp(25)));
        assert!(
            app.game_analysis
                .as_ref()
                .unwrap()
                .status(true)
                .contains("0/2 positions")
        );
        assert_eq!(app.game.line()[0].uci, "d2d4");
        app.start_background_analysis();
        assert!(app.background.is_some());
    }
    #[test]
    fn navigation_and_game_switch_invalidate_analysis() {
        let mut app = App::default();
        apply(
            &mut app,
            "[White \"A\"]\n\n1. e4 *\n\n[White \"B\"]\n\n1. d4 *",
        );
        app.active = Some((7, app.game.revision()));
        app.info.score = Some(engine::Score::Cp(99));
        send(&mut app, Message::Seek(1));
        assert!(app.active.is_none());
        assert!(app.info.score.is_none());
        app.receive(engine::Event::Info {
            id: 7,
            info: engine::Info {
                score: Some(engine::Score::Cp(-99)),
                ..Default::default()
            },
        });
        assert!(app.info.score.is_none());
        let choice = app.review.choices[1].clone();
        send(&mut app, Message::SelectGame(choice));
        assert_eq!(app.game.fen(), Game::default().fen());
        assert_eq!(app.game.line()[0].san, "d4");
        let revision = app.game.revision();
        send(&mut app, Message::Seek(500));
        assert_eq!(app.game.revision(), revision);
    }
}
