// SPDX-License-Identifier: GPL-3.0-or-later
use chess_core::Color;
use engine::Score;
use iced::widget::{column, container, text};
use iced::{Element, Length};

// A bounded visual scale, not a win probability. Scores are White-relative.
fn white_share(score: Option<Score>, turn: Color) -> f32 {
    match score {
        Some(Score::Cp(cp)) => 0.5 + 0.5 * (cp as f32 / 400.0).tanh(),
        Some(Score::Mate(0)) => {
            if turn == Color::White {
                0.0
            } else {
                1.0
            }
        }
        Some(Score::Mate(n)) => {
            if n > 0 {
                1.0
            } else {
                0.0
            }
        }
        None => 0.5,
    }
}
pub fn label(score: Option<Score>, turn: Color) -> String {
    match score {
        Some(Score::Cp(cp)) => format!("{:+.1}", f64::from(cp) / 100.0),
        Some(Score::Mate(0)) => format!("{}M0", if turn == Color::White { "−" } else { "+" }),
        Some(Score::Mate(n)) => format!("M{n:+}"),
        None => "—".into(),
    }
}
pub fn view<'a, Message: 'a>(
    score: Option<Score>,
    turn: Color,
    flipped: bool,
    height: f32,
) -> Element<'a, Message> {
    let white = white_share(score, turn);
    let top_share = if flipped { white } else { 1.0 - white };
    let light = iced::Color::from_rgb8(239, 237, 231);
    let dark = iced::Color::from_rgb8(41, 45, 50);
    let top_color = if flipped { light } else { dark };
    let bottom_color = if flipped { dark } else { light };
    let top = container(iced::widget::Space::new())
        .width(24)
        .height(height * top_share)
        .style(move |_| container::Style {
            background: Some(top_color.into()),
            ..Default::default()
        });
    let bottom = container(iced::widget::Space::new())
        .width(24)
        .height(height * (1.0 - top_share))
        .style(move |_| container::Style {
            background: Some(bottom_color.into()),
            ..Default::default()
        });
    column![
        column![top, bottom],
        container(text(label(score, turn)).size(11)).center_x(Length::Fill)
    ]
    .spacing(4)
    .width(40)
    .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scale_is_white_relative_and_bounded() {
        assert_eq!(white_share(None, Color::White), 0.5);
        assert_eq!(white_share(Some(Score::Cp(0)), Color::Black), 0.5);
        let winning = white_share(Some(Score::Cp(300)), Color::Black);
        assert!(winning > 0.5 && winning < 1.0);
        assert!(white_share(Some(Score::Cp(900)), Color::White) > winning);
        assert!(white_share(Some(Score::Cp(-300)), Color::White) < 0.5);
        assert_eq!(white_share(Some(Score::Mate(3)), Color::Black), 1.0);
        assert_eq!(white_share(Some(Score::Mate(-3)), Color::White), 0.0);
        assert_eq!(white_share(Some(Score::Mate(0)), Color::White), 0.0);
        assert_eq!(white_share(Some(Score::Mate(0)), Color::Black), 1.0);
    }
}
