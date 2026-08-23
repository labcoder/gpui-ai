//! Approval gates: tones, "Always allow", and resolved states.

use gpui::{
    Context, Modifiers, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext,
    Window, div, px, size,
};
use gpui_ai::approval::{ApprovalCard, ApprovalDecision, ApprovalEvent, ApprovalTone};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy)]
enum ProbeKind {
    Pending,
    Approved,
    Rejected,
}

struct Probe {
    kind: ProbeKind,
    events: Rc<RefCell<Vec<ApprovalEvent>>>,
}

impl Render for Probe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        let events = self.events.clone();
        let handler = move |event: &ApprovalEvent, _: &mut Window, _: &mut gpui::App| {
            events.borrow_mut().push(event.clone());
        };
        let card = ApprovalCard::new("purge", "Delete 12 stale records?")
            .description("Removed permanently.")
            .tone(ApprovalTone::Destructive)
            .allow_always(true)
            .note("Decided by Oscar");
        let card = match self.kind {
            ProbeKind::Pending => card,
            ProbeKind::Approved => card.decision(ApprovalDecision::Approved),
            ProbeKind::Rejected => card.decision(ApprovalDecision::Rejected),
        };
        div().size_full().child(card.on_event(handler))
    }
}

fn harness(
    kind: ProbeKind,
    cx: &mut TestAppContext,
) -> (Rc<RefCell<Vec<ApprovalEvent>>>, &mut VisualTestContext) {
    cx.update(gpui_ai::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let (_, cx) = cx.add_window_view({
        let events = events.clone();
        move |_, _| Probe { kind, events }
    });
    cx.simulate_resize(size(px(640.), px(400.)));
    cx.update(|window, cx| window.draw(cx).clear(cx));
    (events, cx)
}

fn click_center(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} should render"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn pending_gates_offer_every_decision_including_always_allow(cx: &mut TestAppContext) {
    let (events, cx) = harness(ProbeKind::Pending, cx);
    assert!(cx.debug_bounds("approval-decision-purge").is_none());
    click_center(cx, "approval-approve-purge");
    click_center(cx, "approval-always-purge");
    click_center(cx, "approval-reject-purge");
    assert_eq!(
        events.borrow().as_slice(),
        &[
            ApprovalEvent::Approved { id: "purge".into() },
            ApprovalEvent::ApprovedAlways { id: "purge".into() },
            ApprovalEvent::Rejected { id: "purge".into() },
        ]
    );
}

#[gpui::test]
fn resolved_gates_show_the_decision_instead_of_buttons(cx: &mut TestAppContext) {
    let (_, cx) = harness(ProbeKind::Approved, cx);
    assert!(cx.debug_bounds("approval-decision-purge").is_some());
    assert!(cx.debug_bounds("approval-approve-purge").is_none());
    assert!(cx.debug_bounds("approval-always-purge").is_none());
    assert!(cx.debug_bounds("approval-reject-purge").is_none());

    let (_, cx) = harness(ProbeKind::Rejected, cx);
    assert!(cx.debug_bounds("approval-decision-purge").is_some());
    assert!(cx.debug_bounds("approval-approve-purge").is_none());
}

#[test]
fn decision_labels_read_as_badges() {
    assert_eq!(ApprovalDecision::Pending.label(), "Pending");
    assert_eq!(ApprovalDecision::Approved.label(), "Approved");
    assert_eq!(ApprovalDecision::Rejected.label(), "Rejected");
}
