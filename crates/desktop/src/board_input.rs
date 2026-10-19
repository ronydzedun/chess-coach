// SPDX-License-Identifier: GPL-3.0-or-later
//! Pointer capture for the board. Chess state changes only after a validated drop.
use crate::{App, Message};
use chess_core::{Square, square_at};
use iced::advanced::{
    Clipboard, Layout, Shell, Widget, layout, mouse, renderer, svg,
    widget::{Tree, tree},
};
use iced::{Element, Event, Length, Point, Rectangle, Size, Theme, touch};
use svg::Renderer as _;

pub(super) struct BoardInput<'a> {
    pub content: Element<'a, Message>,
    pub app: &'a App,
    pub cell: f32,
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
                    let draggable = self
                        .app
                        .game
                        .piece_at(square)
                        .is_some_and(|p| p.color == self.app.game.turn());
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
                    if draggable {
                        // Prepare once on press. All motion, including crossing the
                        // click threshold, stays local and only requests a redraw.
                        shell.publish(Message::DragStarted(square));
                    }
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
                    if press.dragging {
                        shell.publish(Message::Dropped(press.square, square));
                    } else if square == Some(press.square) {
                        shell.publish(Message::ClickReleased(press.square));
                    } else if press.draggable {
                        shell.publish(Message::Dropped(press.square, None));
                    }
                    shell.request_redraw();
                    shell.capture_event();
                }
            }
            Event::Window(iced::window::Event::Unfocused | iced::window::Event::Resized(_))
            | Event::Mouse(mouse::Event::CursorLeft) => {
                state.finger = None;
                state.position = None;
                if state.press.take().is_some_and(|p| p.draggable) {
                    shell.publish(Message::Cancel);
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
    }
    impl<'a> Harness<'a> {
        fn new(app: &'a App) -> Self {
            let widget = BoardInput {
                content: iced::widget::Space::new().width(424).height(424).into(),
                app,
                cell: 50.0,
            };
            let tree = Tree::new(&widget as &dyn Widget<Message, Theme, iced::Renderer>);
            Self {
                widget,
                tree,
                renderer: iced::Renderer::new(iced::Font::default(), iced::Pixels(16.0)),
                node: layout::Node::new(Size::new(424.0, 424.0)),
            }
        }
        fn event(&mut self, event: Event, point: Point) -> Vec<Message> {
            let mut messages = Vec::new();
            self.widget.update(
                &mut self.tree,
                &event,
                Layout::new(&self.node),
                mouse::Cursor::Available(point),
                &self.renderer,
                &mut iced::advanced::clipboard::Null,
                &mut Shell::new(&mut messages),
                &Rectangle::with_size(Size::new(1000.0, 1000.0)),
            );
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
            assert!(
                messages.is_empty() || matches!(messages.as_slice(), [Message::DragStarted(_)])
            );
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
        assert!(matches!(
            h.event(
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                e4
            )
            .as_slice(),
            [Message::DragStarted(Square::E2)]
        ));
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
            [Message::Dropped(Square::E2, Some(Square::E4))]
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
            assert!(app.dragged.is_none());
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
            [Message::ClickReleased(Square::E4)]
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
            [Message::ClickReleased(Square::E2)]
        ));
        h.press(e2);
        assert!(
            h.event(Event::Mouse(mouse::Event::CursorMoved { position: e4 }), e4)
                .is_empty()
        );
        assert!(matches!(
            h.release(e4).as_slice(),
            [Message::Dropped(Square::E2, Some(Square::E4))]
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
            [Message::Dropped(Square::E2, None)]
        ));
        h.press(e2);
        h.event(
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
            outside,
        );
        assert!(matches!(
            h.event(Event::Window(iced::window::Event::Unfocused), outside)
                .as_slice(),
            [Message::Cancel]
        ));
        assert!(h.release(e2).is_empty());
        let e7 = Point::new(249.0, 75.0);
        h.press(e7);
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
            [Message::ClickReleased(Square::E2)]
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
            [Message::Dropped(Square::E2, Some(Square::E4))]
        ));
        h.event(
            Event::Touch(touch::Event::FingerPressed { id, position: e2 }),
            Point::ORIGIN,
        );
        h.event(
            Event::Touch(touch::Event::FingerMoved { id, position: e4 }),
            Point::ORIGIN,
        );
        assert!(matches!(
            h.event(
                Event::Touch(touch::Event::FingerLost { id, position: e4 }),
                Point::ORIGIN
            )
            .as_slice(),
            [Message::Cancel]
        ));
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
                [Message::ClickReleased(Square::E2)]
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
