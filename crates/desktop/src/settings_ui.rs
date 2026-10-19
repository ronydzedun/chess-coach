// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{App, Message, appearance, theme};
use iced::widget::{
    button, column, container, mouse_area, opaque, responsive, row, scrollable, stack, text,
    text_input,
};
use iced::{Alignment, Element, Length, Task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsTab {
    Engine,
    Appearance,
}
pub(super) struct Dialog {
    pub tab: SettingsTab,
    pub path: String,
    pub error: String,
}
impl App {
    pub(super) fn update_settings(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::OpenSettings => {
                if self.settings.is_none() {
                    self.settings = Some(Dialog {
                        tab: SettingsTab::Engine,
                        path: self.appearance.engine_path.clone().unwrap_or_default(),
                        error: String::new(),
                    });
                    return iced::widget::operation::focus("settings-engine-path");
                }
            }
            Message::CloseSettings => self.settings = None,
            Message::SettingsTab(tab) => {
                if let Some(dialog) = &mut self.settings {
                    dialog.tab = tab;
                }
            }
            Message::SettingsPath(path) => {
                if let Some(dialog) = &mut self.settings {
                    dialog.path = path;
                    dialog.error.clear();
                }
            }
            Message::ApplySettings => {
                let Some(dialog) = &mut self.settings else {
                    return Task::none();
                };
                let candidate = match engine_settings(&self.appearance, &dialog.path) {
                    Ok(candidate) => candidate,
                    Err(error) => {
                        dialog.error = error;
                        return Task::none();
                    }
                };
                if let Err(error) = candidate.save() {
                    dialog.error = format!("Could not save settings: {error}");
                    return Task::none();
                }
                let effective_path = candidate.engine_path.clone().unwrap_or_else(|| {
                    engine::discover()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
                self.appearance = candidate;
                self.settings = None;
                if self.engine_path != effective_path {
                    let _ = self.update(Message::EnginePath(effective_path));
                }
                self.notice = "Engine settings saved".into();
            }
            _ => unreachable!("only settings messages are routed here"),
        }
        Task::none()
    }
    pub(super) fn settings_view<'a>(&'a self, base: Element<'a, Message>) -> Element<'a, Message> {
        let dialog = self.settings.as_ref().expect("open settings");
        let card = responsive(move |size| {
            let engine = dialog.tab == SettingsTab::Engine;
            let tabs = row![
                button("Engine")
                    .on_press(Message::SettingsTab(SettingsTab::Engine))
                    .style(move |t, s| if engine {
                        button::primary(t, s)
                    } else {
                        button::text(t, s)
                    }),
                button("Appearance")
                    .on_press(Message::SettingsTab(SettingsTab::Appearance))
                    .style(move |t, s| if !engine {
                        button::primary(t, s)
                    } else {
                        button::text(t, s)
                    }),
            ]
            .spacing(8);
            let body: Element<'_, Message> = if engine {
                column![
                    text("Stockfish executable").size(18),
                    text("Leave empty to detect Stockfish automatically. A custom path is saved on this computer.").size(13).color(theme::MUTED),
                    text_input("Automatic detection", &dialog.path).id("settings-engine-path").on_input(Message::SettingsPath).on_submit(Message::ApplySettings).size(14).padding(10),
                    text(format!("Current engine: {}", if self.engine_path.is_empty() { "Not found" } else { &self.engine_path })).size(12).color(theme::MUTED),
                    text(&dialog.error).size(13).color(iced::Color::from_rgb8(246,126,126)),
                    row![button("Cancel").on_press(Message::CloseSettings).style(button::secondary), button("Apply").on_press(Message::ApplySettings)].spacing(8),
                ].spacing(16).into()
            } else {
                self.appearance_controls()
            };
            let card = container(
                column![
                    row![
                        text("Settings").size(26),
                        iced::widget::Space::new().width(Length::Fill),
                        button("Close")
                            .on_press(Message::CloseSettings)
                            .style(button::text)
                    ]
                    .align_y(Alignment::Center),
                    tabs,
                    scrollable(container(body).padding([4, 0]).width(Length::Fill))
                        .height(Length::Fill),
                ]
                .spacing(16),
            )
            .padding(24)
            .width((size.width - 40.0).clamp(280.0, 600.0))
            .height(
                (size.height - 40.0)
                    .min(if engine { 440.0 } else { 740.0 })
                    .max(200.0),
            )
            .style(container::rounded_box);
            container(opaque(card))
                .center_x(Length::Fill)
                .center_y(Length::Fill)
                .into()
        });
        stack![
            base,
            opaque(
                mouse_area(
                    container(card)
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .style(|_| container::Style {
                            background: Some(
                                iced::Color {
                                    a: 0.65,
                                    ..iced::Color::BLACK
                                }
                                .into()
                            ),
                            ..Default::default()
                        })
                )
                .on_press(Message::CloseSettings)
            )
        ]
        .into()
    }
}
fn engine_settings(
    current: &appearance::Settings,
    path: &str,
) -> Result<appearance::Settings, String> {
    let mut candidate = current.clone();
    candidate.engine_path = (!path.trim().is_empty()).then(|| path.trim().to_owned());
    candidate.validate()?;
    if let Some(path) = &candidate.engine_path
        && !std::path::Path::new(path).is_file()
    {
        return Err("Choose an existing Stockfish executable file, or leave the path empty for automatic detection.".into());
    }
    Ok(candidate)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn engine_settings_validate_without_mutating_current_settings() {
        let current = appearance::Settings::default();
        assert!(engine_settings(&current, "/this-engine-does-not-exist/stockfish").is_err());
        assert!(current.engine_path.is_none());
        let file = tempfile::NamedTempFile::new().unwrap();
        let candidate = engine_settings(&current, file.path().to_str().unwrap()).unwrap();
        assert_eq!(candidate.engine_path.as_deref(), file.path().to_str());
        assert!(
            engine_settings(&candidate, "  ")
                .unwrap()
                .engine_path
                .is_none()
        );
        assert!(engine_settings(&current, "bad\0path").is_err());
    }
    #[test]
    fn settings_drafts_and_keyboard_shortcuts_leave_board_and_analysis_unchanged() {
        let mut app = App::default();
        app.game.play_uci("e2e4").unwrap();
        let fen = app.game.fen();
        app.active = Some((42, app.game.revision()));
        app.info.score = Some(engine::Score::Cp(75));
        let _ = app.update(Message::OpenSettings);
        let _ = app.update(Message::SettingsPath("/missing-engine/stockfish".into()));
        let _ = app.update(Message::New);
        let _ = app.update(Message::ApplySettings);
        assert!(app.settings.as_ref().is_some_and(|d| !d.error.is_empty()));
        assert_eq!(app.game.fen(), fen);
        assert_eq!(app.active, Some((42, app.game.revision())));
        assert_eq!(app.info.score, Some(engine::Score::Cp(75)));
        let original_path = app.engine_path.clone();
        let _ = app.update(Message::CloseSettings);
        assert!(app.settings.is_none());
        assert_eq!(app.engine_path, original_path);
    }
}
