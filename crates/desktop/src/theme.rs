// SPDX-License-Identifier: GPL-3.0-or-later
use iced::{Color, Theme};
pub const LIGHT: Color = Color::from_rgb8(229, 224, 207);
pub const DARK: Color = Color::from_rgb8(110, 143, 129);
pub const SELECTED: Color = Color::from_rgb8(239, 191, 94);
pub const LAST: Color = Color::from_rgb8(180, 193, 121);
pub const INK: Color = Color::from_rgb8(25, 42, 46);
pub const MUTED: Color = Color::from_rgb8(155, 174, 184);
pub const ACCENT: Color = Color::from_rgb8(117, 216, 187);
pub fn theme(_: &crate::App) -> Theme {
    Theme::custom(
        "Chess Coach",
        iced::theme::Palette {
            background: Color::from_rgb8(18, 28, 36),
            text: Color::from_rgb8(233, 239, 239),
            primary: ACCENT,
            success: ACCENT,
            warning: SELECTED,
            danger: Color::from_rgb8(246, 126, 126),
        },
    )
}
