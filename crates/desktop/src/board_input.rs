// SPDX-License-Identifier: GPL-3.0-or-later
//! Pointer capture for the board. Chess state changes only after a validated drop.
use crate::{App, Message, theme};
use chess_core::{Square, square_at};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer, svg,
    widget::{Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, touch};
use renderer::Renderer as _;
use svg::Renderer as _;

pub(super) struct BoardInput<'a> {
    pub content: Element<'a, Message>,
    pub app: &'a App,
    pub cell: f32,
    pub selected_destinations: Vec<Square>,
}
#[derive(Clone, Copy, Debug)]
struct Press {
    square: Square,
    origin: Point,
    piece_bounds: Rectangle,
    draggable: bool,
    dragging: bool,
}
#[derive(Default)]
struct State {
    press: Option<Press>,
    identity: Option<(u64, bool, u64, bool)>,
    finger: Option<touch::Finger>,
    position: Option<Point>,
    preview_destinations: Vec<Square>,
}
fn square_under(bounds: Rectangle, cell: f32, point: Point, flipped: bool) -> Option<Square> {
    let x = point.x - bounds.x - 24.0;
    let y = point.y - bounds.y;
    if x < 0.0 || y < 0.0 || x >= cell * 8.0 || y >= cell * 8.0 {
        return None;
    }
    square_at((y / cell) as usize, (x / cell) as usize, flipped)
}
// Preserve the point at which the piece was grabbed; only translate its original bounds.
fn moving_bounds(press: Press, position: Point) -> Rectangle {
    Rectangle {
        x: press.piece_bounds.x + position.x - press.origin.x,
        y: press.piece_bounds.y + position.y - press.origin.y,
        ..press.piece_bounds
    }
}
impl BoardInput<'_> {
    fn enabled(&self) -> bool {
        self.app.settings.is_none() && !self.app.review.import_open && self.app.promotion.is_none()
    }
    fn identity(&self) -> (u64, bool, u64, bool) {
        (
            self.app.game.revision(),
            self.app.flipped,
            self.app.pointer_epoch,
            self.enabled(),
        )
    }
}
impl Widget<Message, Theme, iced::Renderer> for BoardInput<'_> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State {
            identity: Some(self.identity()),
            ..State::default()
        })
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
        let state = tree.state.downcast_mut::<State>();
        if state.identity != Some(self.identity()) {
            state.press = None;
            state.finger = None;
            state.identity = Some(self.identity());
        }
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn update(
        &mut self,
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        if !self.enabled() {
            state.press = None;
            state.finger = None;
            state.position = None;
            return;
        }
        // A completed touch must not seed the next mouse press with touch coordinates.
        let touch_ended = matches!(event, Event::Touch(touch::Event::FingerLifted { id, .. }) if state.finger == Some(*id));
        // Use one pointer path for mouse and native touch, tracking a single finger.
        let mapped;
        let (event, position) = match event {
            Event::Touch(touch::Event::FingerPressed { id, position }) if state.press.is_none() => {
                state.finger = Some(*id);
                mapped = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
                (&mapped, Some(*position))
            }
            Event::Touch(touch::Event::FingerMoved { id, position })
                if state.finger == Some(*id) =>
            {
                mapped = Event::Mouse(mouse::Event::CursorMoved {
                    position: *position,
                });
                (&mapped, Some(*position))
            }
            Event::Touch(touch::Event::FingerLifted { id, position })
                if state.finger == Some(*id) =>
            {
                state.finger = None;
                mapped = Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left));
                (&mapped, Some(*position))
            }
            Event::Touch(touch::Event::FingerLost { id, .. }) if state.finger == Some(*id) => {
                state.finger = None;
                mapped = Event::Window(iced::window::Event::Unfocused);
                (&mapped, None)
            }
            Event::Touch(_) => return,
            Event::Mouse(_) if state.finger.is_some() => return,
            Event::Mouse(mouse::Event::CursorMoved { position }) => (event, Some(*position)),
            // Iced supplies one cursor snapshot for a batch. Do not overwrite the
            // ordered event positions with that snapshot on press or redraw.
            _ => (event, state.position.or_else(|| cursor.position())),
        };
        state.position = position;
        let square =
            position.and_then(|p| square_under(layout.bounds(), self.cell, p, self.app.flipped));
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let (Some(square), Some(origin)) = (square, position) {
                    let draggable = self.app.game.piece_at(square).is_some();
                    state.preview_destinations = if self
                        .app
                        .game
                        .piece_at(square)
                        .is_some_and(|p| p.color == self.app.game.turn())
                    {
                        self.app.game.destinations(square)
                    } else {
                        Vec::new()
                    };
                    let bounds = layout.bounds();
                    state.press = Some(Press {
                        square,
                        origin,
                        dragging: false,
                        draggable,
                        piece_bounds: Rectangle::new(
                            Point::new(
                                bounds.x
                                    + 24.0
                                    + ((origin.x - bounds.x - 24.0) / self.cell).floor()
                                        * self.cell
                                    + self.cell * 0.05,
                                bounds.y
                                    + ((origin.y - bounds.y) / self.cell).floor() * self.cell
                                    + self.cell * 0.05,
                            ),
                            Size::new(self.cell * 0.9, self.cell * 0.9),
                        ),
                    });
                    // Selection, hints and the moving piece are painted from this
                    // widget state. A press never rebuilds the application.
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(press) = &mut state.press {
                    if press.draggable && !press.dragging && press.origin.distance(*position) >= 5.0
                    {
                        press.dragging = true;
                    }
                    if press.draggable {
                        shell.request_redraw();
                        shell.capture_event();
                    }
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if let Some(press) = state.press.take() {
                    let can_move = self
                        .app
                        .game
                        .piece_at(press.square)
                        .is_some_and(|p| p.color == self.app.game.turn());
                    if press.draggable
                        && !can_move
                        && (press.dragging || square != Some(press.square))
                    {
                        // A visual drag of the other side never submits a move.
                    } else if press.dragging {
                        shell.publish(Message::Dropped(
                            press.square,
                            square,
                            (self.app.game.revision(), self.app.pointer_epoch),
                        ));
                    } else if square == Some(press.square) {
                        shell.publish(Message::BoardClick(
                            press.square,
                            (self.app.game.revision(), self.app.pointer_epoch),
                        ));
                    } else if press.draggable {
                        shell.publish(Message::Dropped(
                            press.square,
                            None,
                            (self.app.game.revision(), self.app.pointer_epoch),
                        ));
                    }
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Window(iced::window::Event::Unfocused | iced::window::Event::Resized(_))
            | Event::Mouse(mouse::Event::CursorLeft) => {
                state.finger = None;
                state.position = None;
                if state.press.take().is_some() {
                    shell.request_redraw();
                }
            }
            _ => {}
        }
        if touch_ended {
            state.position = None;
        }
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        if tree
            .state
            .downcast_ref::<State>()
            .press
            .is_some_and(|p| p.draggable)
        {
            mouse::Interaction::Grabbing
        } else if self.enabled()
            && cursor
                .position()
                .and_then(|p| square_under(layout.bounds(), self.cell, p, self.app.flipped))
                .is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
        let state = tree.state.downcast_ref::<State>();
        let moving = state.press.filter(|p| p.draggable);
        let selected = moving.map(|p| p.square).or(self.app.selected);
        let legal = if moving.is_some() {
            &state.preview_destinations
        } else {
            &self.selected_destinations
        };
        let (light, dark) = self.app.appearance.active_palette().colors();
        let in_check = self.app.game.in_check();
        let bounds = layout.bounds();
        for r in 0..8 {
            for c in 0..8 {
                let square = square_at(r, c, self.app.flipped).expect("board coordinates");
                let cell = Rectangle::new(
                    Point::new(
                        bounds.x + 24.0 + c as f32 * self.cell,
                        bounds.y + r as f32 * self.cell,
                    ),
                    Size::new(self.cell, self.cell),
                );
                let mut background = if (r + c) % 2 == 0 { light } else { dark };
                if self
                    .app
                    .game
                    .last_move()
                    .is_some_and(|m| m.from == square || m.to == square)
                {
                    background = theme::LAST;
                }
                if selected == Some(square) {
                    background = theme::SELECTED;
                }
                let piece = self.app.game.piece_at(square);
                let is_legal = legal.contains(&square);
                if is_legal && piece.is_some() {
                    background = theme::tint(background, theme::INK, 0.20);
                }
                if in_check
                    && piece.is_some_and(|p| {
                        p.color == self.app.game.turn() && p.role == chess_core::Role::King
                    })
                {
                    background = theme::tint(background, iced::Color::from_rgb8(220, 55, 55), 0.65);
                }
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: cell,
                        ..Default::default()
                    },
                    background,
                );
                // Keep a fixed marker slot per square. Inserting/removing dots
                // shifts the software renderer's primitive list and invalidates
                // unrelated squares and UI, making first selection expensive.
                {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: Rectangle::new(
                                Point::new(cell.x + self.cell * 0.4, cell.y + self.cell * 0.4),
                                Size::new(self.cell * 0.2, self.cell * 0.2),
                            ),
                            border: iced::Border {
                                radius: (self.cell * 0.1).into(),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        iced::Color {
                            a: if is_legal && piece.is_none() {
                                0.24
                            } else {
                                0.0
                            },
                            ..theme::INK
                        },
                    );
                }
                if let Some(piece) = piece {
                    renderer.draw_svg(
                        svg::Svg {
                            handle: svg::Handle::from_memory(
                                self.app
                                    .appearance
                                    .piece_set
                                    .svg_bytes(piece.color, piece.role),
                            ),
                            color: None,
                            rotation: iced::Radians(0.0),
                            // Retain the source slot; the preview draws above it.
                            opacity: if moving.is_some_and(|p| p.square == square) {
                                0.0
                            } else {
                                1.0
                            },
                        },
                        Rectangle::new(
                            Point::new(cell.x + self.cell * 0.05, cell.y + self.cell * 0.05),
                            Size::new(self.cell * 0.9, self.cell * 0.9),
                        ),
                        *viewport,
                    );
                }
            }
        }
        let Some(press) = tree
            .state
            .downcast_ref::<State>()
            .press
            .filter(|p| p.draggable)
        else {
            return;
        };
        let Some(position) = tree.state.downcast_ref::<State>().position else {
            return;
        };
        let Some(piece) = self.app.game.piece_at(press.square) else {
            return;
        };
        renderer.draw_svg(
            svg::Svg {
                handle: svg::Handle::from_memory(
                    self.app
                        .appearance
                        .piece_set
                        .svg_bytes(piece.color, piece.role),
                ),
                color: None,
                rotation: iced::Radians(0.0),
                opacity: 1.0,
            },
            moving_bounds(press, position),
            *viewport,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Harness<'a> {
        widget: BoardInput<'a>,
        tree: Tree,
        renderer: iced::Renderer,
        node: layout::Node,
        redraw_requested: bool,
    }
    impl<'a> Harness<'a> {
        fn new(app: &'a App) -> Self {
            let widget = BoardInput {
                content: iced::widget::Space::new().width(424).height(424).into(),
                app,
                cell: 50.0,
                selected_destinations: app
                    .selected
                    .map(|s| app.game.destinations(s))
                    .unwrap_or_default(),
            };
            let tree = Tree::new(&widget as &dyn Widget<Message, Theme, iced::Renderer>);
            Self {
                widget,
                tree,
                renderer: iced::Renderer::new(iced::Font::default(), iced::Pixels(16.0)),
                node: layout::Node::new(Size::new(424.0, 424.0)),
                redraw_requested: false,
            }
        }
        fn event(&mut self, event: Event, point: Point) -> Vec<Message> {
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            self.widget.update(
                &mut self.tree,
                &event,
                Layout::new(&self.node),
                mouse::Cursor::Available(point),
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut shell,
                &Rectangle::with_size(Size::new(1000.0, 1000.0)),
            );
            self.redraw_requested =
                shell.redraw_request() == iced::window::RedrawRequest::NextFrame;
            messages
        }
        fn press(&mut self, point: Point) {
            self.event(
                Event::Mouse(mouse::Event::CursorMoved { position: point }),
                point,
            );
            let messages = self.event(
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                point,
            );
            assert!(messages.is_empty());
        }
        fn release(&mut self, point: Point) -> Vec<Message> {
            self.event(
                Event::Mouse(mouse::Event::CursorMoved { position: point }),
                point,
            );
            self.event(
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                point,
            )
        }
    }
    #[test]
    fn either_side_can_be_dragged_but_opponent_has_no_hints_or_release_action() {
        for black_turn in [false, true] {
            let mut app = App::default();
            if black_turn {
                app.game.play_uci("e2e4").unwrap();
            }
            app.selected = Some(if black_turn { Square::E7 } else { Square::E2 });
            let initial = app.game.fen();
            let committed_selection = app.selected;
            let mut h = Harness::new(&app);
            let origin = if black_turn {
                Point::new(99.0, 375.0)
            } else {
                Point::new(249.0, 75.0)
            };
            let destination =
                Point::new(origin.x, origin.y + if black_turn { -100.0 } else { 100.0 });
            h.press(origin);
            assert!(
                h.event(
                    Event::Mouse(mouse::Event::CursorMoved {
                        position: destination
                    }),
                    destination
                )
                .is_empty()
            );
            let state = h.tree.state.downcast_ref::<State>();
            assert!(state.press.is_some_and(|p| p.draggable && p.dragging));
            assert!(state.preview_destinations.is_empty());
            assert!(h.release(destination).is_empty());
            assert_eq!(app.game.fen(), initial);
            assert_eq!(app.selected, committed_selection);
        }
    }
    #[test]
    fn first_selection_preserves_stationary_render_slots() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let viewport = Rectangle::with_size(Size::new(1000.0, 1000.0));
        let theme = crate::theme::theme(&app);
        let style = renderer::Style {
            text_color: crate::theme::INK,
        };
        h.widget.draw(
            &h.tree,
            &mut h.renderer,
            &theme,
            &style,
            Layout::new(&h.node),
            mouse::Cursor::Unavailable,
            &viewport,
        );
        let idle = h.renderer.layers().to_vec();
        h.renderer.reset(viewport);
        h.press(Point::new(249.0, 325.0));
        let point = Point::new(250.0, 325.0);
        h.event(
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            point,
        );
        h.widget.draw(
            &h.tree,
            &mut h.renderer,
            &theme,
            &style,
            Layout::new(&h.node),
            mouse::Cursor::Available(point),
            &viewport,
        );
        let pressed = h.renderer.layers();
        assert_eq!(idle.len(), pressed.len());
        let before = &idle[0];
        let after = &pressed[0];
        assert_eq!(before.quads.len(), after.quads.len());
        let changed: Vec<_> = before
            .quads
            .iter()
            .zip(&after.quads)
            .filter(|(a, b)| a != b)
            .collect();
        assert_eq!(
            changed.len(),
            3,
            "Only the source square and e3/e4 hints should change"
        );
        assert_eq!(
            before.images.len() + 1,
            after.images.len(),
            "Only the moving preview is appended"
        );
        assert_eq!(
            before
                .images
                .iter()
                .zip(&after.images)
                .filter(|(a, b)| a != b)
                .count(),
            1,
            "Stationary pieces must retain their render slots"
        );
    }
    #[test]
    fn first_unselected_press_and_motion_only_request_redraws() {
        for selected in [None, Some(Square::E2)] {
            let app = App {
                selected,
                ..App::default()
            };
            let initial = app.game.fen();
            let mut h = Harness::new(&app);
            let e2 = Point::new(249.0, 325.0);
            h.press(e2);
            assert!(h.redraw_requested);
            assert_eq!(app.selected, selected);
            assert_eq!(app.game.fen(), initial);
            assert!(
                h.tree
                    .state
                    .downcast_ref::<State>()
                    .preview_destinations
                    .contains(&Square::E4)
            );
            let position = Point::new(e2.x + 1.0, e2.y);
            assert!(
                h.event(
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                    position
                )
                .is_empty()
            );
            assert!(h.redraw_requested);
            assert_eq!(app.selected, selected);
        }
    }
    #[test]
    fn off_center_grab_moves_from_first_pixel_without_threshold_snap() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let origin = Point::new(265.0, 310.0); // Away from e2's center.
        h.press(origin);
        let press = h.tree.state.downcast_ref::<State>().press.unwrap();
        assert_eq!(moving_bounds(press, origin), press.piece_bounds);
        let initial = press.piece_bounds;
        for dx in [1.0, 2.0, 4.0, 5.0, 6.0, 10.0] {
            let position = Point::new(origin.x + dx, origin.y + 1.0);
            // Deliberately pass a different batch cursor snapshot.
            assert!(
                h.event(
                    Event::Mouse(mouse::Event::CursorMoved { position }),
                    Point::new(50.0, 50.0)
                )
                .is_empty()
            );
            let state = h.tree.state.downcast_ref::<State>();
            let press = state.press.unwrap();
            let bounds = moving_bounds(press, state.position.unwrap());
            assert_eq!(bounds.x, initial.x + dx);
            assert_eq!(bounds.y, initial.y + 1.0);
            assert_eq!(bounds.size(), initial.size());
        }
    }
    #[test]
    fn batched_cursor_snapshot_cannot_relocate_press_or_rewind_motion() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let e2 = Point::new(249.0, 325.0);
        let e4 = Point::new(249.0, 225.0);
        h.event(Event::Mouse(mouse::Event::CursorMoved { position: e2 }), e4);
        assert!(
            h.event(
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                e4
            )
            .is_empty()
        );
        assert_eq!(
            h.tree.state.downcast_ref::<State>().press.unwrap().origin,
            e2
        );
        h.event(Event::Mouse(mouse::Event::CursorMoved { position: e4 }), e2);
        h.event(
            Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            e2,
        );
        assert_eq!(h.tree.state.downcast_ref::<State>().position, Some(e4));
        assert!(matches!(
            h.event(
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                e2
            )
            .as_slice(),
            [Message::Dropped(Square::E2, Some(Square::E4), _)]
        ));
    }
    #[test]
    fn batched_clicks_use_ordered_application_selection() {
        for destinations in [[Square::E2, Square::E2], [Square::E2, Square::E4]] {
            let mut app = App::default();
            let messages = {
                let mut h = Harness::new(&app);
                let mut messages = Vec::new();
                for square in destinations {
                    let point = if square == Square::E2 {
                        Point::new(249.0, 325.0)
                    } else {
                        Point::new(249.0, 225.0)
                    };
                    h.event(
                        Event::Mouse(mouse::Event::CursorMoved { position: point }),
                        point,
                    );
                    messages.extend(h.event(
                        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                        point,
                    ));
                    messages.extend(h.release(point));
                }
                messages
            };
            for message in messages {
                let _ = app.update(message);
            }
            assert!(app.selected.is_none());
            assert_eq!(app.game.ply(), usize::from(destinations[1] == Square::E4));
        }
    }
    #[test]
    fn mouse_press_after_touch_uses_mouse_coordinates_without_motion() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let e2 = Point::new(249.0, 325.0);
        let e4 = Point::new(249.0, 225.0);
        let id = touch::Finger(1);
        h.event(
            Event::Touch(touch::Event::FingerPressed { id, position: e2 }),
            e4,
        );
        h.event(
            Event::Touch(touch::Event::FingerLifted { id, position: e2 }),
            e4,
        );
        assert!(
            h.event(
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                e4
            )
            .is_empty()
        );
        assert_eq!(
            h.tree.state.downcast_ref::<State>().press.unwrap().square,
            Square::E4
        );
        assert!(matches!(
            h.event(
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                e4
            )
            .as_slice(),
            [Message::BoardClick(Square::E4, _)]
        ));
    }
    #[test]
    fn click_threshold_and_drag_release_are_distinct() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let e2 = Point::new(249.0, 325.0);
        let e4 = Point::new(249.0, 225.0);
        h.press(e2);
        assert!(
            h.event(
                Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(251.0, 325.0)
                }),
                Point::new(251.0, 325.0)
            )
            .is_empty()
        );
        assert!(matches!(
            h.release(e2).as_slice(),
            [Message::BoardClick(Square::E2, _)]
        ));
        h.press(e2);
        assert!(
            h.event(Event::Mouse(mouse::Event::CursorMoved { position: e4 }), e4)
                .is_empty()
        );
        assert!(matches!(
            h.release(e4).as_slice(),
            [Message::Dropped(Square::E2, Some(Square::E4), _)]
        ));
        assert!(h.release(e4).is_empty());
    }
    #[test]
    fn outside_drop_focus_loss_and_enemy_piece_do_not_move() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let e2 = Point::new(249.0, 325.0);
        let outside = Point::new(500.0, 325.0);
        h.press(e2);
        h.event(
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
            outside,
        );
        assert!(matches!(
            h.release(outside).as_slice(),
            [Message::Dropped(Square::E2, None, _)]
        ));
        h.press(e2);
        h.event(
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
            outside,
        );
        assert!(
            h.event(Event::Window(iced::window::Event::Unfocused), outside)
                .is_empty()
        );
        assert!(h.release(e2).is_empty());
        let e7 = Point::new(249.0, 75.0);
        h.press(e7);
        assert!(
            h.tree
                .state
                .downcast_ref::<State>()
                .press
                .unwrap()
                .draggable
        );
        assert!(
            h.tree
                .state
                .downcast_ref::<State>()
                .preview_destinations
                .is_empty()
        );
        assert!(
            h.event(Event::Mouse(mouse::Event::CursorMoved { position: e2 }), e2)
                .is_empty()
        );
        assert!(h.release(e2).is_empty());
    }
    #[test]
    fn position_and_flip_changes_cancel_pending_pointer_gestures() {
        let mut app = App::default();
        let mut tree = {
            let mut h = Harness::new(&app);
            h.press(Point::new(249.0, 325.0));
            h.tree
        };
        app.flipped = true;
        Harness::new(&app).widget.diff(&mut tree);
        assert!(tree.state.downcast_ref::<State>().press.is_none());
        tree.state.downcast_mut::<State>().press = Some(Press {
            square: Square::E2,
            origin: Point::ORIGIN,
            piece_bounds: Rectangle::default(),
            draggable: true,
            dragging: true,
        });
        app.game.play(Square::E2, Square::E4, None).unwrap();
        let h = Harness::new(&app);
        h.widget.diff(&mut tree);
        assert!(tree.state.downcast_ref::<State>().press.is_none());
    }
    #[test]
    fn native_touch_supports_taps_drag_and_lost_finger() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let e2 = Point::new(249.0, 325.0);
        let e4 = Point::new(249.0, 225.0);
        let id = touch::Finger(1);
        h.event(
            Event::Touch(touch::Event::FingerPressed { id, position: e2 }),
            Point::ORIGIN,
        );
        assert!(matches!(
            h.event(
                Event::Touch(touch::Event::FingerLifted { id, position: e2 }),
                Point::ORIGIN
            )
            .as_slice(),
            [Message::BoardClick(Square::E2, _)]
        ));
        h.event(
            Event::Touch(touch::Event::FingerPressed { id, position: e2 }),
            Point::ORIGIN,
        );
        assert!(
            h.event(
                Event::Touch(touch::Event::FingerMoved { id, position: e4 }),
                Point::ORIGIN
            )
            .is_empty()
        );
        assert!(
            h.event(
                Event::Touch(touch::Event::FingerLifted {
                    id: touch::Finger(2),
                    position: e4
                }),
                Point::ORIGIN
            )
            .is_empty()
        );
        assert!(matches!(
            h.event(
                Event::Touch(touch::Event::FingerLifted { id, position: e4 }),
                Point::ORIGIN
            )
            .as_slice(),
            [Message::Dropped(Square::E2, Some(Square::E4), _)]
        ));
        h.event(
            Event::Touch(touch::Event::FingerPressed { id, position: e2 }),
            Point::ORIGIN,
        );
        h.event(
            Event::Touch(touch::Event::FingerMoved { id, position: e4 }),
            Point::ORIGIN,
        );
        assert!(
            h.event(
                Event::Touch(touch::Event::FingerLost { id, position: e4 }),
                Point::ORIGIN
            )
            .is_empty()
        );
        assert!(h.tree.state.downcast_ref::<State>().press.is_none());
    }
    #[test]
    fn focus_loss_releases_touch_ownership_before_mouse_input() {
        let app = App::default();
        let mut h = Harness::new(&app);
        let e2 = Point::new(249.0, 325.0);
        for start in [e2, Point::ORIGIN] {
            h.event(
                Event::Touch(touch::Event::FingerPressed {
                    id: touch::Finger(1),
                    position: start,
                }),
                Point::ORIGIN,
            );
            h.event(Event::Window(iced::window::Event::Unfocused), Point::ORIGIN);
            h.press(e2);
            assert!(matches!(
                h.release(e2).as_slice(),
                [Message::BoardClick(Square::E2, _)]
            ));
        }
    }
    #[test]
    fn hit_testing_excludes_labels_and_edges_and_handles_flip() {
        let bounds = Rectangle::new(Point::new(100.0, 50.0), Size::new(424.0, 420.0));
        let at = |x, y, flipped| square_under(bounds, 50.0, Point::new(x, y), flipped);
        assert_eq!(at(124.0, 50.0, false), Some(Square::A8));
        assert_eq!(at(124.0, 50.0, true), Some(Square::H1));
        assert_eq!(at(349.0, 375.0, false), Some(Square::E2));
        assert_eq!(at(523.9, 449.9, false), Some(Square::H1));
        assert_eq!(at(123.9, 50.0, false), None);
        assert_eq!(at(524.0, 50.0, false), None);
        assert_eq!(at(124.0, 450.0, false), None);
        assert_eq!(at(124.0, 49.9, false), None);
    }
}
