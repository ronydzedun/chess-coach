// SPDX-License-Identifier: GPL-3.0-or-later
use crate::{
    App, Message,
    appearance::{BoardPalette, PieceSet},
    theme,
};
use iced::widget::{button, column, container, pick_list, row, text, text_input};
use iced::{Element, Length, Task};
impl App {
    fn persist_appearance(&mut self) {
        self.appearance_status = match self.appearance.save() {
            Ok(()) => "Appearance saved on this computer".into(),
            Err(error) => format!("Applied for this session; could not save: {error}"),
        };
    }
    pub(super) fn update_appearance(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::PieceSet(set) => {
                self.appearance.piece_set = set;
                self.persist_appearance();
            }
            Message::Palette(name) => {
                if self.appearance.palettes().iter().any(|p| p.name == name) {
                    self.appearance.selected_palette = name;
                    let palette = self.appearance.active_palette();
                    self.palette_light = palette.light;
                    self.palette_dark = palette.dark;
                    self.persist_appearance();
                }
            }
            Message::PaletteName(name) => self.palette_name = name,
            Message::PaletteLight(color) => self.palette_light = color,
            Message::PaletteDark(color) => self.palette_dark = color,
            Message::SavePalette => {
                let result =
                    BoardPalette::new(&self.palette_name, &self.palette_light, &self.palette_dark)
                        .and_then(|palette| self.appearance.add_palette(palette));
                match result {
                    Ok(()) => {
                        self.palette_name.clear();
                        self.persist_appearance();
                    }
                    Err(error) => self.appearance_status = error,
                }
            }
            Message::DeletePalette => {
                let name = self.appearance.selected_palette.clone();
                self.appearance.remove_palette(&name);
                self.persist_appearance();
            }
            _ => unreachable!("only appearance messages are routed here"),
        }
        Task::none()
    }
    pub(super) fn appearance_controls(&self) -> Element<'_, Message> {
        let palettes: Vec<String> = self
            .appearance
            .palettes()
            .into_iter()
            .map(|p| p.name)
            .collect();
        let custom = self
            .appearance
            .custom_palettes
            .iter()
            .any(|p| p.name == self.appearance.selected_palette);
        let swatches = match BoardPalette::new("Preview", &self.palette_light, &self.palette_dark) {
            Ok(palette) => {
                let (light, dark) = palette.colors();
                row![
                    container(text("Light").color(theme::INK))
                        .padding(12)
                        .width(Length::Fill)
                        .style(move |_| container::Style {
                            background: Some(light.into()),
                            ..Default::default()
                        }),
                    container(text("Dark").color(iced::Color::WHITE))
                        .padding(12)
                        .width(Length::Fill)
                        .style(move |_| container::Style {
                            background: Some(dark.into()),
                            ..Default::default()
                        })
                ]
                .spacing(4)
                .into()
            }
            Err(_) => Element::from(
                text("Enter two different #RRGGBB colors")
                    .size(12)
                    .color(theme::MUTED),
            ),
        };
        column![
            text("MAKE IT YOUR BOARD").size(12).color(theme::ACCENT),
            text("Appearance").size(26),
            text("Pieces").size(13),
            pick_list(PieceSet::ALL, Some(self.appearance.piece_set), Message::PieceSet).width(Length::Fill),
            text("Board palette").size(13),
            pick_list(palettes, Some(self.appearance.selected_palette.clone()), Message::Palette).width(Length::Fill),
            button("Delete custom palette").on_press_maybe(custom.then_some(Message::DeletePalette)).style(button::secondary),
            text("Create a palette").size(18),
            text_input("Palette name", &self.palette_name).on_input(Message::PaletteName).size(13),
            row![text("Light").width(44), text_input("#F0D9B5", &self.palette_light).on_input(Message::PaletteLight).size(13)].spacing(6),
            row![text("Dark").width(44), text_input("#B58863", &self.palette_dark).on_input(Message::PaletteDark).on_submit(Message::SavePalette).size(13)].spacing(6),
            swatches,
            button("Save new palette").on_press(Message::SavePalette),
            text(&self.appearance_status).size(12).color(theme::MUTED),
            text("Artwork: Merida · Celtic · RhosGFX. Licenses and artists are listed in THIRD_PARTY_NOTICES.md.").size(11).color(theme::MUTED),
        ].spacing(12).into()
    }
}
