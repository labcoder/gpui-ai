use gpui::{
    AppContext as _, Context, Entity, Modifiers, ParentElement as _, Render, Styled as _,
    Subscription, TestAppContext, VisualTestContext, Window, div, px, size,
};
use gpui_ai::thread_list::{ThreadItem, ThreadList, ThreadListEvent, ThreadSection};
use std::{cell::RefCell, rc::Rc};

struct Probe {
    threads: Entity<ThreadList>,
    _subscription: Subscription,
}

fn sections() -> Vec<ThreadSection> {
    vec![
        ThreadSection::new("today", "Today").items([
            ThreadItem::new("supplier", "Supplier pricing review").subtitle("2 min ago"),
            ThreadItem::new("cold-chain", "Cold-chain capacity check").subtitle("1 h ago"),
        ]),
        ThreadSection::new("earlier", "Earlier").items([
            ThreadItem::new("margins", "Q2 margin analysis").subtitle("Aug 12"),
            ThreadItem::new("packaging", "Packaging vendor shortlist")
                .subtitle("Aug 3")
                .archived(true),
        ]),
    ]
}

impl Probe {
    fn new(
        events: Rc<RefCell<Vec<ThreadListEvent>>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let threads = cx.new(|cx| {
            let mut list = ThreadList::new("probe", window, cx);
            list.set_sections(sections(), cx);
            list.set_active(Some("supplier"), cx);
            list
        });
        let subscription = cx.subscribe(&threads, move |_, _, event: &ThreadListEvent, _| {
            events.borrow_mut().push(event.clone());
        });
        Self {
            threads,
            _subscription: subscription,
        }
    }
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div().size_full().child(self.threads.clone())
    }
}

fn harness(
    cx: &mut TestAppContext,
    height: f32,
) -> (
    Entity<Probe>,
    Rc<RefCell<Vec<ThreadListEvent>>>,
    &mut VisualTestContext,
) {
    cx.update(gpui_ai::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let (view, cx) = cx.add_window_view({
        let events = events.clone();
        move |window, cx| Probe::new(events, window, cx)
    });
    cx.simulate_resize(size(px(320.), px(height)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (view, events, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should render"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    draw(cx);
}

#[gpui::test]
fn selecting_and_row_actions_report_stable_ids(cx: &mut TestAppContext) {
    let (_, events, cx) = harness(cx, 600.);
    click(cx, "thread-cold-chain");
    assert!(cx.debug_bounds("thread-rename-supplier").is_none());
    click(cx, "thread-more-supplier");
    assert!(cx.debug_bounds("thread-rename-supplier").is_some());
    click(cx, "thread-rename-supplier");
    click(cx, "thread-delete-supplier");
    assert_eq!(
        events.borrow().as_slice(),
        &[
            ThreadListEvent::Selected {
                id: "cold-chain".into()
            },
            ThreadListEvent::RenameRequested {
                id: "supplier".into()
            },
            ThreadListEvent::DeleteRequested {
                id: "supplier".into()
            },
        ]
    );
    // Destructive actions close the row's toolbar instead of leaving stale targets.
    assert!(cx.debug_bounds("thread-delete-supplier").is_none());
    click(cx, "thread-list-new");
    assert_eq!(events.borrow().last(), Some(&ThreadListEvent::NewRequested));
}

#[gpui::test]
fn archived_threads_hide_until_requested_and_can_be_restored(cx: &mut TestAppContext) {
    let (_, events, cx) = harness(cx, 600.);
    assert!(cx.debug_bounds("thread-packaging").is_none());
    click(cx, "thread-list-archived-toggle");
    assert!(cx.debug_bounds("thread-packaging").is_some());
    click(cx, "thread-more-packaging");
    click(cx, "thread-archive-packaging");
    assert_eq!(
        events.borrow().as_slice(),
        &[ThreadListEvent::UnarchiveRequested {
            id: "packaging".into()
        }]
    );
}

#[gpui::test]
fn programmatic_query_filters_rows_and_reports_no_matches(cx: &mut TestAppContext) {
    let (view, events, cx) = harness(cx, 600.);
    view.update_in(cx, |probe, window, cx| {
        probe
            .threads
            .update(cx, |threads, cx| threads.set_query("margin", window, cx));
    });
    draw(cx);
    assert!(cx.debug_bounds("thread-supplier").is_none());
    assert!(cx.debug_bounds("thread-margins").is_some());
    assert_eq!(
        events.borrow().as_slice(),
        &[ThreadListEvent::QueryChanged {
            query: "margin".into()
        }]
    );

    view.update_in(cx, |probe, window, cx| {
        probe
            .threads
            .update(cx, |threads, cx| threads.set_query("zzz", window, cx));
    });
    draw(cx);
    assert!(cx.debug_bounds("thread-margins").is_none());
    assert!(cx.debug_bounds("thread-list-empty").is_some());
}

#[gpui::test]
fn active_thread_survives_a_reordered_snapshot(cx: &mut TestAppContext) {
    let (view, _, cx) = harness(cx, 600.);
    view.update_in(cx, |probe, _, cx| {
        probe.threads.update(cx, |threads, cx| {
            let mut reordered = sections();
            reordered.reverse();
            threads.set_sections(reordered, cx);
        });
    });
    draw(cx);
    let active = view.read_with(cx, |probe, cx| probe.threads.read(cx).active_id().cloned());
    assert_eq!(active.as_deref(), Some("supplier"));
    assert!(cx.debug_bounds("thread-supplier").is_some());
}

#[gpui::test]
fn constrained_list_keeps_the_last_thread_reachable(cx: &mut TestAppContext) {
    // Geometry, not motion, is under test: settle the row reveals.
    cx.update(|cx| cx.set_reduce_motion(true));
    let (view, _, cx) = harness(cx, 150.);
    let host = cx
        .debug_bounds("thread-list-scroll")
        .expect("scroll region should render");
    view.update_in(cx, |probe, _, cx| {
        probe
            .threads
            .update(cx, |threads, cx| threads.scroll_to_end(cx));
    });
    draw(cx);
    let last = cx
        .debug_bounds("thread-margins")
        .expect("last visible thread should render");
    assert!(
        last.bottom() <= host.bottom() + px(1.),
        "last thread {last:?} must be inside the scroll region {host:?}"
    );
    assert!(last.top() >= host.top() - px(1.));
}
