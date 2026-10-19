// SPDX-License-Identifier: GPL-3.0-or-later
//! Responsive, clickable evaluation history. Values are pawns from White's perspective.
use crate::{Message, theme};
use iced::advanced::{Clipboard, Layout, Shell, Widget, layout, mouse, renderer, svg, text};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme};
use std::fmt::Write as _;
use svg::Renderer as _;
use text::Renderer as _;

const HEIGHT: f32 = 220.0;
const LIMIT: f32 = 10.0;

pub(super) fn chart(scores: Vec<Option<f32>>, selected: usize) -> Element<'static, Message> {
    Element::new(Chart { scores, selected })
}

struct Chart {
    scores: Vec<Option<f32>>,
    selected: usize,
}

fn plot(bounds: Rectangle) -> Rectangle {
    Rectangle {
        x: bounds.x + 32.0,
        y: bounds.y + 12.0,
        width: (bounds.width - 44.0).max(0.0),
        height: (bounds.height - 40.0).max(0.0),
    }
}

fn x_at(ply: usize, count: usize, bounds: Rectangle) -> f32 {
    bounds.x + bounds.width * ply as f32 / count.saturating_sub(1).max(1) as f32
}

fn y_at(value: f32, bounds: Rectangle) -> f32 {
    bounds.y + bounds.height * (LIMIT - value.clamp(-LIMIT, LIMIT)) / (2.0 * LIMIT)
}

fn seek_at(point: Point, bounds: Rectangle, count: usize) -> Option<usize> {
    let area = plot(bounds);
    if count == 0
        || area.width <= 0.0
        || area.height <= 0.0
        || point.x < area.x
        || point.x > area.x + area.width
        || point.y < area.y
        || point.y > area.y + area.height
    {
        return None;
    }
    Some((((point.x - area.x) / area.width) * count.saturating_sub(1) as f32).round() as usize)
}

fn press_position(event: &Event, cursor: mouse::Cursor) -> Option<Point> {
    match event {
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
        | Event::Touch(iced::touch::Event::FingerPressed { .. }) => cursor.position(),
        _ => None,
    }
}

// A new subpath after each unknown value prevents a false connection across missing analysis.
fn score_path(scores: &[Option<f32>], bounds: Rectangle) -> String {
    let mut path = String::new();
    let mut connected = false;
    for (ply, score) in scores.iter().enumerate() {
        if let Some(value) = score.filter(|value| value.is_finite()) {
            let command = if connected { 'L' } else { 'M' };
            let _ = write!(
                path,
                "{command}{:.2},{:.2} ",
                x_at(ply, scores.len(), bounds),
                y_at(value, bounds)
            );
            connected = true;
        } else {
            connected = false;
        }
    }
    path
}

fn label(renderer: &mut iced::Renderer, value: String, position: Point, viewport: Rectangle) {
    renderer.fill_text(
        text::Text {
            content: value,
            bounds: Size::new(42.0, 18.0),
            size: iced::Pixels(11.0),
            line_height: text::LineHeight::Relative(1.0),
            font: iced::Font::DEFAULT,
            align_x: text::Alignment::Left,
            align_y: iced::alignment::Vertical::Top,
            shaping: text::Shaping::Basic,
            wrapping: text::Wrapping::None,
        },
        position,
        theme::MUTED,
        viewport,
    );
}

impl Widget<Message, Theme, iced::Renderer> for Chart {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fixed(HEIGHT))
    }

    fn layout(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(Length::Fill, HEIGHT, Size::new(400.0, HEIGHT)))
    }

    fn update(
        &mut self,
        _tree: &mut iced::advanced::widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        // The enclosing scrollable translates the cursor, but not touch event positions.
        let position = press_position(event, cursor);
        if let Some(ply) =
            position.and_then(|point| seek_at(point, layout.bounds(), self.scores.len()))
        {
            shell.publish(Message::ReportSeek(ply));
            shell.capture_event();
        }
    }

    fn mouse_interaction(
        &self,
        _tree: &iced::advanced::widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if cursor
            .position()
            .and_then(|point| seek_at(point, layout.bounds(), self.scores.len()))
            .is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }

    fn draw(
        &self,
        _tree: &iced::advanced::widget::Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let local = Rectangle::with_size(bounds.size());
        let area = plot(local);
        if area.width <= 0.0 || area.height <= 0.0 {
            return;
        }
        let mut image = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">"##,
            bounds.width, bounds.height, bounds.width, bounds.height
        );
        for value in [-5.0, 0.0, 5.0] {
            let y = y_at(value, area);
            let opacity = if value == 0.0 { "0.55" } else { "0.20" };
            let _ = write!(
                image,
                r##"<path d="M{} {y}H{}" stroke="#9baeb8" stroke-opacity="{opacity}"/>"##,
                area.x,
                area.x + area.width
            );
            label(
                renderer,
                format!("{value:+.0}"),
                Point::new(bounds.x + 2.0, bounds.y + y - 6.0),
                *viewport,
            );
        }
        let path = score_path(&self.scores, area);
        let _ = write!(
            image,
            r##"<path d="{path}" fill="none" stroke="#75d8bb" stroke-width="2" stroke-linejoin="round"/>"##
        );
        // Dots also expose isolated known values between two gaps.
        for (ply, score) in self.scores.iter().enumerate() {
            if let Some(value) = score.filter(|value| value.is_finite()) {
                let _ = write!(
                    image,
                    r##"<circle cx="{}" cy="{}" r="1.6" fill="#75d8bb"/>"##,
                    x_at(ply, self.scores.len(), area),
                    y_at(value, area)
                );
            }
        }
        if self.selected < self.scores.len() {
            let selected = self.selected;
            let x = x_at(selected, self.scores.len(), area);
            let _ = write!(
                image,
                r##"<path d="M{x} {}V{}" stroke="#efbf5e" stroke-width="1.5"/>"##,
                area.y,
                area.y + area.height
            );
        }
        image.push_str("</svg>");
        renderer.draw_svg(
            svg::Svg {
                handle: svg::Handle::from_memory(image.into_bytes()),
                color: None,
                rotation: iced::Radians(0.0),
                opacity: 1.0,
            },
            bounds,
            *viewport,
        );
        // Choose a full-move interval that keeps labels readable at narrow widths.
        let last = self.scores.len().saturating_sub(1);
        let intervals = (area.width / 70.0).floor().max(1.0) as usize;
        let full_moves = last.div_ceil(2);
        let step = full_moves.div_ceil(intervals).max(1);
        for full_move in (0..=last / 2).step_by(step) {
            let x = x_at(full_move * 2, self.scores.len(), area);
            label(
                renderer,
                full_move.to_string(),
                Point::new(bounds.x + x - 3.0, bounds.y + area.y + area.height + 8.0),
                *viewport,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_hit_testing_uses_scroll_translated_cursor() {
        let bounds = Rectangle::new(Point::new(50.0, 600.0), Size::new(300.0, HEIGHT));
        let area = plot(bounds);
        let content_point = Point::new(x_at(5, 11, area), area.center_y());
        let event = Event::Touch(iced::touch::Event::FingerPressed {
            id: iced::touch::Finger(1),
            position: Point::new(content_point.x, content_point.y - 200.0),
        });
        let point = press_position(&event, mouse::Cursor::Available(content_point)).unwrap();
        assert_eq!(seek_at(point, bounds, 11), Some(5));
        assert!(press_position(&event, mouse::Cursor::Unavailable).is_none());
    }

    #[test]
    fn white_advantage_is_up_and_extreme_values_saturate() {
        let area = Rectangle::with_size(Size::new(400.0, 160.0));
        assert_eq!(y_at(0.0, area), 80.0);
        assert_eq!(y_at(5.0, area), 40.0);
        assert_eq!(y_at(100.0, area), 0.0);
        assert_eq!(y_at(-100.0, area), 160.0);
    }

    #[test]
    fn unknown_and_nonfinite_evaluations_break_the_line() {
        let area = Rectangle::with_size(Size::new(400.0, 160.0));
        let path = score_path(
            &[
                Some(1.0),
                None,
                Some(2.0),
                Some(3.0),
                Some(f32::NAN),
                Some(-1.0),
            ],
            area,
        );
        assert_eq!(path.matches('M').count(), 3);
        assert_eq!(path.matches('L').count(), 1);
        assert!(!path.contains("NaN"));
        assert!(score_path(&[None, None], area).is_empty());
    }

    #[test]
    fn click_mapping_tracks_responsive_plot_and_half_moves() {
        for width in [200.0, 700.0] {
            let bounds = Rectangle::new(Point::new(50.0, 30.0), Size::new(width, HEIGHT));
            let area = plot(bounds);
            for ply in 0..=40 {
                let point = Point::new(x_at(ply, 41, area), area.y + 20.0);
                assert_eq!(seek_at(point, bounds, 41), Some(ply));
            }
            assert_eq!(seek_at(Point::new(bounds.x, area.y), bounds, 41), None);
            assert_eq!(seek_at(Point::new(area.x, bounds.y), bounds, 41), None);
            assert_eq!(seek_at(Point::new(area.x, area.y), bounds, 0), None);
            assert_eq!(seek_at(Point::new(area.x, area.y), bounds, 1), Some(0));
        }
    }
}
