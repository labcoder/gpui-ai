use gpui::{
    AppContext as _, ClipboardItem, Context, Element as _, Entity, InteractiveElement as _,
    IntoElement as _, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, MouseButton,
    ParentElement as _, Render, RenderOnce as _, Role, ScrollDelta, ScrollWheelEvent, Styled as _,
    Subscription, TestAppContext, VisualTestContext, Window, accesskit, canvas, div, point, px,
};
use gpui_ai::prelude::{
    Chat, ChatEvent, ChatMessage, ChatRole, CommandSearch, CommandSearchEvent, CommandSearchItem,
    FineTuneCard, FineTuneEvent, FineTuneTypeface, FineTuneValues, SidebarNav, SidebarNavEvent,
    SidebarNavItem, SidebarSection,
};
use gpui_ai::{
    approval::ApprovalCard,
    code_block::CodeBlock,
    insight::{InsightCard, InsightMetric, InsightPoint},
    prompt_bar::{PromptBar, PromptBarEvent, PromptModel},
    recommendation::RecommendationCard,
    search_results::{SearchResult, SearchResults},
    selection_actions::{SelectionAction, SelectionActions, SelectionActionsEvent},
    stream::{ProgressState, Progressive},
    streaming_text::{CitationRef, SourceRef, StreamingText, StreamingTextEvent},
    task::{TaskRow, TaskSnapshot},
    thinking::{Thinking, ThinkingTrace},
    todo_list::{TodoItem, TodoList},
};
use gpui_component::IconName;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{Arc, Mutex},
};

#[derive(Clone, Copy)]
enum ComponentProbeKind {
    Approval,
    Recommendation,
    Task,
    Code,
    Search,
    Todo,
    StreamingText,
    Thinking,
    Insight,
}

struct CapturedNode {
    role: Option<Role>,
    node: accesskit::Node,
}

struct PublicPromptProbe {
    prompt: Entity<PromptBar>,
    events: Rc<RefCell<Vec<PromptBarEvent>>>,
    _subscription: Subscription,
}

struct PublicSelectionProbe {
    selection: Entity<SelectionActions>,
    events: Rc<RefCell<Vec<SelectionActionsEvent>>>,
    _subscription: Subscription,
}

struct PublicCitationProbe {
    events: Rc<RefCell<Vec<StreamingTextEvent>>>,
}

struct PublicChatProbe {
    chat: Entity<Chat>,
    events: Rc<RefCell<Vec<ChatEvent>>>,
    _subscription: Subscription,
}

struct PublicCommandSearchProbe {
    search: Entity<CommandSearch>,
    events: Rc<RefCell<Vec<CommandSearchEvent>>>,
    _subscription: Subscription,
}

struct PublicSidebarNavProbe {
    nav: Entity<SidebarNav>,
    events: Rc<RefCell<Vec<SidebarNavEvent>>>,
    _subscription: Subscription,
}

struct PublicFineTuneProbe {
    card: Entity<FineTuneCard>,
    events: Rc<RefCell<Vec<FineTuneEvent>>>,
    _subscription: Subscription,
}

struct SelectionTestRoot<V: Render + 'static> {
    view: Entity<V>,
}

impl<V: Render + 'static> SelectionTestRoot<V> {
    fn new(view: Entity<V>) -> Self {
        Self { view }
    }

    fn on_copy(
        &mut self,
        _: &gpui_component::input::Copy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = gpui_base::TextSelection::selected_text(window, cx)
            .trim()
            .to_string();
        if selected.is_empty() {
            cx.propagate();
        } else {
            cx.write_to_clipboard(ClipboardItem::new_string(selected));
        }
    }
}

impl<V: Render + 'static> Render for SelectionTestRoot<V> {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .id("selection-test-root")
            .key_context("Root")
            .relative()
            .size_full()
            .on_action(cx.listener(Self::on_copy))
            .child(gpui_base::TextSelectionLayer)
            .child(self.view.clone())
    }
}

struct ConstrainedFineTuneProbe {
    card: Entity<FineTuneCard>,
}

struct NarrowFineTuneProbe {
    card: Entity<FineTuneCard>,
}

impl PublicFineTuneProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let card = cx.new(|cx| {
            FineTuneCard::new(
                "public-fine-tune",
                FineTuneValues::new(320., 180., 24., 0.72, "inter-regular")
                    .accent(gpui::hsla(0.58, 0.75, 0.52, 1.)),
                [
                    FineTuneTypeface::new("inter-regular", "Inter"),
                    FineTuneTypeface::new("inter-display", "Inter"),
                ],
                window,
                cx,
            )
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let subscription = cx.subscribe(&card, move |_, _, event, _| {
            captured.borrow_mut().push(event.clone());
        });
        Self {
            card,
            events,
            _subscription: subscription,
        }
    }
}

impl Render for PublicFineTuneProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .debug_selector(|| "public-fine-tune-host".to_owned())
            .w(px(420.))
            .h(px(520.))
            .child(self.card.clone())
    }
}

impl ConstrainedFineTuneProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let card = cx.new(|cx| {
            FineTuneCard::new(
                "constrained-fine-tune",
                FineTuneValues::new(320., 180., 24., 0.72, "inter"),
                [FineTuneTypeface::new("inter", "Inter")],
                window,
                cx,
            )
        });
        Self { card }
    }
}

impl Render for ConstrainedFineTuneProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .debug_selector(|| "constrained-fine-tune-host".to_owned())
            .w(px(420.))
            .h(px(220.))
            .overflow_hidden()
            .child(self.card.clone())
    }
}

impl NarrowFineTuneProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let card = cx.new(|cx| {
            FineTuneCard::new(
                "narrow-fine-tune",
                FineTuneValues::new(320., 180., 24., 0.72, "inter")
                    .accent(gpui::hsla(0.58, 0.75, 0.52, 1.)),
                [FineTuneTypeface::new("inter", "Inter")],
                window,
                cx,
            )
        });
        Self { card }
    }
}

impl Render for NarrowFineTuneProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .debug_selector(|| "narrow-fine-tune-host".to_owned())
            .w(px(216.))
            .h(px(520.))
            .overflow_hidden()
            .child(self.card.clone())
    }
}

fn sidebar_sections() -> Vec<SidebarSection> {
    vec![
        SidebarSection::new("workspace", "Workspace").items([
            SidebarNavItem::new("overview", "Overview").icon(IconName::LayoutDashboard),
            SidebarNavItem::new("orders", "Orders")
                .icon(IconName::SquareTerminal)
                .badge("12")
                .children([
                    SidebarNavItem::new("history", "History"),
                    SidebarNavItem::new("suppliers", "Suppliers").children([
                        SidebarNavItem::new("supplier-risk", "Risk reports"),
                        SidebarNavItem::new("supplier-score", "Scorecards"),
                    ]),
                    SidebarNavItem::new("exports", "Exports").disabled(true),
                ]),
        ]),
        SidebarSection::new("reports", "Reports").items([
            SidebarNavItem::new("live-report", "Reports").icon(IconName::ChartPie),
            SidebarNavItem::new("archive-report", "Reports").icon(IconName::BookOpen),
        ]),
    ]
}

impl PublicSidebarNavProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let nav = cx.new(|cx| SidebarNav::new("public-sidebar", window, cx));
        nav.update(cx, |nav, cx| {
            nav.set_sections(sidebar_sections(), cx);
            nav.set_active_item("archive-report", cx);
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let subscription = cx.subscribe(&nav, move |_, _, event, _| {
            captured.borrow_mut().push(event.clone());
        });
        Self {
            nav,
            events,
            _subscription: subscription,
        }
    }
}

impl Render for PublicSidebarNavProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .debug_selector(|| "public-sidebar-host".to_owned())
            .w(px(260.))
            .h(px(520.))
            .overflow_hidden()
            .child(self.nav.clone())
    }
}

struct OverflowSidebarProbe {
    nav: Entity<SidebarNav>,
}

impl OverflowSidebarProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let nav = cx.new(|cx| SidebarNav::new("overflow-sidebar", window, cx));
        nav.update(cx, |nav, cx| {
            nav.set_sections(
                (0..40).map(|index| {
                    SidebarSection::new(format!("section-{index}"), format!("Section {index}"))
                        .items([SidebarNavItem::new(
                            format!("overflow-{index}"),
                            format!("Navigation item {index}"),
                        )])
                }),
                cx,
            );
            nav.set_active_item("overflow-39", cx);
        });
        Self { nav }
    }
}

impl Render for OverflowSidebarProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .debug_selector(|| "overflow-sidebar-host".to_owned())
            .w(px(260.))
            .h(px(220.))
            .overflow_hidden()
            .child(self.nav.clone())
    }
}

impl PublicCommandSearchProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| CommandSearch::new("public-command-search", window, cx));
        search.update(cx, |search, cx| {
            search.set_items(
                [
                    CommandSearchItem::new("pricing", "Open report")
                        .subtitle("Supplier pricing")
                        .keywords(["margin"])
                        .shortcut("Ctrl+R"),
                    CommandSearchItem::new("disabled", "Unavailable report").disabled(true),
                ],
                window,
                cx,
            );
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let subscription = cx.subscribe(&search, move |_, _, event, _| {
            captured.borrow_mut().push(event.clone());
        });
        Self {
            search,
            events,
            _subscription: subscription,
        }
    }
}

impl Render for PublicCommandSearchProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        self.search.clone()
    }
}

impl PublicChatProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prompt = cx.new(|cx| PromptBar::new("public-chat-prompt", window, cx));
        let chat = cx.new(|cx| Chat::new("public-chat", prompt, window, cx));
        chat.update(cx, |chat, cx| {
            chat.set_messages(
                Arc::from([ChatMessage::new(
                    "failed-answer",
                    ChatRole::Assistant,
                    Progressive::failed("Partial answer".to_owned(), "Network unavailable"),
                )
                .retryable(true)]),
                window,
                cx,
            );
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let subscription = cx.subscribe(&chat, move |_, _, event, _| {
            captured.borrow_mut().push(event.clone());
        });
        Self {
            chat,
            events,
            _subscription: subscription,
        }
    }
}

impl Render for PublicChatProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        self.chat.clone()
    }
}

struct BoundedCitationProbe;

impl Render for BoundedCitationProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        let citations = [
            CitationRef::new("first", "One", "Open first", "app://first"),
            CitationRef::new("second", "Two", "Open second", "app://second"),
            CitationRef::new("third", "Three", "Open third", "app://third"),
            CitationRef::new("fourth", "Four", "Open fourth", "app://fourth"),
            CitationRef::new("final", "Final", "Open final", "app://final"),
        ];
        div()
            .debug_selector(|| "bounded-citation-host".to_owned())
            .w(px(184.))
            .h(px(160.))
            .overflow_hidden()
            .child(
                StreamingText::new(
                    "bounded-citations",
                    &Progressive::complete(
                        "[[cite:first]] [[cite:second]] [[cite:third]] [[cite:fourth]] [[cite:final]]"
                            .into(),
                    ),
                )
                .citations(citations)
                .on_event(|_, _, _| {}),
            )
    }
}

impl Render for PublicCitationProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        let events = self.events.clone();
        StreamingText::new(
            "citation-answer",
            &Progressive::complete(
                "[[cite:pricing]] changed while supply held [[cite:supply]].".into(),
            ),
        )
        .citations([
            CitationRef::new(
                "pricing",
                "Pricing report",
                "Open the pricing report",
                "app://reports/pricing",
            ),
            CitationRef::new(
                "supply",
                "Supply report",
                "Open the supply report",
                "app://reports/supply",
            ),
        ])
        .on_event(move |event, _, _| events.borrow_mut().push(event.clone()))
    }
}

struct BoundedSelectionProbe {
    selection: Entity<SelectionActions>,
}

impl BoundedSelectionProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let selection = cx.new(|cx| {
            SelectionActions::new(
                "bounded-selection",
                "Selectable action words for testing outside release and narrow overflow.",
                window,
                cx,
            )
        });
        selection.update(cx, |selection, cx| {
            selection.set_actions(
                [
                    SelectionAction::new("ask", "Ask about this selection"),
                    SelectionAction::new("explain", "Explain this selected passage"),
                    SelectionAction::new("rewrite", "Rewrite this selected passage clearly"),
                    SelectionAction::new("compare", "Compare this passage with the source"),
                    SelectionAction::new("final", "Open the final long selection action"),
                ],
                cx,
            );
        });
        Self { selection }
    }
}

impl Render for BoundedSelectionProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .debug_selector(|| "bounded-selection-host".to_owned())
                    .w(px(184.))
                    .h(px(144.))
                    .child(self.selection.clone()),
            )
            .child(
                div()
                    .debug_selector(|| "selection-actions-outside-target".to_owned())
                    .flex_1(),
            )
    }
}

impl PublicSelectionProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let selection = cx.new(|cx| {
            SelectionActions::new(
                "public-selection",
                "Selectable action words for testing.",
                window,
                cx,
            )
        });
        selection.update(cx, |selection, cx| {
            selection.set_actions(
                [
                    SelectionAction::new("ask", "Ask"),
                    SelectionAction::new("explain", "Explain"),
                    SelectionAction::new("rewrite", "Rewrite"),
                ],
                cx,
            );
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let _subscription = cx.subscribe(&selection, move |_, _, event, _| {
            captured.borrow_mut().push(event.clone());
        });
        Self {
            selection,
            events,
            _subscription,
        }
    }
}

impl Render for PublicSelectionProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        self.selection.clone()
    }
}

impl PublicPromptProbe {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prompt = cx.new(|cx| PromptBar::new("public-prompt", window, cx));
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        let _subscription = cx.subscribe(&prompt, move |_, _, event, _| {
            captured.borrow_mut().push(event.clone());
        });
        Self {
            prompt,
            events,
            _subscription,
        }
    }
}

impl Render for PublicPromptProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        self.prompt.clone()
    }
}

struct ComponentProbe {
    kind: ComponentProbeKind,
    captured: Arc<Mutex<Option<CapturedNode>>>,
}

impl Render for ComponentProbe {
    fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        let kind = self.kind;
        let captured = self.captured.clone();
        canvas(
            move |_, window, cx| {
                let mut node = accesskit::Node::new(Role::Unknown);
                macro_rules! write_element {
                    ($element:expr) => {{
                        let element = $element.into_element();
                        let role = element.a11y_role();
                        element.write_a11y_info(&mut node);
                        role
                    }};
                }
                let role = match kind {
                    ComponentProbeKind::Approval => write_element!(
                        ApprovalCard::new("approval", "Send supplier confirmations?")
                            .description("This action cannot be recalled")
                            .render(window, cx)
                    ),
                    ComponentProbeKind::Recommendation => write_element!(
                        RecommendationCard::new("recommendation", "Choose Alpenrose Dairy")
                            .description("Lowest price at the required volume")
                            .render(window, cx)
                    ),
                    ComponentProbeKind::Task => write_element!(
                        TaskRow::new(&Progressive::failed(
                            TaskSnapshot::new("index", "Index repository").detail("3,214 files"),
                            "Disk unavailable",
                        ))
                        .render(window, cx)
                    ),
                    ComponentProbeKind::Code => write_element!(
                        CodeBlock::new("code", "fn main() {}")
                            .language("rust")
                            .render(window, cx)
                    ),
                    ComponentProbeKind::Search => write_element!(
                        SearchResults::new("search", "gpui wasm")
                            .results([SearchResult::new("result", "GPUI Component")])
                            .render(window, cx)
                    ),
                    ComponentProbeKind::Todo => write_element!(
                        TodoList::new("todo")
                            .title("Migration plan")
                            .items([
                                TodoItem::new("read", "Read schema").done(),
                                TodoItem::new("write", "Write migration"),
                            ])
                            .render(window, cx)
                    ),
                    ComponentProbeKind::StreamingText => write_element!(
                        StreamingText::new("answer", &Progressive::complete("Forty-two".into()))
                            .render(window, cx)
                    ),
                    ComponentProbeKind::Thinking => write_element!(
                        Thinking::new("thinking", &Progressive::running(ThinkingTrace::new()))
                            .render(window, cx)
                    ),
                    ComponentProbeKind::Insight => write_element!(
                        InsightCard::new("insight", "Demand changed")
                            .page(2, 3)
                            .metrics([InsightMetric::new("mint", "Mint Chip", "$2,377.66")])
                            .series(
                                "Weekly demand",
                                [
                                    InsightPoint::new("Mon", 18.0),
                                    InsightPoint::new("Tue", 24.0)
                                ],
                            )
                            .chart_summary("Weekly demand rose from 18 to 24 orders.")
                            .follow_up("Rebalance flavors")
                            .on_event(|_, _, _| {})
                            .render(window, cx)
                    ),
                };
                *captured.lock().expect("capture mutex should be available") =
                    Some(CapturedNode { role, node });
            },
            |_, _, _, _| {},
        )
    }
}

fn capture(kind: ComponentProbeKind, cx: &mut TestAppContext) -> CapturedNode {
    cx.update(gpui_ai::init);
    let captured = Arc::new(Mutex::new(None));
    let result = captured.clone();
    let (_, cx) = cx.add_window_view(move |_, _| ComponentProbe { kind, captured });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    result
        .lock()
        .expect("capture mutex should be available")
        .take()
        .expect("component node should be captured")
}

fn activate_key(cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).expect("test key should parse");
    cx.simulate_event(KeyDownEvent {
        keystroke: keystroke.clone(),
        is_held: false,
        prefer_character_input: false,
    });
    cx.simulate_event(KeyUpEvent { keystroke });
}

struct PublicSourceProbe {
    events: Rc<RefCell<Vec<StreamingTextEvent>>>,
}

impl Render for PublicSourceProbe {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
        let events = self.events.clone();
        div().size_full().child(
            StreamingText::new("sourced", &Progressive::complete("Forty-two".into()))
                .source_refs([
                    SourceRef::new("pricing.md"),
                    SourceRef::with_id("dairy-index", "Dairy index")
                        .url("https://www.dairyreport.org/index"),
                ])
                .on_event(move |event, _, _| events.borrow_mut().push(event.clone())),
        )
    }
}

#[gpui::test]
fn public_web_sources_activate_typed_events_while_files_stay_static(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let captured = events.clone();
    let (_, cx) = cx.add_window_view(move |_, _| PublicSourceProbe { events });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(
        cx.debug_bounds("streaming-source-pricing.md").is_none(),
        "a source without a location is a static chip, never a dead button"
    );
    let chip = cx
        .debug_bounds("streaming-source-dairy-index")
        .expect("web source should be an activatable chip");
    cx.simulate_click(chip.center(), Modifiers::default());
    assert_eq!(
        captured.borrow().as_slice(),
        &[StreamingTextEvent::SourceActivated {
            id: "dairy-index".into(),
            url: "https://www.dairyreport.org/index".into(),
        }]
    );
    captured.borrow_mut().clear();

    cx.update(|window, cx| window.focus_next(cx));
    activate_key(cx, "enter");
    assert_eq!(
        captured.borrow().as_slice(),
        &[StreamingTextEvent::SourceActivated {
            id: "dairy-index".into(),
            url: "https://www.dairyreport.org/index".into(),
        }]
    );
}

#[gpui::test]
fn public_streaming_citation_companions_activate_typed_events(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let events = Rc::new(RefCell::new(Vec::new()));
    let captured = events.clone();
    let (_, cx) = cx.add_window_view(move |_, _| PublicCitationProbe { events });
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let body = cx
        .debug_bounds("streaming-text-body")
        .expect("selectable Markdown body should render");
    cx.simulate_click(
        point(body.left() + px(3.), body.top() + px(8.)),
        Modifiers::default(),
    );
    assert_eq!(
        captured.borrow().as_slice(),
        &[StreamingTextEvent::CitationActivated {
            id: "pricing".into(),
            destination: "app://reports/pricing".into(),
        }]
    );
    captured.borrow_mut().clear();

    cx.update(|window, cx| window.focus_next(cx));
    activate_key(cx, "enter");
    assert_eq!(
        captured.borrow().as_slice(),
        &[StreamingTextEvent::CitationActivated {
            id: "pricing".into(),
            destination: "app://reports/pricing".into(),
        }]
    );
    captured.borrow_mut().clear();

    let pricing = cx
        .debug_bounds("streaming-citation-pricing")
        .expect("resolved citation companion should render");
    cx.simulate_click(pricing.center(), Modifiers::default());
    assert_eq!(
        captured.borrow().as_slice(),
        &[StreamingTextEvent::CitationActivated {
            id: "pricing".into(),
            destination: "app://reports/pricing".into(),
        }]
    );

    captured.borrow_mut().clear();
    activate_key(cx, "enter");
    assert_eq!(
        captured.borrow().as_slice(),
        &[StreamingTextEvent::CitationActivated {
            id: "pricing".into(),
            destination: "app://reports/pricing".into(),
        }]
    );
    assert!(cx.debug_bounds("streaming-citation-supply").is_some());
}

#[gpui::test]
fn streaming_citation_companions_keep_the_final_link_reachable_in_a_narrow_root(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (_, cx) = cx.add_window_view(|_, _| BoundedCitationProbe);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let host = cx
        .debug_bounds("bounded-citation-host")
        .expect("bounded citation host should render");
    let scroller = cx
        .debug_bounds("streaming-citation-scroll")
        .expect("citation scroller should render");
    cx.simulate_mouse_move(scroller.center(), None, Modifiers::default());

    for _ in 0..12 {
        cx.simulate_event(ScrollWheelEvent {
            position: scroller.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-120.))),
            ..Default::default()
        });
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let final_link = cx
        .debug_bounds("streaming-citation-final")
        .expect("final citation should remain rendered after scrolling");
    assert!(
        final_link.left() >= host.left() && final_link.right() <= host.right(),
        "{final_link:?} vs {host:?}"
    );
}

#[gpui::test]
fn content_cards_are_named_groups(cx: &mut TestAppContext) {
    let approval = capture(ComponentProbeKind::Approval, cx);
    assert_eq!(approval.role, Some(Role::Group));
    assert_eq!(approval.node.label(), Some("Send supplier confirmations?"));
    assert_eq!(
        approval.node.description(),
        Some("This action cannot be recalled")
    );

    let recommendation = capture(ComponentProbeKind::Recommendation, cx);
    assert_eq!(recommendation.role, Some(Role::Group));
    assert_eq!(recommendation.node.label(), Some("Choose Alpenrose Dairy"));
    assert_eq!(
        recommendation.node.description(),
        Some("Lowest price at the required volume")
    );

    let insight = capture(ComponentProbeKind::Insight, cx);
    assert_eq!(insight.role, Some(Role::Group));
    assert_eq!(insight.node.label(), Some("Demand changed"));
    assert_eq!(
        insight.node.description(),
        Some("Insights · 2 of 3. Weekly demand rose from 18 to 24 orders.")
    );
}

#[gpui::test]
fn task_state_is_available_without_color_or_icons(cx: &mut TestAppContext) {
    let task = capture(ComponentProbeKind::Task, cx);
    assert_eq!(task.role, Some(Role::ListItem));
    assert_eq!(task.node.label(), Some("Index repository, failed"));
    assert_eq!(task.node.description(), Some("Disk unavailable"));
}

#[gpui::test]
fn composite_content_has_a_named_semantic_boundary(cx: &mut TestAppContext) {
    let code = capture(ComponentProbeKind::Code, cx);
    assert_eq!(code.role, Some(Role::Code));
    assert_eq!(code.node.label(), Some("rust code"));

    let search = capture(ComponentProbeKind::Search, cx);
    assert_eq!(search.role, Some(Role::Search));
    assert_eq!(search.node.label(), Some("1 result for “gpui wasm”"));

    let todo = capture(ComponentProbeKind::Todo, cx);
    assert_eq!(todo.role, Some(Role::List));
    assert_eq!(todo.node.label(), Some("Migration plan, 1 of 2 complete"));

    let answer = capture(ComponentProbeKind::StreamingText, cx);
    assert_eq!(answer.role, Some(Role::Article));
    assert_eq!(answer.node.label(), Some("Answer"));

    let thinking = capture(ComponentProbeKind::Thinking, cx);
    assert_eq!(thinking.role, Some(Role::Group));
    assert_eq!(thinking.node.label(), Some("Thinking…"));
}

#[gpui::test]
fn public_prompt_bar_empty_catalog_and_disabled_submit_are_noninteractive(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicPromptProbe::new);
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("prompt-bar-model-empty").is_some());
    assert!(cx.debug_bounds("prompt-bar-model-trigger").is_none());
    let submit = cx
        .debug_bounds("prompt-bar-send-control")
        .expect("disabled submit should remain visible");
    cx.simulate_click(submit.center(), Modifiers::default());

    assert!(probe.read_with(cx, |probe, _| probe.events.borrow().is_empty()));
}

#[gpui::test]
fn public_prompt_bar_assembled_controls_activate_typed_events(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicPromptProbe::new);
    cx.update(|window, cx| {
        let prompt = probe.read(cx).prompt.clone();
        prompt.update(cx, |prompt, cx| {
            prompt.set_models(
                [
                    PromptModel::new("balanced", "Balanced"),
                    PromptModel::new("fast", "Fast"),
                ],
                cx,
            );
            prompt.set_draft("Summarize this", window, cx);
        });
        window.draw(cx).clear(cx);
    });

    assert!(cx.debug_bounds("prompt-bar-model-empty").is_none());
    let model_trigger = cx
        .debug_bounds("prompt-bar-model-trigger")
        .expect("configured model trigger should render");
    cx.simulate_click(model_trigger.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let fast_model = cx
        .debug_bounds("prompt-bar-model-option-fast")
        .expect("opened model menu should render the Fast option");
    cx.simulate_click(fast_model.center(), Modifiers::default());

    let attach = cx
        .debug_bounds("prompt-bar-attach-control")
        .expect("attach control should render");
    cx.simulate_click(attach.center(), Modifiers::default());
    let enhance = cx
        .debug_bounds("prompt-bar-enhance-control")
        .expect("enhance control should render");
    cx.simulate_click(enhance.center(), Modifiers::default());
    let submit = cx
        .debug_bounds("prompt-bar-send-control")
        .expect("enabled submit should remain visible");
    cx.simulate_click(submit.center(), Modifiers::default());

    cx.update(|window, cx| {
        let prompt = probe.read(cx).prompt.clone();
        prompt.update(cx, |prompt, cx| {
            prompt.set_progress(ProgressState::Running, cx)
        });
        window.draw(cx).clear(cx);
    });
    let cancel = cx
        .debug_bounds("prompt-bar-cancel-control")
        .expect("running prompt should render cancel");
    cx.simulate_click(cancel.center(), Modifiers::default());

    assert!(probe.read_with(cx, |probe, _| {
        let events = probe.events.borrow();
        assert!(events.iter().any(|event| matches!(
            event,
            PromptBarEvent::ModelChanged { id, model_id }
                if id == "public-prompt" && model_id == "fast"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            PromptBarEvent::AttachRequested { id } if id == "public-prompt"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            PromptBarEvent::EnhanceRequested { id, draft }
                if id == "public-prompt" && draft == "Summarize this"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            PromptBarEvent::Submit { id, submission }
                if id == "public-prompt"
                    && submission.text() == "Summarize this"
                    && submission.model_id() == Some(&"balanced".into())
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            PromptBarEvent::CancelRequested { id } if id == "public-prompt"
        )));
        true
    }));
}

#[cfg_attr(
    target_os = "macos",
    ignore = "pinned GPUI TestWindow has no native macOS handle for focused InputState"
)]
#[gpui::test]
fn public_command_search_exposes_stable_keyboard_events_and_row_bounds(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicCommandSearchProbe::new);
    let cx: &mut VisualTestContext = cx;
    let search = probe.read_with(cx, |probe, _| probe.search.clone());
    cx.update(|window, cx| {
        search.update(cx, |search, cx| search.focus(window, cx));
        window.draw(cx).clear(cx);
    });

    assert!(
        cx.debug_bounds("command-search-public-command-search")
            .is_some()
    );
    assert!(cx.debug_bounds("command-search-item-pricing").is_some());
    assert!(cx.debug_bounds("command-search-item-disabled").is_some());

    cx.simulate_keystrokes("enter");
    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [CommandSearchEvent::Selected {
            id: "public-command-search".into(),
            item_id: "pricing".into(),
        }]
    );
}

#[gpui::test]
fn public_sidebar_nav_filters_recursively_and_routes_duplicate_labels_by_stable_id(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("sidebar-nav-item-live-report").is_some());
    let live = cx
        .debug_bounds("sidebar-nav-item-live-report")
        .expect("the duplicate-label item should render by stable ID");
    cx.simulate_click(
        point(live.left() + px(12.), live.center().y),
        Modifiers::default(),
    );
    activate_key(cx, "enter");
    activate_key(cx, "space");

    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("risk", window, cx));
        window.draw(cx).clear(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("sidebar-nav-item-orders").is_some());
    assert!(cx.debug_bounds("sidebar-nav-item-suppliers").is_some());
    assert!(cx.debug_bounds("sidebar-nav-item-supplier-risk").is_some());
    assert!(cx.debug_bounds("sidebar-nav-item-history").is_none());
    assert!(cx.debug_bounds("sidebar-nav-item-live-report").is_none());

    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "live-report".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "live-report".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "live-report".into(),
            },
            SidebarNavEvent::QueryChanged {
                id: "public-sidebar".into(),
                query: "risk".into(),
            },
        ]
    );
}

#[gpui::test]
fn public_sidebar_nav_native_hover_survives_stationary_pointer_replacement_and_query(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let overview = cx
        .debug_bounds("sidebar-nav-item-overview")
        .expect("expanded row should render through the production tree");
    let idle_overview_hover = cx
        .debug_bounds("sidebar-nav-hover-overview")
        .expect("expanded row should retain a stable native hover layer");
    assert!(idle_overview_hover.size.width < overview.size.width / 2.);

    cx.simulate_mouse_move(overview.center(), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let hover = cx
        .debug_bounds("sidebar-nav-hover-overview")
        .expect("expanded row should render its theme-token hover layer");
    assert!(hover.left() >= overview.left() && hover.right() <= overview.right());
    assert!(hover.size.width > overview.size.width / 2.);

    let orders = cx
        .debug_bounds("sidebar-nav-item-orders")
        .expect("second expanded row should render");
    cx.simulate_mouse_move(orders.center(), None, Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(
        cx.debug_bounds("sidebar-nav-hover-overview"),
        Some(idle_overview_hover),
        "moving directly to Orders should restore Overview's idle hover width"
    );
    let orders_hover = cx
        .debug_bounds("sidebar-nav-hover-orders")
        .expect("hover presentation should transfer between stable rows");
    assert!(orders_hover.left() >= orders.left() && orders_hover.right() <= orders.right());
    assert!(orders_hover.size.width > orders.size.width / 2.);

    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    let mut replacement = sidebar_sections();
    replacement.truncate(1);
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_sections(replacement, cx));
        window.draw(cx).clear(cx);
    });
    assert_eq!(
        cx.debug_bounds("sidebar-nav-item-orders"),
        Some(orders),
        "stable replacement should leave Orders under the stationary pointer"
    );
    assert_eq!(
        cx.debug_bounds("sidebar-nav-hover-orders"),
        Some(orders_hover),
        "stable replacement should retain native hover without a pointer move"
    );

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("workspace", window, cx));
        window.draw(cx).clear(cx);
    });
    assert_eq!(
        cx.debug_bounds("sidebar-nav-item-orders"),
        Some(orders),
        "section-label filtering should preserve the hovered row layout"
    );
    assert_eq!(
        cx.debug_bounds("sidebar-nav-hover-orders"),
        Some(orders_hover),
        "programmatic query should retain native hover without a pointer move"
    );

    cx.simulate_click(orders.center(), Modifiers::default());
    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::QueryChanged {
                id: "public-sidebar".into(),
                query: "workspace".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "orders".into(),
            },
        ]
    );

    let host = cx
        .debug_bounds("public-sidebar-host")
        .expect("sidebar host should remain rendered");
    cx.simulate_mouse_move(
        point(host.right() + px(20.), host.bottom() + px(20.)),
        None,
        Modifiers::default(),
    );
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let exited = cx
        .debug_bounds("sidebar-nav-hover-orders")
        .expect("native hover outline remains mounted after exit");
    assert!(exited.size.width < orders_hover.size.width);
}

#[gpui::test]
fn public_sidebar_nav_suppresses_disabled_selection_and_emits_collapse_identity(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let disabled = cx
        .debug_bounds("sidebar-nav-item-exports")
        .expect("disabled item should remain visible and named");
    assert!(cx.debug_bounds("sidebar-nav-filter").is_some());
    cx.simulate_click(disabled.center(), Modifiers::default());
    assert!(probe.read_with(cx, |probe, _| probe.events.borrow().is_empty()));

    let new_task = cx
        .debug_bounds("sidebar-nav-new-task")
        .expect("new-task control should render");
    cx.simulate_click(new_task.center(), Modifiers::default());

    let collapse = cx
        .debug_bounds("sidebar-nav-collapse")
        .expect("collapse control should render");
    cx.simulate_click(collapse.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-nav-item-overview").is_some());
    assert!(cx.debug_bounds("sidebar-nav-filter").is_none());
    assert!(cx.debug_bounds("sidebar-nav-new-task").is_none());

    let host = cx
        .debug_bounds("public-sidebar-host")
        .expect("the constrained sidebar host should remain available");
    let expand = cx
        .debug_bounds("sidebar-nav-collapse")
        .expect("collapsed navigation should expose one expand control");
    assert!(expand.left() >= host.left(), "{expand:?} vs {host:?}");
    assert!(expand.right() <= host.right(), "{expand:?} vs {host:?}");
    assert!(expand.size.width >= px(30.), "{expand:?}");

    // Calling focus_filter while collapsed must not move focus into its
    // unmounted input.
    cx.update(|window, cx| nav.update(cx, |nav, cx| nav.focus_filter(window, cx)));
    cx.simulate_keystrokes("risk");
    assert_eq!(nav.read_with(cx, |nav, _| nav.query().clone()), "");

    // Pointer completes the collapsed-to-expanded round trip.
    cx.simulate_click(expand.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-nav-filter").is_some());

    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::NewTaskRequested {
                id: "public-sidebar".into(),
            },
            SidebarNavEvent::CollapsedChanged {
                id: "public-sidebar".into(),
                collapsed: true,
            },
            SidebarNavEvent::CollapsedChanged {
                id: "public-sidebar".into(),
                collapsed: false,
            },
        ]
    );
}

#[gpui::test]
fn public_sidebar_nav_keyboard_expands_the_only_compact_header_control(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_collapsed(true, cx));
        window.draw(cx).clear(cx);
    });

    cx.update(|window, cx| {
        window.focus_next(cx);
        assert!(window.focused(cx).is_some());
        window.draw(cx).clear(cx);
    });
    activate_key(cx, "enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(!nav.read_with(cx, |nav, _| nav.is_collapsed()));
    assert!(cx.debug_bounds("sidebar-nav-filter").is_some());
    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::CollapsedChanged {
                id: "public-sidebar".into(),
                collapsed: true,
            },
            SidebarNavEvent::CollapsedChanged {
                id: "public-sidebar".into(),
                collapsed: false,
            },
        ]
    );
}

#[cfg_attr(
    target_os = "macos",
    ignore = "pinned GPUI TestWindow has no native macOS handle for focused InputState"
)]
#[gpui::test]
fn public_sidebar_nav_native_filter_typing_updates_query_and_emits_identity(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.focus_filter(window, cx));
    });
    cx.simulate_keystrokes("risk");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert_eq!(nav.read_with(cx, |nav, _| nav.query().clone()), "risk");
    assert!(cx.debug_bounds("sidebar-nav-item-supplier-risk").is_some());
    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().last().cloned()),
        Some(SidebarNavEvent::QueryChanged {
            id: "public-sidebar".into(),
            query: "risk".into(),
        })
    );
}

#[gpui::test]
fn public_sidebar_nav_programmatic_query_notifies_while_filter_is_unmounted(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_collapsed(true, cx));
        window.draw(cx).clear(cx);
    });
    probe.read_with(cx, |probe, _| probe.events.borrow_mut().clear());

    let notifications = Rc::new(Cell::new(0));
    let observed = notifications.clone();
    let _observation =
        cx.update(|_, cx| cx.observe(&nav, move |_, _| observed.set(observed.get() + 1)));
    cx.update(|window, cx| nav.update(cx, |nav, cx| nav.set_query("risk", window, cx)));

    assert_eq!(notifications.get(), 1);
    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [SidebarNavEvent::QueryChanged {
            id: "public-sidebar".into(),
            query: "risk".into(),
        }]
    );

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_collapsed(false, cx));
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("sidebar-nav-item-orders").is_some());
    assert!(cx.debug_bounds("sidebar-nav-item-supplier-risk").is_some());
    assert!(cx.debug_bounds("sidebar-nav-item-history").is_none());
    assert_eq!(
        probe.read_with(cx, |probe, _| {
            probe
                .events
                .borrow()
                .iter()
                .filter(|event| matches!(event, SidebarNavEvent::QueryChanged { .. }))
                .count()
        }),
        1
    );
}

#[gpui::test]
fn public_sidebar_nav_preserves_active_identity_after_controlled_reorder(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| {
            nav.set_sections(
                [
                    SidebarSection::new("reports", "Reports").items([
                        SidebarNavItem::new("archive-report", "Reports").icon(IconName::BookOpen),
                        SidebarNavItem::new("live-report", "Reports").icon(IconName::ChartPie),
                    ]),
                    sidebar_sections().remove(0),
                ],
                cx,
            )
        });
        window.draw(cx).clear(cx);
    });

    assert!(
        cx.debug_bounds("sidebar-nav-active-archive-report")
            .is_some()
    );
    assert!(cx.debug_bounds("sidebar-nav-active-live-report").is_none());
}

#[gpui::test]
fn public_sidebar_nav_keeps_controlled_active_descendants_reachable(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let orders = cx
        .debug_bounds("sidebar-nav-item-orders")
        .expect("parent route should render");
    cx.simulate_click(orders.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-nav-item-history").is_none());

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_active_item("supplier-risk", cx));
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("sidebar-nav-item-supplier-risk").is_some());
    assert!(
        cx.debug_bounds("sidebar-nav-active-supplier-risk")
            .is_some()
    );

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_collapsed(true, cx));
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("sidebar-nav-active-orders").is_some());
    assert!(cx.debug_bounds("sidebar-nav-item-supplier-risk").is_none());
}

#[gpui::test]
fn public_sidebar_nav_parent_activation_intentionally_selects_and_toggles(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let orders = cx
        .debug_bounds("sidebar-nav-item-orders")
        .expect("parent route should render");
    cx.simulate_click(orders.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-nav-item-history").is_none());
    cx.simulate_click(orders.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-nav-item-history").is_some());

    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "orders".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "orders".into(),
            },
        ]
    );
}

#[gpui::test]
fn public_sidebar_nav_distinguishes_empty_catalog_from_no_results(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("absent", window, cx));
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("sidebar-nav-no-results").is_some());
    assert!(cx.debug_bounds("sidebar-nav-empty").is_none());

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_sections([], cx));
        window.draw(cx).clear(cx);
    });
    assert!(cx.debug_bounds("sidebar-nav-no-results").is_none());
    assert!(cx.debug_bounds("sidebar-nav-empty").is_some());
}

#[gpui::test]
fn public_sidebar_nav_scrolls_the_final_stable_item_into_the_constrained_viewport(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (_, cx) = cx.add_window_view(OverflowSidebarProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let host = cx
        .debug_bounds("overflow-sidebar-host")
        .expect("constrained sidebar host should render");
    assert!(cx.debug_bounds("sidebar-nav-item-overflow-39").is_none());

    // A frame per wheel event, because the nav now virtualizes rows rather
    // than whole sections: each frame measures the window it drew, so the
    // reachable end of a forty-section list is discovered as the reader
    // scrolls toward it rather than known from the first frame.
    for _ in 0..36 {
        cx.simulate_event(ScrollWheelEvent {
            position: host.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-180.))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let final_item = cx
        .debug_bounds("sidebar-nav-item-overflow-39")
        .expect("the final stable item should enter the rendered range after scrolling");
    assert!(final_item.top() >= host.top(), "{final_item:?} vs {host:?}");
    assert!(
        final_item.bottom() <= host.bottom(),
        "{final_item:?} vs {host:?}"
    );
}

#[gpui::test]
fn public_sidebar_nav_tree_keyboard_walks_rows_honors_bounds_and_skips_unavailable(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    // Pointer activation is what puts a reader inside the tree: it moves the
    // roving row onto what it activated and focuses the tree itself, so the
    // arrow keys start from a row rather than from wherever the list rests.
    let overview = cx
        .debug_bounds("sidebar-nav-item-overview")
        .expect("a root item should render");
    assert!(
        cx.debug_bounds("sidebar-nav-item-exports").is_some(),
        "the unavailable row is rendered, so skipping it is a navigation claim"
    );
    cx.simulate_click(overview.center(), Modifiers::default());

    // End reaches the last visible row; Home reaches the section header, which
    // names its items and carries no application intent of its own.
    activate_key(cx, "end");
    activate_key(cx, "enter");
    activate_key(cx, "home");
    activate_key(cx, "enter");
    activate_key(cx, "down");
    activate_key(cx, "space");

    // Down to the last enabled descendant, then past the unavailable row into
    // the next section.
    for _ in 0..5 {
        activate_key(cx, "down");
    }
    activate_key(cx, "down");
    activate_key(cx, "down");
    activate_key(cx, "enter");

    // Up steps over the same unavailable row on the way back.
    activate_key(cx, "up");
    activate_key(cx, "up");
    activate_key(cx, "enter");

    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "overview".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "archive-report".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "overview".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "live-report".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "supplier-score".into(),
            },
        ]
    );
}

#[gpui::test]
fn public_sidebar_nav_tree_keyboard_expands_collapses_and_walks_parents(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let overview = cx
        .debug_bounds("sidebar-nav-item-overview")
        .expect("a root item should render");
    cx.simulate_click(overview.center(), Modifiers::default());
    activate_key(cx, "down");

    activate_key(cx, "left");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("sidebar-nav-item-history").is_none(),
        "Left collapses the expanded parent the reader is standing on"
    );

    activate_key(cx, "right");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("sidebar-nav-item-history").is_some(),
        "Right expands the collapsed parent the reader is standing on"
    );

    // A second Right enters the first child; Left from a leaf walks back out
    // to the parent that owns it.
    activate_key(cx, "right");
    activate_key(cx, "enter");
    activate_key(cx, "left");
    activate_key(cx, "enter");
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        cx.debug_bounds("sidebar-nav-item-history").is_none(),
        "activating the parent toggled it the way a click does"
    );

    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "overview".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "history".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "orders".into(),
            },
        ]
    );
}

#[gpui::test]
fn public_sidebar_nav_filter_reveals_matched_ancestry_and_restores_expansion(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let suppliers = cx
        .debug_bounds("sidebar-nav-item-suppliers")
        .expect("the nested parent should render");
    cx.simulate_click(suppliers.center(), Modifiers::default());
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-nav-item-supplier-risk").is_none());

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("risk", window, cx));
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("sidebar-nav-item-supplier-risk").is_some(),
        "a query exposes the ancestry it matched inside a collapsed parent"
    );
    assert!(cx.debug_bounds("sidebar-nav-item-history").is_none());

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("", window, cx));
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("sidebar-nav-item-supplier-risk").is_none(),
        "clearing the query restores the expansion the reader chose, not the one it revealed"
    );
    assert!(cx.debug_bounds("sidebar-nav-item-history").is_some());
}

#[gpui::test]
fn public_sidebar_nav_keyboard_focus_survives_a_filter_round_trip(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let history = cx
        .debug_bounds("sidebar-nav-item-history")
        .expect("the nested leaf should render");
    cx.simulate_click(history.center(), Modifiers::default());

    // Filtering rebuilds the rows around a smaller projection; the focused row
    // is named, not numbered, so it comes through both directions intact.
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("history", window, cx));
        window.draw(cx).clear(cx);
    });
    activate_key(cx, "enter");

    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("", window, cx));
        window.draw(cx).clear(cx);
    });
    activate_key(cx, "enter");

    let selections = probe.read_with(cx, |probe, _| {
        probe
            .events
            .borrow()
            .iter()
            .filter_map(|event| match event {
                SidebarNavEvent::Selected { item_id, .. } => Some(item_id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    });
    assert_eq!(selections, ["history", "history", "history"]);
}

#[gpui::test]
fn public_sidebar_nav_keyboard_focus_survives_a_controlled_reorder(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    let cx: &mut VisualTestContext = cx;
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let history = cx
        .debug_bounds("sidebar-nav-item-history")
        .expect("the nested leaf should render");
    cx.simulate_click(history.center(), Modifiers::default());

    // The same rows in a different order: focus is retained by stable ID, so
    // it stays on the row it was on rather than on the position it occupied.
    let mut reordered = sidebar_sections();
    reordered.reverse();
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_sections(reordered, cx));
        window.draw(cx).clear(cx);
    });
    assert!(
        cx.debug_bounds("sidebar-nav-active-archive-report")
            .is_some(),
        "the controlled active marker survives the reorder too"
    );

    activate_key(cx, "enter");
    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "history".into(),
            },
            SidebarNavEvent::Selected {
                id: "public-sidebar".into(),
                item_id: "history".into(),
            },
        ]
    );
}

#[gpui::test]
fn public_fine_tune_reset_and_apply_emit_stable_card_identity(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicFineTuneProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let reset = cx
        .debug_bounds("fine-tune-reset")
        .expect("reset should be a real rendered control");
    let apply = cx
        .debug_bounds("fine-tune-apply")
        .expect("apply should be a real rendered control");
    cx.simulate_click(reset.center(), Modifiers::default());
    cx.simulate_click(apply.center(), Modifiers::default());

    assert_eq!(
        probe.read_with(cx, |probe, _| probe.events.borrow().clone()),
        [
            FineTuneEvent::ResetRequested {
                id: "public-fine-tune".into(),
            },
            FineTuneEvent::ApplyRequested {
                id: "public-fine-tune".into(),
            },
        ]
    );
}

#[test]
fn fine_tune_presentation_uses_theme_typography_tokens() {
    let source = include_str!("../src/fine_tune.rs");
    let fixed_gpui_text_helper = [".text_", "sm()"].concat();

    assert!(
        !source.contains(&fixed_gpui_text_helper),
        "Fine-tune type styles must resolve through semantic theme tokens"
    );
}

#[gpui::test]
fn public_fine_tune_rendered_clear_accent_and_slider_keyboard_paths_emit_typed_events(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicFineTuneProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("fine-tune-typeface").is_some());
    let clear_accent = cx
        .debug_bounds("fine-tune-clear-accent")
        .expect("the populated accent should expose a named clear control");
    cx.simulate_click(clear_accent.center(), Modifiers::default());
    assert!(matches!(
        probe.read_with(cx, |probe, _| probe.events.borrow().last().cloned()),
        Some(FineTuneEvent::AccentChanged { id, accent: None })
            if id == "public-fine-tune"
    ));

    let slider = cx
        .debug_bounds("fine-tune-opacity-slider")
        .expect("the named opacity slider should render");
    cx.simulate_click(slider.center(), Modifiers::default());
    probe.update(cx, |probe, _| probe.events.borrow_mut().clear());
    cx.simulate_keystrokes("right");
    cx.run_until_parked();

    let events = probe.read_with(cx, |probe, _| probe.events.borrow().clone());
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        FineTuneEvent::OpacityChanged { id, opacity }
            if id == "public-fine-tune" && *opacity > 0.5
    ));
}

#[gpui::test]
fn public_fine_tune_empty_typeface_catalog_cannot_open_a_popup(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicFineTuneProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| {
        probe.update(cx, |probe, cx| {
            probe.card.update(cx, |card, cx| card.set_typefaces([], cx));
        });
        window.draw(cx).clear(cx);
    });

    let typeface = cx
        .debug_bounds("fine-tune-typeface")
        .expect("the empty typeface state should remain visible");
    assert!(cx.debug_bounds("popup-content").is_none());
    cx.simulate_click(typeface.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("popup-content").is_none());
    assert!(probe.read_with(cx, |probe, _| probe.events.borrow().is_empty()));
}

#[gpui::test]
fn public_fine_tune_keeps_controls_inside_a_narrow_card(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (_, cx) = cx.add_window_view(NarrowFineTuneProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let host = cx
        .debug_bounds("narrow-fine-tune-host")
        .expect("the narrow Fine-tune host should render");
    for selector in [
        "fine-tune-clear-accent",
        "fine-tune-reset",
        "fine-tune-apply",
    ] {
        let control = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should render"));
        assert!(control.left() >= host.left(), "{selector}: {control:?}");
        assert!(control.right() <= host.right(), "{selector}: {control:?}");
    }
}

#[gpui::test]
fn public_fine_tune_keeps_apply_reachable_in_a_constrained_viewport(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (_, cx) = cx.add_window_view(ConstrainedFineTuneProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let host = cx
        .debug_bounds("constrained-fine-tune-host")
        .expect("constrained FineTune host should render");
    let initial_apply = cx
        .debug_bounds("fine-tune-apply")
        .expect("Apply should remain laid out below the fold");
    assert!(initial_apply.bottom() > host.bottom());

    for _ in 0..8 {
        cx.simulate_event(ScrollWheelEvent {
            position: host.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-120.))),
            ..Default::default()
        });
    }
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let final_apply = cx
        .debug_bounds("fine-tune-apply")
        .expect("Apply should remain rendered after scrolling");
    assert!(
        final_apply.top() >= host.top(),
        "{final_apply:?} vs {host:?}"
    );
    assert!(
        final_apply.bottom() <= host.bottom(),
        "{final_apply:?} vs {host:?}"
    );
}

#[gpui::test]
fn public_chat_keeps_typed_retry_and_keyboard_composer_paths(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicChatProbe::new);
    let cx: &mut VisualTestContext = cx;
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("chat-transcript").is_some());
    assert!(cx.debug_bounds("chat-message-failed-answer").is_some());
    assert!(cx.debug_bounds("chat-retry-failed-answer").is_some());
    let retry = cx
        .debug_bounds("chat-retry-failed-answer")
        .expect("retry action should remain reachable");
    cx.simulate_click(retry.center(), Modifiers::default());

    #[cfg(not(target_os = "macos"))]
    {
        let chat = probe.read_with(cx, |probe, _| probe.chat.clone());
        let prompt = chat.read_with(cx, |chat, _| chat.prompt_bar().clone());
        cx.update(|window, cx| {
            prompt.update(cx, |prompt, cx| {
                prompt.set_draft("Continue from chat", window, cx);
                prompt.focus(window, cx);
            });
        });
        cx.simulate_keystrokes("enter");
    }

    probe.read_with(cx, |probe, _| {
        let events = probe.events.borrow();
        assert!(events.iter().any(|event| matches!(
            event,
            ChatEvent::RetryRequested { message_id } if message_id == "failed-answer"
        )));
        #[cfg(not(target_os = "macos"))]
        assert!(events.iter().any(|event| matches!(
            event,
            ChatEvent::Prompt(PromptBarEvent::Submit { submission, .. })
                if submission.text() == "Continue from chat"
        )));
    });
}

#[gpui::test]
fn public_selection_actions_preserve_selection_and_activate_typed_events(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let probe = cx.new(|cx| PublicSelectionProbe::new(window, cx));
        SelectionTestRoot::new(probe)
    });
    let probe = root.read_with(cx, |root, _| root.view.clone());
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let surface = cx
        .debug_bounds("selection-actions-surface")
        .expect("selection surface should render");
    let from = point(surface.left() + px(14.), surface.top() + px(14.));
    let to = point(surface.right() - px(14.), surface.top() + px(24.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    assert!(
        cx.debug_bounds("selection-actions-toolbar").is_none(),
        "toolbar must stay hidden while selection drag is active"
    );
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let selected = probe.read_with(cx, |probe, cx| {
        probe.selection.read(cx).selected_text().to_string()
    });
    assert!(selected.contains("Selectable action words"), "{selected:?}");
    let ask = cx
        .debug_bounds("selection-action-ask")
        .expect("settled selection should expose Ask");
    cx.simulate_mouse_down(ask.center(), MouseButton::Left, Modifiers::default());
    assert!(probe.read_with(cx, |probe, cx| {
        !probe.selection.read(cx).selected_text().is_empty()
    }));
    cx.simulate_mouse_up(ask.center(), MouseButton::Left, Modifiers::default());
    assert!(probe.read_with(cx, |probe, _| {
        let events = probe.events.borrow();
        let matched = events.iter().any(|event| {
            matches!(
                event,
                SelectionActionsEvent::Invoked {
                    id,
                    action_id,
                    selected_text,
                } if id == "public-selection"
                    && action_id == "ask"
                    && selected_text.contains("Selectable action words")
            )
        });
        assert!(matched, "unexpected selection events: {events:?}");
        true
    }));

    probe.update(cx, |probe, cx| {
        probe.selection.update(cx, |selection, cx| {
            selection.set_markdown("Replacement content", cx)
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("selection-actions-toolbar").is_none());
    assert!(probe.read_with(cx, |probe, cx| {
        probe.selection.read(cx).selected_text().is_empty()
    }));
}

#[gpui::test]
fn public_selection_actions_follow_keyboard_select_all_and_copy(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let probe = cx.new(|cx| PublicSelectionProbe::new(window, cx));
        SelectionTestRoot::new(probe)
    });
    let probe = root.read_with(cx, |root, _| root.view.clone());
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let surface = cx
        .debug_bounds("selection-actions-surface")
        .expect("selection surface should render");

    let focus_from = point(surface.left() + px(14.), surface.top() + px(14.));
    let focus_to = point(surface.left() + px(42.), surface.top() + px(14.));
    cx.simulate_mouse_down(focus_from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(focus_to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(focus_to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    #[cfg(target_os = "macos")]
    cx.simulate_keystrokes("cmd-a");
    #[cfg(not(target_os = "macos"))]
    cx.simulate_keystrokes("ctrl-a");
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert_eq!(
        probe.read_with(cx, |probe, cx| probe
            .selection
            .read(cx)
            .selected_text()
            .to_string()),
        "Selectable action words for testing."
    );
    assert!(cx.debug_bounds("selection-actions-toolbar").is_some());

    #[cfg(target_os = "macos")]
    cx.simulate_keystrokes("cmd-c");
    #[cfg(not(target_os = "macos"))]
    cx.simulate_keystrokes("ctrl-c");
    let clipboard = cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()));
    assert_eq!(
        clipboard.as_deref(),
        Some("Selectable action words for testing.")
    );
}

#[gpui::test]
fn public_selection_actions_clear_after_an_outside_left_click(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let probe = cx.new(|cx| BoundedSelectionProbe::new(window, cx));
        SelectionTestRoot::new(probe)
    });
    let probe = root.read_with(cx, |root, _| root.view.clone());
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let surface = cx
        .debug_bounds("selection-actions-surface")
        .expect("selection surface should render");
    let from = point(surface.left() + px(14.), surface.top() + px(14.));
    let to = point(surface.right() - px(14.), surface.top() + px(24.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("selection-actions-toolbar").is_some());

    let outside = cx
        .debug_bounds("selection-actions-outside-target")
        .expect("outside target should render");
    cx.simulate_click(outside.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("selection-actions-toolbar").is_none());
    assert!(probe.read_with(cx, |probe, cx| {
        probe.selection.read(cx).selected_text().is_empty()
    }));
}

#[gpui::test]
fn public_selection_actions_follow_native_empty_selection(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let probe = cx.new(|cx| BoundedSelectionProbe::new(window, cx));
        SelectionTestRoot::new(probe)
    });
    let probe = root.read_with(cx, |root, _| root.view.clone());
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let surface = cx
        .debug_bounds("selection-actions-surface")
        .expect("selection surface should render");
    let from = point(surface.left() + px(14.), surface.top() + px(14.));
    let to = point(surface.right() - px(14.), surface.top() + px(24.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("selection-actions-toolbar").is_some());

    cx.update(gpui_base::TextSelection::clear);
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(cx.debug_bounds("selection-actions-toolbar").is_none());
    assert!(probe.read_with(cx, |probe, cx| {
        probe.selection.read(cx).selected_text().is_empty()
    }));
}

#[gpui::test]
fn public_selection_actions_settle_when_a_drag_releases_outside(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (root, cx) = cx.add_window_view(|window, cx| {
        let probe = cx.new(|cx| BoundedSelectionProbe::new(window, cx));
        SelectionTestRoot::new(probe)
    });
    let probe = root.read_with(cx, |root, _| root.view.clone());
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let surface = cx
        .debug_bounds("selection-actions-surface")
        .expect("selection surface should render");
    let outside = cx
        .debug_bounds("selection-actions-outside-target")
        .expect("outside target should render");
    let from = point(surface.left() + px(14.), surface.top() + px(14.));
    let release = point(surface.right() - px(8.), outside.top() + px(12.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(release, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(release, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    assert!(probe.read_with(cx, |probe, cx| {
        !probe.selection.read(cx).selected_text().is_empty()
    }));
    assert!(cx.debug_bounds("selection-actions-toolbar").is_some());
}

#[gpui::test]
fn public_selection_actions_keep_long_final_action_reachable_in_a_narrow_root(
    cx: &mut TestAppContext,
) {
    cx.update(gpui_ai::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let probe = cx.new(|cx| BoundedSelectionProbe::new(window, cx));
        SelectionTestRoot::new(probe)
    });
    let cx: &mut VisualTestContext = cx;
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let surface = cx
        .debug_bounds("selection-actions-surface")
        .expect("selection surface should render");
    let from = point(surface.left() + px(14.), surface.top() + px(14.));
    let to = point(surface.right() - px(14.), surface.top() + px(24.));
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Modifiers::default());
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let toolbar = cx
        .debug_bounds("selection-actions-toolbar")
        .expect("toolbar should render after selection");
    // The toolbar is placed by the upstream positioner, whose containment
    // boundary is the viewport rather than the narrow root the selection
    // lives in: a toolbar wider than its host clamps on-screen instead of
    // clipping inside it. Reachability is asserted against the window and,
    // below, against the toolbar's own scrollable frame.
    let viewport = cx.update(|window, _| window.viewport_size());
    assert!(toolbar.left() >= px(0.), "{toolbar:?}");
    assert!(
        toolbar.right() <= viewport.width,
        "{toolbar:?} vs {viewport:?}"
    );

    for _ in 0..12 {
        cx.simulate_event(ScrollWheelEvent {
            position: toolbar.center(),
            delta: ScrollDelta::Pixels(point(px(-120.), px(0.))),
            ..Default::default()
        });
    }
    cx.update(|window, cx| window.draw(cx).clear(cx));

    let final_action = cx
        .debug_bounds("selection-action-final")
        .expect("final action should remain rendered after horizontal scrolling");
    assert!(
        final_action.left() >= toolbar.left() && final_action.right() <= toolbar.right(),
        "{final_action:?} vs {toolbar:?}"
    );
    assert!(
        final_action.left() >= px(0.) && final_action.right() <= viewport.width,
        "{final_action:?} vs {viewport:?}"
    );
}

#[gpui::test]
fn sidebar_filter_keeps_one_line_height_under_an_overlong_query(cx: &mut TestAppContext) {
    cx.update(gpui_ai::init);
    let (probe, cx) = cx.add_window_view(PublicSidebarNavProbe::new);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
    let one_line = cx
        .debug_bounds("sidebar-nav-filter")
        .expect("the filter field should render")
        .size
        .height;
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| {
            nav.set_query("wholesale scorecards ".repeat(24), window, cx);
        });
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    // A single-line filter holds one line of height however long the query
    // grows; upstream reserves its editor scrollbar for multi-line input.
    let overlong = cx
        .debug_bounds("sidebar-nav-filter")
        .expect("the filter field should survive an overlong query")
        .size
        .height;
    assert_eq!(one_line, overlong);
    cx.update(|window, cx| {
        nav.update(cx, |nav, cx| nav.set_query("", window, cx));
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("sidebar-nav-item-orders").is_some());
}
