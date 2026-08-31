//! The smallest complete gpui-ai application: one window, one approval gate.
//!
//! `cargo run -p gallery --example minimal`
//!
//! The gallery next door shows every component; this shows the wiring, which
//! is the part that is the same for all of them:
//!
//! - `gpui_ai::init` once, before any window (it initialises gpui-component
//!   too, so there is no second call to remember)
//! - a `gpui_component::Root` wrapping the top-level view
//! - a stateless component built where it is rendered, emitting typed events
//!   through `on_event`
//! - a caller's own styles on that component, over its defaults
//! - a decoration under its content, rounded by `decoration::frame_radius`
//!
//! Nothing here is gallery-specific. Copy it into a new binary, swap the
//! component, and it still runs.

use gpui::{App, Context, Entity, Window, WindowOptions, div, prelude::*, px};
use gpui_ai::prelude::*;
use gpui_component::{ActiveTheme as _, Root, v_flex};

struct Demo {
    /// What the gate decided, once it has decided anything.
    decision: Option<ApprovalDecision>,
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let gate = ApprovalCard::new("publish", "Publish the launch plan?")
            .description("Recipients cannot be recalled once this goes out.")
            .decision(self.decision.unwrap_or(ApprovalDecision::Pending))
            // A caller's styles land on the component's own frame, after its
            // defaults. Both of these outrank what the card would have chosen.
            .border_color(cx.theme().border)
            .when(self.decision.is_none(), |card| {
                // Under the content, over the background. It rounds itself,
                // because nothing can clip a subtree to a corner radius.
                card.decoration(Decoration::behind(
                    div()
                        .size_full()
                        .rounded(decoration::frame_radius(cx))
                        .bg(cx.theme().primary.opacity(0.06)),
                ))
            })
            .on_event(
                cx.listener(|demo: &mut Self, event: &ApprovalEvent, _, cx| {
                    demo.decision = Some(match event {
                        ApprovalEvent::Approved { .. } | ApprovalEvent::ApprovedAlways { .. } => {
                            ApprovalDecision::Approved
                        }
                        ApprovalEvent::Rejected { .. } => ApprovalDecision::Rejected,
                    });
                    cx.notify();
                }),
            );

        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_4()
            .bg(cx.theme().background)
            .child(div().w(px(420.)).child(gate))
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx: &mut App| {
            // Once, before any window. This initialises gpui-component as
            // well, so an application does not call both.
            gpui_ai::init(cx);

            let demo: Entity<Demo> = cx.new(|_| Demo { decision: None });
            cx.open_window(WindowOptions::default(), move |window, cx| {
                cx.new(|cx| Root::new(demo, window, cx).bg(cx.theme().background))
            })
            .expect("a window");
            cx.activate(true);
        });
}
