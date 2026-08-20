use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, Modifiers,
    MouseButton, ParentElement as _, Render, Styled as _, TestAppContext, VisualTestContext,
    Window, div, point, px,
};
use gpui_component::Root;
use mighty_gpui::{
    code_block::CodeBlock,
    insight::InsightCard,
    selection_actions::{SelectionAction, SelectionActions},
    stream::Progressive,
    streaming_text::{CitationRef, StreamingText},
    thinking::{StepStatus, Thinking, ThinkingStep, ThinkingTrace},
};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Surface {
    Answer,
    CitationAnswer,
    CitationLabel,
    Code,
    ThinkingProse,
    ThinkingDetail,
    Insight,
    SelectionActions,
}

struct ReadableSurface {
    surface: Surface,
    selection: Option<gpui::Entity<SelectionActions>>,
}

impl ReadableSurface {
    fn new(surface: Surface, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let selection = (surface == Surface::SelectionActions).then(|| {
            let selection = cx.new(|cx| {
                SelectionActions::new(
                    "readable-selection",
                    "selectable_action selectable_action",
                    window,
                    cx,
                )
            });
            selection.update(cx, |selection, cx| {
                selection.set_actions([SelectionAction::new("ask", "Ask")], cx)
            });
            selection
        });
        Self { surface, selection }
    }
}

impl Render for ReadableSurface {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let surface: AnyElement = match self.surface {
            Surface::Answer => {
                let content = Progressive::complete("Selectable answer text".to_owned());
                StreamingText::new("answer", &content).into_any_element()
            }
            Surface::CitationAnswer => {
                let content = Progressive::complete(
                    "Readable prose cites [[cite:pricing]] without losing selection.".to_owned(),
                );
                StreamingText::new("citation-answer", &content)
                    .citations([CitationRef::new(
                        "pricing",
                        "Pricing report",
                        "Open the pricing report",
                        "app://reports/pricing",
                    )])
                    .on_event(|_, _, _| {})
                    .into_any_element()
            }
            Surface::CitationLabel => {
                let content = Progressive::complete(
                    "Readable prose cites [[cite:pricing]] without a handler.".to_owned(),
                );
                StreamingText::new("citation-label", &content)
                    .citations([CitationRef::new(
                        "pricing",
                        "Pricing report",
                        "Open the pricing report",
                        "app://reports/pricing",
                    )])
                    .into_any_element()
            }
            Surface::Code => CodeBlock::new("code", "selectable_code selectable_code")
                .language("rust")
                .into_any_element(),
            Surface::ThinkingProse => {
                let trace =
                    Progressive::complete(ThinkingTrace::new().prose("Selectable reasoning prose"));
                Thinking::new("thinking-prose", &trace)
                    .open(true)
                    .into_any_element()
            }
            Surface::ThinkingDetail => {
                let trace = Progressive::complete(
                    ThinkingTrace::new().steps([ThinkingStep::new("Inspect evidence")
                        .detail("Selectable reasoning detail")
                        .status(StepStatus::Done)]),
                );
                Thinking::new("thinking-detail", &trace)
                    .open(true)
                    .into_any_element()
            }
            Surface::Insight => InsightCard::new("insight", "Demand changed")
                .body("selectable_insight selectable_insight")
                .into_any_element(),
            Surface::SelectionActions => self
                .selection
                .as_ref()
                .expect("selection entity is created for the selection surface")
                .clone()
                .into_any_element(),
        };

        div().size_full().p(px(16.)).child(
            div()
                .debug_selector(|| "readable-surface".into())
                .w_full()
                .child(surface),
        )
    }
}

fn select_text(surface: Surface, cx: &mut TestAppContext) -> String {
    cx.update(mighty_gpui::init);
    let (_, cx) = cx.add_window_view(move |window, cx| {
        let content = cx.new(|cx| ReadableSurface::new(surface, window, cx));
        Root::new(content, window, cx)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let bounds = cx
        .debug_bounds("readable-surface")
        .expect("readable surface should be rendered");
    let (content_x, content_y) = match surface {
        Surface::Answer | Surface::CitationAnswer | Surface::CitationLabel => {
            (bounds.left() + px(1.), bounds.top() + px(1.))
        }
        Surface::Code => (
            bounds.left() + px(48.),
            bounds.top() + bounds.size.height * 0.68,
        ),
        Surface::ThinkingProse => (
            bounds.left() + px(21.),
            bounds.top() + bounds.size.height * 0.70,
        ),
        Surface::ThinkingDetail => (
            bounds.left() + px(37.),
            bounds.top() + bounds.size.height * 0.74,
        ),
        Surface::Insight => (bounds.left() + px(17.), bounds.top() + px(80.)),
        Surface::SelectionActions => (bounds.left() + px(14.), bounds.top() + px(14.)),
    };
    let from = point(content_x, content_y);
    let to = point(bounds.left() + px(600.), bounds.bottom() - px(1.));

    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });

    cx.update(|window, cx| gpui_base::TextSelection::selected_text(window, cx))
}

#[gpui::test]
fn streamed_answers_export_selected_text(cx: &mut TestAppContext) {
    let selected = select_text(Surface::Answer, cx);

    assert!(selected.contains("Selectable answer text"), "{selected:?}");
}

#[gpui::test]
fn inline_citations_preserve_readable_selected_text(cx: &mut TestAppContext) {
    let selected = select_text(Surface::CitationAnswer, cx);

    assert!(selected.contains("Readable prose cites"), "{selected:?}");
    assert!(selected.contains("Pricing report"), "{selected:?}");
    assert!(!selected.contains("cite:pricing"), "{selected:?}");
    assert!(!selected.contains("mighty-citation"), "{selected:?}");
}

#[gpui::test]
fn citations_without_handlers_render_readable_non_link_labels(cx: &mut TestAppContext) {
    let selected = select_text(Surface::CitationLabel, cx);

    assert!(selected.contains("[Pricing report]"), "{selected:?}");
    assert!(!selected.contains("cite:pricing"), "{selected:?}");
    assert!(!selected.contains("mighty-citation"), "{selected:?}");
}

#[gpui::test]
fn code_blocks_export_selected_text(cx: &mut TestAppContext) {
    let selected = select_text(Surface::Code, cx);

    assert!(selected.contains("selectable_code"), "{selected:?}");
}

#[gpui::test]
fn thinking_prose_exports_selected_text(cx: &mut TestAppContext) {
    let selected = select_text(Surface::ThinkingProse, cx);

    assert!(
        selected.contains("Selectable reasoning prose"),
        "{selected:?}"
    );
}

#[gpui::test]
fn thinking_details_export_selected_text(cx: &mut TestAppContext) {
    let selected = select_text(Surface::ThinkingDetail, cx);

    assert!(selected.contains("reasoning detail"), "{selected:?}");
}

#[gpui::test]
fn insight_bodies_export_selected_text(cx: &mut TestAppContext) {
    let selected = select_text(Surface::Insight, cx);

    assert!(selected.contains("selectable_insight"), "{selected:?}");
}

#[gpui::test]
fn selection_action_surfaces_export_selected_text(cx: &mut TestAppContext) {
    let selected = select_text(Surface::SelectionActions, cx);

    assert!(selected.contains("selectable_action"), "{selected:?}");
}
