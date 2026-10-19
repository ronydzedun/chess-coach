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
    pub threads: String,
    pub hash_mib: String,
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
                        threads: self.appearance.engine_threads.to_string(),
                        hash_mib: self.appearance.engine_hash_mib.to_string(),
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
            Message::SettingsThreads(threads) => {
                if let Some(dialog) = &mut self.settings {
                    dialog.threads = threads;
                    dialog.error.clear();
                }
            }
            Message::SettingsHash(hash) => {
                if let Some(dialog) = &mut self.settings {
                    dialog.hash_mib = hash;
                    dialog.error.clear();
                }
            }
            Message::ApplySettings => {
                let Some(dialog) = &mut self.settings else {
                    return Task::none();
                };
                let candidate = match engine_settings(
                    &self.appearance,
                    &dialog.path,
                    &dialog.threads,
                    &dialog.hash_mib,
                ) {
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
                let resources_changed =
                    self.appearance.engine_options() != candidate.engine_options();
                self.appearance = candidate;
                self.settings = None;
                if self.engine_path != effective_path || resources_changed {
                    self.engine.reset();
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
                    row![
                        column![text("CPU threads").size(13), text_input("1–1024", &dialog.threads).id("settings-threads").on_input(Message::SettingsThreads).on_submit(Message::ApplySettings).padding(10)].spacing(6),
                        column![text("Hash (MiB)").size(13), text_input("1–4096", &dialog.hash_mib).id("settings-hash").on_input(Message::SettingsHash).on_submit(Message::ApplySettings).padding(10)].spacing(6),
                    ].spacing(12),
                    text(format!("{} logical CPU threads available. Hash is the cache size; total engine memory is higher.", std::thread::available_parallelism().map(usize::from).unwrap_or(1))).size(12).color(theme::MUTED),
                    text("The engine stays ready between moves and reuses its cache. Apply restarts it when these settings change.").size(12).color(theme::MUTED),
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
                    .min(if engine { 620.0 } else { 740.0 })
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
    threads: &str,
    hash_mib: &str,
) -> Result<appearance::Settings, String> {
    let mut candidate = current.clone();
    candidate.engine_path = (!path.trim().is_empty()).then(|| path.trim().to_owned());
    candidate.engine_threads = threads
        .trim()
        .parse()
        .map_err(|_| "Threads must be a whole number from 1 to 1024.".to_string())?;
    candidate.engine_hash_mib = hash_mib
        .trim()
        .parse()
        .map_err(|_| "Hash must be a whole number from 1 to 4096 MiB.".to_string())?;
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
        assert!(
            engine_settings(
                &current,
                "/this-engine-does-not-exist/stockfish",
                "2",
                "128"
            )
            .is_err()
        );
        assert!(current.engine_path.is_none());
        let file = tempfile::NamedTempFile::new().unwrap();
        let candidate =
            engine_settings(&current, file.path().to_str().unwrap(), "2", "128").unwrap();
        assert_eq!(candidate.engine_path.as_deref(), file.path().to_str());
        assert!(
            engine_settings(&candidate, "  ", "2", "128")
                .unwrap()
                .engine_path
                .is_none()
        );
        assert!(engine_settings(&current, "bad\0path", "2", "128").is_err());
    }
    #[test]
    fn resource_drafts_validate_and_cancel_preserves_active_settings() {
        let current = appearance::Settings::default();
        let valid = engine_settings(&current, "", "4", "512").unwrap();
        assert_eq!(valid.engine_threads, 4);
        assert_eq!(valid.engine_hash_mib, 512);
        for (threads, hash) in [
            ("0", "256"),
            ("1025", "256"),
            ("4", "0"),
            ("4", "4097"),
            ("abc", "256"),
            ("4", "1.5"),
        ] {
            assert!(engine_settings(&current, "", threads, hash).is_err());
        }
        let mut app = App::default();
        let options = app.appearance.engine_options();
        let _ = app.update(Message::OpenSettings);
        let _ = app.update(Message::SettingsThreads("0".into()));
        let _ = app.update(Message::ApplySettings);
        assert!(app.settings.as_ref().is_some_and(|d| !d.error.is_empty()));
        assert_eq!(app.appearance.engine_options(), options);
        let _ = app.update(Message::SettingsThreads("4".into()));
        let _ = app.update(Message::SettingsHash("512".into()));
        let _ = app.update(Message::CloseSettings);
        assert_eq!(app.appearance.engine_options(), options);
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
