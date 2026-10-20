// SPDX-License-Identifier: GPL-3.0-or-later
fn main() -> iced::Result {
    iced::application(desktop::App::new, desktop::App::update, desktop::App::view)
        .title("Chess Coach")
        .theme(desktop::theme::theme)
        .subscription(desktop::App::subscription)
        .font(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").as_slice())
        .default_font(iced::Font::with_name("DejaVu Sans"))
        .window_size((1120.0, 820.0))
        .window(iced::window::Settings {
            min_size: Some(iced::Size::new(520.0, 600.0)),
            ..Default::default()
        })
        .centered()
        .run()
}
