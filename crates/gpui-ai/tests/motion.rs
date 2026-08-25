//! Reorder: a row that changes places is carried from where it was.

use gpui::{
    App, Context, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext,
    Window, canvas, div, px, size,
};
use gpui_ai::motion::reorder;
use std::{
    cell::Cell,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Duration,
};

/// One row under a spacer whose height the test controls.
///
/// Growing the spacer moves the row down the page, which is what a reorder
/// does to a row without needing a list to reorder. The row reports where it
/// was actually painted — including whatever `reorder` displaced it by — into
/// `painted`.
struct Probe {
    spacer: Rc<Cell<f32>>,
    painted: Arc<Mutex<Option<f32>>>,
}

impl Render for Probe {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let painted = self.painted.clone();
        let row = div().h(px(40.)).w_full().child(
            canvas(
                move |bounds, _, _| {
                    *painted.lock().expect("probe mutex") = Some(f32::from(bounds.origin.y));
                },
                |_, _, _, _| {},
            )
            .absolute()
            .inset_0(),
        );
        div()
            .size_full()
            .child(div().h(px(self.spacer.get())).w_full())
            .child(reorder(row, "probe-row", window, cx))
    }
}

fn painted_at(painted: &Arc<Mutex<Option<f32>>>) -> f32 {
    painted
        .lock()
        .expect("probe mutex")
        .expect("the row should have painted")
}

fn harness(cx: &mut TestAppContext) -> (Rc<Cell<f32>>, Arc<Mutex<Option<f32>>>, &mut VisualTestContext)
{
    cx.update(gpui_ai::init);
    let spacer = Rc::new(Cell::new(20.0));
    let painted = Arc::new(Mutex::new(None));
    let (_, cx) = cx.add_window_view({
        let spacer = spacer.clone();
        let painted = painted.clone();
        move |_, _| Probe { spacer, painted }
    });
    cx.simulate_resize(size(px(400.), px(400.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (spacer, painted, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(16));
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn a_row_that_changes_places_is_carried_from_where_it_was(cx: &mut TestAppContext) {
    let (spacer, painted, cx) = harness(cx);
    let was = painted_at(&painted);

    // The row now belongs 100px further down. Drawn plainly it would simply be
    // there on the next frame, which reads as the list redrawing rather than
    // the row moving.
    spacer.set(120.0);
    draw(cx);
    let belongs = painted_at(&painted);
    assert!(
        (belongs - was - 100.0).abs() < 1.0,
        "the probe should move the row by 100px, not {}",
        belongs - was
    );

    // Carried: the frame after the move draws it back near where it was.
    draw(cx);
    let carried = painted_at(&painted);
    assert!(
        carried < belongs - 50.0,
        "a moved row must be carried from where it was: it belongs at {belongs} \
         and was drawn at {carried}"
    );

    // And it arrives, in about the second the spring is tuned for.
    for _ in 0..90 {
        tick(cx);
    }
    let arrived = painted_at(&painted);
    assert!(
        (arrived - belongs).abs() < 1.0,
        "the row stopped {}px from where it belongs",
        arrived - belongs
    );
}

#[gpui::test]
fn a_row_that_has_not_moved_is_never_displaced(cx: &mut TestAppContext) {
    let (_, painted, cx) = harness(cx);
    let settled = painted_at(&painted);

    // The common case by far: a list redraws and nothing changed places.
    for _ in 0..6 {
        tick(cx);
        assert_eq!(
            painted_at(&painted),
            settled,
            "a row nobody moved must not drift"
        );
    }
}

#[gpui::test]
fn reduced_motion_moves_the_row_without_carrying_it(cx: &mut TestAppContext) {
    let (spacer, painted, cx) = harness(cx);
    cx.update(|_, cx: &mut App| cx.set_reduce_motion(true));
    draw(cx);

    spacer.set(120.0);
    draw(cx);
    let belongs = painted_at(&painted);

    // Every later frame draws it in the same place: a reader who asked for
    // less motion gets a list that is reordered, not one that is reordering.
    for _ in 0..4 {
        draw(cx);
        assert_eq!(
            painted_at(&painted),
            belongs,
            "reduced motion must not carry a row anywhere"
        );
    }
}
