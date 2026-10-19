// SPDX-License-Identifier: GPL-3.0-or-later
pub mod import;
mod review;
pub mod theme;
use chess_core::{Color, Game, Role, Square, square_at};
use iced::widget::{
    Column, Row, button, column, container, pick_list, responsive, row, scrollable, text,
    text_editor, text_input,
};
use iced::{Alignment, Border, Element, Length, Subscription, Task, keyboard};
pub use review::GameChoice;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    Time,
    Depth,
}
impl std::fmt::Display for SearchMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Time => "Time (ms)",
            Self::Depth => "Depth",
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PanelTab {
    #[default]
    Analysis,
    Moves,
}
pub struct App {
    review: review::Review,
    panel_tab: PanelTab,
    game: Game,
    selected: Option<Square>,
    promotion: Option<(Square, Square)>,
    flipped: bool,
    fen_input: String,
    notice: String,
    engine: engine::Engine,
    engine_path: String,
    engine_status: String,
    search_mode: SearchMode,
    search_limit: String,
    next_request: u64,
    active: Option<(u64, u64)>,
    analyzing: bool,
    info: engine::Info,
    best_move: Option<String>,
}
impl Default for App {
    fn default() -> Self {
        let game = Game::default();
        let fen_input = game.fen();
        let engine_path = engine::discover()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let engine_status = if engine_path.is_empty() {
            "No engine found. Enter an executable path."
        } else {
            "Engine found. Ready to analyze."
        }
        .into();
        Self {
            review: review::Review::default(),
            panel_tab: PanelTab::Analysis,
            engine: engine::Engine::new(),
            engine_path,
            engine_status,
            search_mode: SearchMode::Time,
            search_limit: "1000".into(),
            next_request: 0,
            active: None,
            analyzing: false,
            info: engine::Info::default(),
            best_move: None,
            game,
            selected: None,
            promotion: None,
            flipped: false,
            fen_input,
            notice: "Select a piece, then its destination. Ctrl+N new · Ctrl+Z undo · Ctrl+F flip"
                .into(),
        }
    }
}
#[derive(Debug, Clone)]
pub enum Message {
    Square(Square),
    Promote(Role),
    Cancel,
    New,
    Undo,
    Flip,
    FenChanged(String),
    LoadFen,
    CopyFen,
    EnginePath(String),
    SearchMode(SearchMode),
    SearchLimit(String),
    Analyze,
    Stop,
    Poll,
    Key(keyboard::Event),
    Seek(usize),
    Panel(PanelTab),
    OpenImport,
    CancelImport,
    OpenPgnFile,
    ImportPasted,
    PgnEdit(text_editor::Action),
    PgnLoaded(u64, Result<Option<import::ImportedPgn>, String>),
    SelectGame(GameChoice),
}
impl App {
    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            keyboard::listen().map(Message::Key),
            iced::time::every(std::time::Duration::from_millis(50)).map(|_| Message::Poll),
        ])
    }
    fn changed(&mut self) {
        self.stop_analysis();
        self.info = engine::Info::default();
        self.best_move = None;
        self.selected = None;
        self.promotion = None;
        self.fen_input = self.game.fen();
        self.notice = "Position updated".into();
    }
    fn stop_analysis(&mut self) {
        self.engine.stop();
        self.active = None;
        self.analyzing = false;
        self.engine_status = "Analysis stopped".into();
    }
    fn start_analysis(&mut self) {
        let limit = match self.search_limit.parse::<u32>() {
            Ok(n) if self.search_mode == SearchMode::Time && (1..=300_000).contains(&n) => {
                engine::Limit::TimeMs(n)
            }
            Ok(n) if self.search_mode == SearchMode::Depth && (1..=128).contains(&n) => {
                engine::Limit::Depth(n as u8)
            }
            _ => {
                self.notice = "Use 1–300000 milliseconds or depth 1–128".into();
                return;
            }
        };
        if self.engine_path.trim().is_empty() {
            self.notice = "Stockfish is optional. Enter its executable path to analyze.".into();
            return;
        }
        self.next_request = self.next_request.wrapping_add(1);
        self.active = Some((self.next_request, self.game.revision()));
        self.info = engine::Info::default();
        self.best_move = None;
        self.analyzing = true;
        self.engine_status = "Starting UCI engine…".into();
        self.engine.analyze(
            self.next_request,
            self.engine_path.trim().into(),
            self.game.fen(),
            self.game.turn() == Color::White,
            limit,
        );
    }
    fn receive(&mut self, event: engine::Event) {
        let id = match &event {
            engine::Event::Ready { id }
            | engine::Event::Info { id, .. }
            | engine::Event::BestMove { id, .. }
            | engine::Event::Stopped { id }
            | engine::Event::Error { id, .. } => *id,
        };
        if self.active != Some((id, self.game.revision())) {
            return;
        }
        match event {
            engine::Event::Ready { .. } => self.engine_status = "Analyzing locally…".into(),
            engine::Event::Info { info, .. } => {
                if info.depth.is_some() {
                    self.info.depth = info.depth;
                }
                if info.score.is_some() {
                    self.info.score = info.score;
                }
                if !info.pv.is_empty() {
                    self.info.pv = info.pv;
                }
            }
            engine::Event::BestMove { best_move, .. } => {
                self.best_move = best_move;
                self.analyzing = false;
                self.engine_status = "Analysis complete".into();
            }
            engine::Event::Stopped { .. } => {
                self.analyzing = false;
                self.engine_status = "Analysis stopped".into();
            }
            engine::Event::Error { message, .. } => {
                self.analyzing = false;
                self.engine_status = message;
            }
        }
    }
    fn play(&mut self, from: Square, to: Square, promotion: Option<Role>) {
        match self.game.play(from, to, promotion) {
            Ok(()) => {
                self.review.mark_edited();
                self.changed();
            }
            Err(err) => self.notice = err,
        }
    }
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Panel(tab) => self.panel_tab = tab,
            Message::OpenImport
            | Message::CancelImport
            | Message::OpenPgnFile
            | Message::ImportPasted
            | Message::PgnEdit(_)
            | Message::PgnLoaded(..)
            | Message::SelectGame(_) => return self.update_review(message),
            Message::Seek(ply) => match self.game.seek(ply) {
                Ok(true) => {
                    self.changed();
                    self.notice =
                        format!("Position {} of {}", self.game.ply(), self.game.line().len());
                }
                Ok(false) => {}
                Err(error) => self.notice = error,
            },
            Message::Square(square) => {
                if self.promotion.is_some() {
                    return Task::none();
                }
                if self.selected == Some(square) {
                    self.selected = None;
                } else if self
                    .game
                    .piece_at(square)
                    .is_some_and(|p| p.color == self.game.turn())
                {
                    self.selected = Some(square);
                } else if let Some(from) = self.selected {
                    if self.game.needs_promotion(from, square) {
                        self.promotion = Some((from, square));
                    } else {
                        self.play(from, square, None);
                    }
                }
            }
            Message::Promote(role) => {
                if let Some((from, to)) = self.promotion {
                    self.play(from, to, Some(role));
                }
            }
            Message::Cancel => {
                self.selected = None;
                self.promotion = None;
            }
            Message::New => {
                self.game.reset();
                self.review.detach();
                self.changed();
            }
            Message::Undo => {
                if self.game.undo() {
                    self.changed();
                } else {
                    self.notice = "No moves to undo".into();
                }
            }
            Message::Flip => self.flipped = !self.flipped,
            Message::FenChanged(value) => self.fen_input = value,
            Message::LoadFen => match self.game.load_fen(&self.fen_input) {
                Ok(()) => {
                    self.review.detach();
                    self.changed();
                }
                Err(err) => self.notice = err,
            },
            Message::CopyFen => {
                self.notice = "FEN copied to clipboard".into();
                return iced::clipboard::write(self.game.fen());
            }
            Message::EnginePath(path) => {
                self.stop_analysis();
                self.engine_path = path;
            }
            Message::SearchMode(mode) => {
                self.search_mode = mode;
                self.search_limit = match mode {
                    SearchMode::Time => "1000",
                    SearchMode::Depth => "18",
                }
                .into();
            }
            Message::SearchLimit(limit) => self.search_limit = limit,
            Message::Analyze => self.start_analysis(),
            Message::Stop => self.stop_analysis(),
            Message::Poll => {
                while let Some(event) = self.engine.try_recv() {
                    self.receive(event);
                }
            }
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if self.review.import_open {
                    if key.as_ref() == keyboard::Key::Named(keyboard::key::Named::Escape) {
                        return self.update(Message::CancelImport);
                    }
                    return Task::none();
                }
                let action = match key.as_ref() {
                    keyboard::Key::Named(keyboard::key::Named::Escape) => Some(Message::Cancel),
                    keyboard::Key::Character("o") if modifiers.command() => {
                        Some(Message::OpenImport)
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowLeft)
                        if modifiers.is_empty() =>
                    {
                        self.game.ply().checked_sub(1).map(Message::Seek)
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowRight)
                        if modifiers.is_empty() && self.game.ply() < self.game.line().len() =>
                    {
                        Some(Message::Seek(self.game.ply() + 1))
                    }
                    keyboard::Key::Named(keyboard::key::Named::Home) if modifiers.is_empty() => {
                        Some(Message::Seek(0))
                    }
                    keyboard::Key::Named(keyboard::key::Named::End) if modifiers.is_empty() => {
                        Some(Message::Seek(self.game.line().len()))
                    }
                    keyboard::Key::Character("n") if modifiers.command() => Some(Message::New),
                    keyboard::Key::Character("z") if modifiers.command() => Some(Message::Undo),
                    keyboard::Key::Character("f") if modifiers.command() => Some(Message::Flip),
                    _ => None,
                };
                if let Some(action) = action {
                    return self.update(action);
                }
            }
            Message::Key(_) => {}
        }
        Task::none()
    }
    fn board(&self, side: f32) -> Element<'_, Message> {
        let cell = (side - 24.0) / 8.0;
        let legal = self
            .selected
            .map(|s| self.game.destinations(s))
            .unwrap_or_default();
        let mut board = Column::new().spacing(0);
        for r in 0..8 {
            let rank = if self.flipped { r + 1 } else { 8 - r };
            let mut line = Row::new()
                .spacing(0)
                .align_y(Alignment::Center)
                .push(container(text(rank.to_string()).size(11).color(theme::MUTED)).width(24));
            for c in 0..8 {
                let square = square_at(r, c, self.flipped).expect("board coordinates");
                let mut background = if (r + c) % 2 == 0 {
                    theme::LIGHT
                } else {
                    theme::DARK
                };
                if self
                    .game
                    .last_move()
                    .is_some_and(|m| m.from == square || m.to == square)
                {
                    background = theme::LAST;
                }
                if self.selected == Some(square) {
                    background = theme::SELECTED;
                }
                let is_legal = legal.contains(&square);
                let glyph = self
                    .game
                    .piece_at(square)
                    .map(|p| piece_glyph(p.color, p.role))
                    .unwrap_or(if is_legal { "·" } else { "" });
                let checked = self.game.in_check()
                    && self
                        .game
                        .piece_at(square)
                        .is_some_and(|p| p.color == self.game.turn() && p.role == Role::King);
                let content = container(text(glyph).size(cell * 0.70).color(theme::INK))
                    .center_x(Length::Fill)
                    .center_y(Length::Fill);
                line = line.push(
                    button(content)
                        .width(cell)
                        .height(cell)
                        .padding(0)
                        .on_press(Message::Square(square))
                        .style(move |_, status| button::Style {
                            background: Some(background.into()),
                            text_color: theme::INK,
                            border: Border {
                                color: if checked {
                                    iced::Color::from_rgb8(216, 63, 63)
                                } else {
                                    theme::INK
                                },
                                width: if checked {
                                    3.0
                                } else if is_legal {
                                    2.0
                                } else if status == button::Status::Hovered {
                                    1.0
                                } else {
                                    0.0
                                },
                                radius: 0.0.into(),
                            },
                            ..Default::default()
                        }),
                );
            }
            board = board.push(line);
        }
        let mut files = Row::new().push(iced::widget::Space::new().width(24));
        for c in 0..8 {
            let file = if self.flipped { b'h' - c } else { b'a' + c };
            files = files.push(
                container(
                    text((file as char).to_string())
                        .size(11)
                        .color(theme::MUTED),
                )
                .center_x(cell),
            );
        }
        board.push(files).into()
    }
    fn move_list(&self) -> Element<'_, Message> {
        let mut history = Column::new().spacing(7);
        for (index, m) in self.game.line().iter().enumerate() {
            let prefix = if m.mover == Color::White {
                format!("{}.", m.number)
            } else {
                format!("{}…", m.number)
            };
            let current = self.game.ply() == index + 1;
            let future = index >= self.game.ply();
            history = history.push(
                button(row![text(prefix).width(44), text(&m.san)])
                    .width(Length::Fill)
                    .on_press(Message::Seek(index + 1))
                    .style(move |theme, status| {
                        let mut style = if current {
                            button::primary(theme, status)
                        } else {
                            button::text(theme, status)
                        };
                        if future {
                            style.text_color = crate::theme::MUTED;
                        }
                        style
                    }),
            );
        }
        if self.game.line().is_empty() {
            history = history.push(
                text("Your moves will appear here")
                    .size(13)
                    .color(theme::MUTED),
            );
        }
        column![
            text("MOVES · click to navigate")
                .size(12)
                .color(theme::ACCENT),
            history
        ]
        .spacing(8)
        .into()
    }
    fn engine_controls(&self) -> Element<'_, Message> {
        let score = match self.info.score {
            Some(engine::Score::Cp(cp)) => format!("{:+.2}", f64::from(cp) / 100.0),
            Some(engine::Score::Mate(0)) => format!(
                "M0 · {} wins",
                if self.game.turn() == Color::White {
                    "Black"
                } else {
                    "White"
                }
            ),
            Some(engine::Score::Mate(n)) => format!("M{n:+}"),
            None => "—".into(),
        };
        let pv = if self.info.pv.is_empty() {
            "Principal variation will appear here".into()
        } else {
            self.info.pv.join(" ")
        };
        let panel = column![
            text("LOCAL ANALYSIS").size(12).color(theme::ACCENT),
            text("Stockfish").size(26),
            text_input("Path to Stockfish executable", &self.engine_path)
                .on_input(Message::EnginePath)
                .size(12),
            row![
                pick_list(
                    [SearchMode::Time, SearchMode::Depth],
                    Some(self.search_mode),
                    Message::SearchMode
                ),
                text_input("Limit", &self.search_limit)
                    .on_input(Message::SearchLimit)
                    .on_submit(Message::Analyze)
                    .size(13)
            ]
            .spacing(6),
            row![
                button("Analyze").on_press(Message::Analyze),
                button("Stop").on_press_maybe(self.analyzing.then_some(Message::Stop))
            ]
            .spacing(8),
            text(&self.engine_status).size(12).color(theme::MUTED),
            row![
                text(score).size(32),
                text("White's perspective").size(12).color(theme::MUTED)
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            text(format!(
                "Depth: {} · Best: {}",
                self.info
                    .depth
                    .map(|d| d.to_string())
                    .unwrap_or_else(|| "—".into()),
                self.best_move.as_deref().unwrap_or("—")
            ))
            .size(13),
            text(pv).size(13),
        ]
        .spacing(12);
        panel.into()
    }
    fn panel(&self) -> Element<'_, Message> {
        let analysis = self.panel_tab == PanelTab::Analysis;
        let tabs = row![
            button("Analysis")
                .on_press(Message::Panel(PanelTab::Analysis))
                .style(move |theme, status| if analysis {
                    button::primary(theme, status)
                } else {
                    button::text(theme, status)
                }),
            button("Moves")
                .on_press(Message::Panel(PanelTab::Moves))
                .style(move |theme, status| if !analysis {
                    button::primary(theme, status)
                } else {
                    button::text(theme, status)
                }),
        ]
        .spacing(8);
        let mut panel = column![tabs, self.review_controls()].spacing(12);
        if self.promotion.is_some() {
            panel = panel
                .push(text("Choose promotion").color(theme::ACCENT))
                .push(
                    row![
                        button("Queen").on_press(Message::Promote(Role::Queen)),
                        button("Rook").on_press(Message::Promote(Role::Rook))
                    ]
                    .spacing(5),
                )
                .push(
                    row![
                        button("Bishop").on_press(Message::Promote(Role::Bishop)),
                        button("Knight").on_press(Message::Promote(Role::Knight)),
                        button("Cancel").on_press(Message::Cancel)
                    ]
                    .spacing(5),
                );
        }
        panel = panel.push(match self.panel_tab {
            PanelTab::Analysis => self.engine_controls(),
            PanelTab::Moves => self.move_list(),
        });
        scrollable(container(panel).padding(16).width(Length::Fill))
            .height(Length::Fill)
            .into()
    }
    pub fn view(&self) -> Element<'_, Message> {
        if self.review.import_open {
            return self.import_view();
        }
        let turn = if self.game.turn() == Color::White {
            "White"
        } else {
            "Black"
        };
        let header = row![
            column![
                text("CHESS COACH").size(24),
                text("A quiet place to think.").size(13).color(theme::MUTED)
            ]
            .spacing(4),
            iced::widget::Space::new().width(Length::Fill),
            text(format!("{turn} to move · {}", self.game.status())).size(16)
        ]
        .align_y(Alignment::Center);
        let toolbar = row![
            button("New game").on_press(Message::New),
            button("Undo").on_press(Message::Undo),
            button("Flip board").on_press(Message::Flip),
            button("Import PGN").on_press(Message::OpenImport)
        ]
        .spacing(8);
        let fen = row![
            text_input("FEN position", &self.fen_input)
                .on_input(Message::FenChanged)
                .on_submit(Message::LoadFen)
                .size(13),
            button("Load FEN").on_press(Message::LoadFen),
            button("Copy").on_press(Message::CopyFen)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        let body = responsive(move |size| {
            if size.width >= 800.0 {
                let side = (size.width - 338.0).min(size.height - 10.0).max(120.0);
                row![
                    container(self.board(side))
                        .width(Length::Fill)
                        .center_x(Length::Fill)
                        .center_y(Length::Fill),
                    container(self.panel()).width(310).height(Length::Fill)
                ]
                .spacing(16)
                .height(Length::Fill)
                .into()
            } else {
                let side = size.width.min(size.height * 0.65).max(120.0);
                column![
                    container(self.board(side)).center_x(Length::Fill),
                    self.panel()
                ]
                .spacing(10)
                .height(Length::Fill)
                .into()
            }
        });
        container(
            column![
                header,
                toolbar,
                fen,
                body,
                text(&self.notice).size(12).color(theme::MUTED)
            ]
            .spacing(16),
        )
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
    }
}
fn piece_glyph(color: Color, role: Role) -> &'static str {
    match (color, role) {
        (Color::White, Role::King) => "♔",
        (Color::White, Role::Queen) => "♕",
        (Color::White, Role::Rook) => "♖",
        (Color::White, Role::Bishop) => "♗",
        (Color::White, Role::Knight) => "♘",
        (Color::White, Role::Pawn) => "♙",
        (Color::Black, Role::King) => "♚",
        (Color::Black, Role::Queen) => "♛",
        (Color::Black, Role::Rook) => "♜",
        (Color::Black, Role::Bishop) => "♝",
        (Color::Black, Role::Knight) => "♞",
        (Color::Black, Role::Pawn) => "♟",
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn send(app: &mut App, message: Message) {
        let _ = app.update(message);
    }
    #[test]
    fn board_actions_and_fen_are_transactional() {
        let mut app = App::default();
        let start = app.game.fen();
        send(&mut app, Message::Square(Square::E2));
        send(&mut app, Message::Square(Square::E5));
        assert_eq!(app.game.fen(), start);
        send(&mut app, Message::Square(Square::E4));
        assert_eq!(app.game.history().len(), 1);
        let moved = app.game.fen();
        send(&mut app, Message::FenChanged("bad".into()));
        send(&mut app, Message::LoadFen);
        assert_eq!(app.game.fen(), moved);
        send(&mut app, Message::Undo);
        assert_eq!(app.game.fen(), start);
        send(&mut app, Message::New);
        assert_eq!(app.game.fen(), start);
        send(&mut app, Message::Flip);
        assert!(app.flipped);
    }
    #[test]
    fn analysis_events_require_both_request_and_position() {
        let mut app = App::default();
        app.active = Some((4, app.game.revision()));
        app.receive(engine::Event::Info {
            id: 3,
            info: engine::Info {
                score: Some(engine::Score::Cp(99)),
                ..Default::default()
            },
        });
        assert!(app.info.score.is_none());
        app.receive(engine::Event::Info {
            id: 4,
            info: engine::Info {
                score: Some(engine::Score::Cp(25)),
                ..Default::default()
            },
        });
        assert_eq!(app.info.score, Some(engine::Score::Cp(25)));
        send(&mut app, Message::New);
        app.receive(engine::Event::BestMove {
            id: 4,
            best_move: Some("e2e4".into()),
        });
        assert!(app.best_move.is_none());
        assert!(app.info.score.is_none());
        app.active = Some((5, app.game.revision()));
        app.game.play_uci("e2e4").unwrap();
        app.receive(engine::Event::Info {
            id: 5,
            info: engine::Info {
                score: Some(engine::Score::Mate(3)),
                ..Default::default()
            },
        });
        assert!(app.info.score.is_none());
    }
    #[test]
    fn missing_engine_leaves_board_functional() {
        let mut app = App::default();
        send(&mut app, Message::EnginePath(String::new()));
        send(&mut app, Message::Analyze);
        assert!(!app.analyzing);
        assert!(app.active.is_none());
        send(&mut app, Message::Square(Square::E2));
        send(&mut app, Message::Square(Square::E4));
        assert_eq!(app.game.history().len(), 1);
        send(&mut app, Message::Undo);
        assert!(app.game.history().is_empty());
    }
    #[test]
    fn invalid_search_limit_preserves_position() {
        let mut app = App::default();
        let fen = app.game.fen();
        send(&mut app, Message::SearchLimit("-1".into()));
        send(&mut app, Message::Analyze);
        assert_eq!(app.game.fen(), fen);
        assert!(!app.analyzing);
    }
    #[test]
    fn promotion_waits_for_choice() {
        let mut app = App::default();
        send(
            &mut app,
            Message::FenChanged("7k/P7/8/8/8/8/8/7K w - - 0 1".into()),
        );
        send(&mut app, Message::LoadFen);
        send(&mut app, Message::Square(Square::A7));
        send(&mut app, Message::Square(Square::A8));
        assert!(app.promotion.is_some());
        assert!(app.game.history().is_empty());
        send(&mut app, Message::Promote(Role::Knight));
        assert_eq!(app.game.piece_at(Square::A8).unwrap().role, Role::Knight);
    }
}
