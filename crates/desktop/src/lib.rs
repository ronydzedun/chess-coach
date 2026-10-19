// SPDX-License-Identifier: GPL-3.0-or-later
mod analysis_cache;
pub mod appearance;
mod appearance_ui;
mod evaluation;
pub mod import;
mod review;
mod settings_ui;
pub use settings_ui::SettingsTab;
pub mod theme;
use chess_core::{Color, Game, Role, Square, square_at};
use iced::widget::{
    Column, Row, button, column, container, pick_list, responsive, row, scrollable, text,
    text_editor, text_input,
};
use iced::{Alignment, Element, Length, Subscription, Task, keyboard};
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
    settings: Option<settings_ui::Dialog>,
    appearance: appearance::Settings,
    palette_name: String,
    palette_light: String,
    palette_dark: String,
    appearance_status: String,
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
    active_key: Option<analysis_cache::Key>,
    analysis_cache: analysis_cache::Cache,
    analyzing: bool,
    auto_analysis: bool,
    analysis_due: Option<std::time::Instant>,
    info: engine::Info,
    best_move: Option<String>,
}
impl Default for App {
    fn default() -> Self {
        let game = Game::default();
        let fen_input = game.fen();
        let (appearance, appearance_status) = match appearance::Settings::load() {
            Ok(settings) => (settings, String::new()),
            Err(error) => (
                appearance::Settings::default(),
                format!("Using default settings. {error}"),
            ),
        };
        let engine_path = appearance.engine_path.clone().unwrap_or_else(|| {
            engine::discover()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default()
        });
        let engine_status = if engine_path.is_empty() {
            "No engine found. Open Settings to configure Stockfish."
        } else {
            "Engine found. Ready to analyze."
        }
        .into();
        Self {
            settings: None,
            appearance,
            palette_name: String::new(),
            palette_light: "#F0D9B5".into(),
            palette_dark: "#B58863".into(),
            appearance_status,
            review: review::Review::default(),
            panel_tab: PanelTab::Analysis,
            engine: engine::Engine::new(),
            engine_path,
            engine_status,
            search_mode: SearchMode::Time,
            search_limit: "1000".into(),
            next_request: 0,
            active: None,
            active_key: None,
            analysis_cache: analysis_cache::Cache::default(),
            analyzing: false,
            auto_analysis: true,
            analysis_due: Some(std::time::Instant::now()),
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
    OpenSettings,
    CloseSettings,
    SettingsTab(SettingsTab),
    SettingsPath(String),
    SettingsThreads(String),
    SettingsHash(String),
    ApplySettings,
    SearchMode(SearchMode),
    SearchLimit(String),
    Analyze,
    ToggleAnalysis,
    Stop,
    Poll,
    Key(keyboard::Event),
    Seek(usize),
    Panel(PanelTab),
    PieceSet(appearance::PieceSet),
    Palette(String),
    PaletteName(String),
    PaletteLight(String),
    PaletteDark(String),
    SavePalette,
    DeletePalette,
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
        // Text inputs capture Escape to blur. The modal must close on that same key.
        let modal_escape = if self.settings.is_some() {
            iced::event::listen_with(|event, status, _| match event {
                iced::Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                    if status == iced::event::Status::Captured
                        && key.as_ref() == keyboard::Key::Named(keyboard::key::Named::Escape) =>
                {
                    Some(Message::CloseSettings)
                }
                _ => None,
            })
        } else {
            Subscription::none()
        };
        Subscription::batch([
            keyboard::listen().map(Message::Key),
            modal_escape,
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
        self.schedule_analysis();
    }
    fn schedule_analysis(&mut self) {
        self.stop_analysis();
        if self.restore_cached_analysis() {
            return;
        }
        if self.auto_analysis {
            self.analysis_due =
                Some(std::time::Instant::now() + std::time::Duration::from_millis(300));
        }
    }
    fn stop_analysis(&mut self) {
        self.analysis_due = None;
        self.engine.stop();
        self.active = None;
        self.active_key = None;
        self.analyzing = false;
        self.engine_status = "Analysis stopped".into();
    }
    fn analysis_key(&self) -> Result<analysis_cache::Key, String> {
        let limit = match self.search_limit.parse::<u32>() {
            Ok(n) if self.search_mode == SearchMode::Time && (1..=300_000).contains(&n) => {
                engine::Limit::TimeMs(n)
            }
            Ok(n) if self.search_mode == SearchMode::Depth && (1..=128).contains(&n) => {
                engine::Limit::Depth(n as u8)
            }
            _ => return Err("Use 1–300000 milliseconds or depth 1–128".into()),
        };
        if self.engine_path.trim().is_empty() {
            return Err("Stockfish is optional. Open Settings to configure its executable.".into());
        }
        Ok(analysis_cache::Key {
            fen: self.game.fen(),
            engine_path: self.engine_path.trim().into(),
            options: self.appearance.engine_options(),
            limit,
        })
    }
    fn restore_cached_analysis(&mut self) -> bool {
        let Some(result) = self
            .analysis_key()
            .ok()
            .and_then(|key| self.analysis_cache.get(&key))
        else {
            return false;
        };
        self.info = result.info;
        self.best_move = result.best_move;
        self.engine_status = "Cached analysis".into();
        true
    }
    fn start_analysis(&mut self, use_cache: bool) {
        self.analysis_due = None;
        if !self.auto_analysis {
            return;
        }
        if use_cache && self.restore_cached_analysis() {
            return;
        }
        let key = match self.analysis_key() {
            Ok(key) => key,
            Err(error) => {
                self.notice = error;
                return;
            }
        };
        self.next_request = self.next_request.wrapping_add(1);
        self.active = Some((self.next_request, self.game.revision()));
        self.active_key = Some(key.clone());
        self.info = engine::Info::default();
        self.best_move = None;
        self.analyzing = true;
        self.engine_status = "Preparing local analysis…".into();
        self.engine.analyze_with_options(
            self.next_request,
            key.engine_path.into(),
            key.fen,
            self.game.turn() == Color::White,
            key.limit,
            key.options,
        );
    }
    fn receive(&mut self, event: engine::Event) {
        let id = match &event {
            engine::Event::Ready { id }
            | engine::Event::Info { id, .. }
            | engine::Event::BestMove { id, .. }
            | engine::Event::Incomplete { id, .. }
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
                if let Some(key) = self.active_key.take() {
                    self.analysis_cache.insert(
                        key,
                        analysis_cache::Result {
                            info: self.info.clone(),
                            best_move: self.best_move.clone(),
                        },
                    );
                }
                self.active = None;
                self.analyzing = false;
                self.engine_status = "Analysis complete".into();
            }
            engine::Event::Incomplete { best_move, .. } => {
                self.best_move = best_move;
                self.active = None;
                self.active_key = None;
                self.analyzing = false;
                self.engine_status = "Search deadline exceeded · result not cached".into();
            }
            engine::Event::Stopped { .. } => {
                self.active = None;
                self.active_key = None;
                self.analyzing = false;
                self.engine_status = "Analysis stopped".into();
            }
            engine::Event::Error { message, .. } => {
                self.active = None;
                self.active_key = None;
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
        if self.settings.is_some()
            && !matches!(
                &message,
                Message::OpenSettings
                    | Message::CloseSettings
                    | Message::SettingsTab(_)
                    | Message::SettingsPath(_)
                    | Message::SettingsThreads(_)
                    | Message::SettingsHash(_)
                    | Message::ApplySettings
                    | Message::PieceSet(_)
                    | Message::Palette(_)
                    | Message::PaletteName(_)
                    | Message::PaletteLight(_)
                    | Message::PaletteDark(_)
                    | Message::SavePalette
                    | Message::DeletePalette
                    | Message::Poll
                    | Message::Key(_)
            )
        {
            return Task::none();
        }
        match message {
            Message::OpenSettings
            | Message::CloseSettings
            | Message::SettingsTab(_)
            | Message::SettingsPath(_)
            | Message::SettingsThreads(_)
            | Message::SettingsHash(_)
            | Message::ApplySettings => return self.update_settings(message),
            Message::Panel(tab) => self.panel_tab = tab,
            Message::PieceSet(_)
            | Message::Palette(_)
            | Message::PaletteName(_)
            | Message::PaletteLight(_)
            | Message::PaletteDark(_)
            | Message::SavePalette
            | Message::DeletePalette => return self.update_appearance(message),
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
                self.engine.reset();
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
                    self.engine.reset();
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
                self.info = engine::Info::default();
                self.best_move = None;
                self.schedule_analysis();
            }
            Message::SearchMode(mode) => {
                self.info = engine::Info::default();
                self.best_move = None;
                self.search_mode = mode;
                self.search_limit = match mode {
                    SearchMode::Time => "1000",
                    SearchMode::Depth => "18",
                }
                .into();
                self.schedule_analysis();
            }
            Message::SearchLimit(limit) => {
                self.info = engine::Info::default();
                self.best_move = None;
                self.search_limit = limit;
                self.schedule_analysis();
            }
            Message::Analyze => {
                self.stop_analysis();
                self.start_analysis(false);
            }
            Message::ToggleAnalysis => {
                self.auto_analysis = !self.auto_analysis;
                self.schedule_analysis();
            }
            Message::Stop => {
                self.auto_analysis = false;
                self.stop_analysis();
            }
            Message::Poll => {
                if self
                    .analysis_due
                    .is_some_and(|due| std::time::Instant::now() >= due)
                {
                    self.start_analysis(true);
                }
                while let Some(event) = self.engine.try_recv() {
                    self.receive(event);
                }
            }
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
                if self.settings.is_some() {
                    if key.as_ref() == keyboard::Key::Named(keyboard::key::Named::Escape) {
                        return self.update(Message::CloseSettings);
                    }
                    return Task::none();
                }
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
        let side = side - 48.0;
        let cell = (side - 24.0) / 8.0;
        let (light, dark) = self.appearance.active_palette().colors();
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
                let mut background = if (r + c) % 2 == 0 { light } else { dark };
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
                let checked = self.game.in_check()
                    && self
                        .game
                        .piece_at(square)
                        .is_some_and(|p| p.color == self.game.turn() && p.role == Role::King);
                if is_legal && self.game.piece_at(square).is_some() {
                    background = theme::tint(background, theme::INK, 0.20);
                }
                if checked {
                    background = theme::tint(background, iced::Color::from_rgb8(220, 55, 55), 0.65);
                }
                let piece: Element<'_, Message> = match self.game.piece_at(square) {
                    Some(p) => iced::widget::svg(iced::widget::svg::Handle::from_memory(
                        self.appearance.piece_set.svg_bytes(p.color, p.role),
                    ))
                    .width(cell * 0.9)
                    .height(cell * 0.9)
                    .into(),
                    None => container(iced::widget::Space::new())
                        .width(cell * 0.20)
                        .height(cell * 0.20)
                        .style(move |_| container::Style {
                            background: is_legal.then_some(
                                iced::Color {
                                    a: 0.24,
                                    ..theme::INK
                                }
                                .into(),
                            ),
                            border: iced::Border {
                                radius: (cell * 0.10).into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        })
                        .into(),
                };
                let content = container(piece)
                    .center_x(Length::Fill)
                    .center_y(Length::Fill);
                line = line.push(
                    button(content)
                        .width(cell)
                        .height(cell)
                        .padding(0)
                        .on_press(Message::Square(square))
                        .style(move |_, status| button::Style {
                            background: Some(
                                if status == button::Status::Hovered {
                                    theme::tint(background, theme::INK, 0.08)
                                } else {
                                    background
                                }
                                .into(),
                            ),
                            text_color: theme::INK,
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
        row![
            evaluation::view(self.info.score, self.game.turn(), self.flipped, cell * 8.0),
            board.push(files)
        ]
        .spacing(8)
        .into()
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
                button(if self.auto_analysis {
                    "Analysis: ON"
                } else {
                    "Analysis: OFF"
                })
                .on_press(Message::ToggleAnalysis),
                button("Reanalyze")
                    .on_press_maybe(self.auto_analysis.then_some(Message::Analyze))
                    .style(button::secondary),
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
        let moves = self.panel_tab == PanelTab::Moves;
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
                .style(move |theme, status| if moves {
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
        scrollable(
            container(panel)
                .padding(16)
                .width(Length::Fill)
                .style(container::rounded_box),
        )
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
            button("New").on_press(Message::New),
            button("Undo").on_press(Message::Undo),
            button("Flip").on_press(Message::Flip),
            button("Import PGN").on_press(Message::OpenImport),
            button("Settings").on_press(Message::OpenSettings)
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
        let base = container(
            column![
                header,
                toolbar,
                fen,
                body,
                text(if self.appearance_status.starts_with("Using default") {
                    format!("{} · {}", self.notice, self.appearance_status)
                } else {
                    self.notice.clone()
                })
                .size(12)
                .color(theme::MUTED)
            ]
            .spacing(16),
        )
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .into();
        if self.settings.is_some() {
            self.settings_view(base)
        } else {
            base
        }
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
    fn automatic_analysis_is_scheduled_and_toggle_cancels_it() {
        let mut app = App::default();
        assert!(app.auto_analysis);
        assert!(app.analysis_due.is_some());
        send(&mut app, Message::ToggleAnalysis);
        assert!(!app.auto_analysis);
        assert!(app.analysis_due.is_none());
        send(&mut app, Message::Analyze);
        assert!(!app.analyzing);
        assert!(app.active.is_none());
        send(&mut app, Message::Square(Square::E2));
        send(&mut app, Message::Square(Square::E4));
        assert!(app.analysis_due.is_none());
        send(&mut app, Message::ToggleAnalysis);
        assert!(app.analysis_due.is_some());
        app.active = Some((7, app.game.revision()));
        app.info.score = Some(engine::Score::Cp(50));
        send(&mut app, Message::Undo);
        assert!(app.active.is_none());
        assert!(app.info.score.is_none());
        assert!(app.analysis_due.is_some());
    }
    #[test]
    fn automatic_analysis_failure_does_not_retry_every_poll() {
        let mut app = App::default();
        app.engine_path.clear();
        app.analysis_due = Some(std::time::Instant::now());
        send(&mut app, Message::Poll);
        assert!(app.analysis_due.is_none());
        assert!(!app.analyzing);
        let requests = app.next_request;
        send(&mut app, Message::Poll);
        assert_eq!(app.next_request, requests);
    }
    fn cache_app() -> App {
        App {
            engine_path: "/nonexistent/chess-coach-cache-test-engine".into(),
            ..App::default()
        }
    }
    fn completed(app: &mut App, id: u64, cp: i32) {
        app.analysis_due = None;
        app.analyzing = true;
        app.active = Some((id, app.game.revision()));
        app.active_key = Some(app.analysis_key().unwrap());
        app.info = engine::Info::default();
        app.receive(engine::Event::Info {
            id,
            info: engine::Info {
                depth: Some(18),
                score: Some(engine::Score::Cp(cp)),
                pv: vec!["e2e4".into(), "e7e5".into()],
            },
        });
        app.receive(engine::Event::BestMove {
            id,
            best_move: Some(
                if app.game.turn() == Color::White {
                    "e2e4"
                } else {
                    "e7e5"
                }
                .into(),
            ),
        });
    }
    #[test]
    fn returning_to_completed_positions_restores_immediately_even_when_off() {
        let mut app = cache_app();
        completed(&mut app, 10, 35);
        send(&mut app, Message::Square(Square::E2));
        send(&mut app, Message::Square(Square::E4));
        assert!(app.analysis_due.is_some());
        assert!(app.info.score.is_none());
        completed(&mut app, 11, -125);
        let requests = app.next_request;
        send(&mut app, Message::Seek(0));
        assert_eq!(app.info.score, Some(engine::Score::Cp(35)));
        assert_eq!(app.info.depth, Some(18));
        assert_eq!(app.best_move.as_deref(), Some("e2e4"));
        assert!(app.analysis_due.is_none() && app.active.is_none() && !app.analyzing);
        assert_eq!(app.engine_status, "Cached analysis");
        send(&mut app, Message::Seek(1));
        assert_eq!(app.info.score, Some(engine::Score::Cp(-125)));
        assert_eq!(app.best_move.as_deref(), Some("e7e5"));
        send(&mut app, Message::ToggleAnalysis);
        send(&mut app, Message::Undo);
        assert!(!app.auto_analysis);
        assert_eq!(app.info.score, Some(engine::Score::Cp(35)));
        assert!(app.analysis_due.is_none());
        app.receive(engine::Event::BestMove {
            id: 11,
            best_move: Some("a7a8q".into()),
        });
        app.receive(engine::Event::Error {
            id: 10,
            message: "old failure".into(),
        });
        assert_eq!(app.best_move.as_deref(), Some("e2e4"));
        assert_eq!(app.next_request, requests);
        assert_eq!(app.engine_status, "Cached analysis");
    }
    #[test]
    fn interrupted_failed_and_stale_searches_do_not_create_results() {
        let mut app = cache_app();
        app.active = Some((20, app.game.revision()));
        app.active_key = Some(app.analysis_key().unwrap());
        app.receive(engine::Event::Info {
            id: 20,
            info: engine::Info {
                score: Some(engine::Score::Cp(999)),
                ..Default::default()
            },
        });
        send(&mut app, Message::Square(Square::E2));
        send(&mut app, Message::Square(Square::E4));
        app.receive(engine::Event::BestMove {
            id: 20,
            best_move: Some("e2e4".into()),
        });
        send(&mut app, Message::Undo);
        assert!(app.info.score.is_none() && app.analysis_due.is_some());
        assert!(!app.restore_cached_analysis());
        app.active = Some((22, app.game.revision()));
        app.active_key = Some(app.analysis_key().unwrap());
        app.receive(engine::Event::Incomplete {
            id: 22,
            best_move: Some("e2e4".into()),
        });
        assert!(!app.restore_cached_analysis());
        assert!(app.active_key.is_none());
        app.active = Some((21, app.game.revision()));
        app.active_key = Some(app.analysis_key().unwrap());
        app.receive(engine::Event::Error {
            id: 21,
            message: "failed".into(),
        });
        assert!(!app.restore_cached_analysis());
    }
    #[test]
    fn cache_respects_settings_and_manual_reanalysis_bypasses_it() {
        let mut app = cache_app();
        completed(&mut app, 30, 70);
        send(&mut app, Message::SearchLimit("2000".into()));
        assert!(app.info.score.is_none() && app.analysis_due.is_some());
        send(&mut app, Message::SearchLimit("1000".into()));
        assert_eq!(app.info.score, Some(engine::Score::Cp(70)));
        assert!(app.analysis_due.is_none());
        let old_path = app.engine_path.clone();
        send(
            &mut app,
            Message::EnginePath("/another/missing-engine".into()),
        );
        assert!(app.info.score.is_none() && app.analysis_due.is_some());
        send(&mut app, Message::EnginePath(old_path));
        assert_eq!(app.info.score, Some(engine::Score::Cp(70)));
        let requests = app.next_request;
        send(&mut app, Message::Analyze);
        assert_eq!(app.next_request, requests + 1);
        assert!(app.analyzing && app.active_key.is_some());
        assert!(app.info.score.is_none());
        send(&mut app, Message::Stop);
        assert!(app.active_key.is_none());
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
