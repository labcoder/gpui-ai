//! Approval gates: tones, "Always allow", and resolved states.

use gpui::{
    Context, Modifiers, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext,
    Window, div, px, size,
};
use gpui_ai::approval::{ApprovalCard, ApprovalDecision, ApprovalEvent, ApprovalTone};
use gpui_component::ActiveTheme as _;
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

/// A button centres the part of its label anyone can actually see.
///
/// Not the line box — that one is easy to centre and looks wrong. GPUI centres
/// a line's ascent-to-descent box, and a font's ascent reserves room for
/// accents that `Approve` never uses, so centring it hangs the word low: eight
/// pixels above the caps and three below the descender, on a button that is
/// twenty-four tall. What has to end up centred is the band from the cap
/// height to the descent, which is the ink, and this measures that band from
/// the same metrics the label offsets itself by.
#[gpui::test]
fn a_button_centres_the_ink_of_its_label(cx: &mut TestAppContext) {
    let (_, cx) = harness(ProbeKind::Pending, cx);
    let (ascent, descent, cap, leading) = cx.update(|window, cx| {
        let body = cx.theme().typography_tokens().sm;
        let text = cx.text_system();
        let font = text.resolve_font(&window.text_style().font());
        (
            text.ascent(font, body.size),
            text.descent(font, body.size),
            text.cap_height(font, body.size),
            body.line_height,
        )
    });
    for (button, label) in [
        ("approval-approve-purge", "button-label-Approve"),
        ("approval-reject-purge", "button-label-Reject"),
    ] {
        let button = cx
            .debug_bounds(button)
            .unwrap_or_else(|| panic!("{button} should render"));
        let label = cx
            .debug_bounds(label)
            .unwrap_or_else(|| panic!("{label} should render"));
        // Where GPUI puts the baseline inside the line box, which starts at
        // the top of the label's box because the padding that raises it is
        // all underneath.
        let baseline = label.top() + (leading - ascent - descent) / 2.0 + ascent;
        let above = baseline - cap - button.top();
        let below = button.bottom() - (baseline + descent);
        assert!(
            (above - below).abs() <= px(1.0),
            "a button must centre its label's ink: {above:?} above the caps,              {below:?} below the descent"
        );
    }
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
