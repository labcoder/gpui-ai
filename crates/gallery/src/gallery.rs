//! Native component gallery for gpui-ai.
//!
//! One story per component, driven by simulated agent activity (fake token
//! streams, task lifecycles). All simulation lives here — the library
//! components only ever see data.

use crate::{StoryId, sim};

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_ai::prelude::*;
use gpui_component::{
    ActiveTheme as _, IconName, Root, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    text::TextView,
    theme::{Theme, ThemeMode, ThemeRegistry},
    v_flex,
};

// Catalog keyboard actions: page and jump navigation for the story feed.
actions!(
    catalog,
    [PageUp, PageDown, ScrollHome, ScrollEnd, CancelAutoscroll]
);

/// Distance scrolled by one Page Up/Down, as a fraction of one story row.
const PAGE_FRACTION: f32 = 3.0;
use std::{collections::HashMap, ops::Range, sync::Arc, time::Duration};

const CONTRAST_THEME: &str = r##"{
  "name": "gpui-ai gallery themes",
  "themes": [{
    "name": "gpui-ai Contrast",
    "mode": "dark",
    "radius": 8,
    "radius.lg": 12,
    "shadow": true,
    "colors": {
      "background": "#050505",
      "foreground": "#ffffff",
      "border": "#525252",
      "ring": "#facc15",
      "primary": "#facc15",
      "primary.foreground": "#171717",
      "secondary": "#262626",
      "secondary.foreground": "#ffffff",
      "muted": "#262626",
      "muted.foreground": "#d4d4d4",
      "accent.background": "#404040",
      "accent.foreground": "#ffffff",
      "danger": "#fb7185",
      "danger.foreground": "#171717",
      "info": "#67e8f9",
      "success": "#86efac",
      "warning": "#fde047"
    }
  }]
}"##;

const CONTRAST_THEME_NAME: &str = "gpui-ai Contrast";

/// Curated showcase themes, embedded as the same JSON the website shares.
/// Keeping this file in-repo means the downloadable theme pack and the
/// gallery demo can never drift apart.
const SHOWCASE_THEMES_JSON: &str = include_str!("../themes/showcase-themes.json");

fn story_list_frame() -> Stateful<Div> {
    div()
        .id("gallery-story-list")
        .accessibility_id("gallery.story-list")
        .role(Role::List)
        .aria_label("Component stories")
}

fn story_frame(story: StoryId, in_catalog: bool) -> Stateful<Div> {
    let frame = v_flex()
        .id(format!("gallery-story-{}", story.slug()))
        .accessibility_id(format!("gallery.story.{}", story.slug()))
        .aria_label(story.title());
    if in_catalog {
        frame.role(Role::ListItem)
    } else {
        frame.role(Role::Group)
    }
}

fn story_needs_simulation(story: StoryId) -> bool {
    matches!(
        story,
        StoryId::Loading
            | StoryId::Tasks
            | StoryId::Thinking
            | StoryId::Search
            | StoryId::ImageGeneration
            | StoryId::StreamingText
            | StoryId::Chat
            | StoryId::CodeBlock
            | StoryId::ToolCalls
    )
}

/// One demonstrable state of a seven-state table story.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableStoryState {
    Populated,
    Loading,
    Error,
    Empty,
    Disabled,
    Selected,
    Constrained,
}

impl TableStoryState {
    /// Every demonstrated state in switcher order.
    const ALL: [Self; 7] = [
        Self::Populated,
        Self::Loading,
        Self::Error,
        Self::Empty,
        Self::Disabled,
        Self::Selected,
        Self::Constrained,
    ];

    /// Switcher labels parallel to [`Self::ALL`].
    const LABELS: &'static [(&'static str, &'static str)] = &[
        ("populated", "Populated"),
        ("loading", "Loading"),
        ("error", "Error"),
        ("empty", "Empty"),
        ("disabled", "Disabled"),
        ("selected", "Selected"),
        ("constrained", "Constrained"),
    ];

    /// Position of this state in [`Self::ALL`] and [`Self::LABELS`].
    fn index(self) -> usize {
        Self::ALL.iter().position(|kind| *kind == self).unwrap_or(0)
    }
}

/// Builds the shared state-switcher toolbar for a multi-state story.
///
/// Only the active state's content is rendered; switching notifies the owning
/// story entity through `apply`.
fn story_state_switcher<T: 'static>(
    owner: gpui::WeakEntity<T>,
    slug: &'static str,
    states: &'static [(&'static str, &'static str)],
    active_index: usize,
    apply: fn(&mut T, usize, &mut Context<T>),
) -> Stateful<Div> {
    h_flex()
        .id(format!("{slug}-state-switcher"))
        .debug_selector(move || format!("{slug}-state-switcher"))
        .flex_none()
        .flex_wrap()
        .gap_1()
        .role(Role::Toolbar)
        .aria_label("Demonstrated state")
        .children(
            states
                .iter()
                .enumerate()
                .map(|(index, (state_slug, label))| {
                    let owner = owner.clone();
                    let is_active = index == active_index;
                    div()
                        .debug_selector(move || format!("{slug}-state-{state_slug}"))
                        .child(
                            Button::new(format!("{slug}-state-{state_slug}"))
                                .when(is_active, |button| button.primary())
                                .when(!is_active, |button| button.outline())
                                .label(*label)
                                .on_click(move |_, _, cx| {
                                    let _ = owner.update(cx, |story, cx| apply(story, index, cx));
                                }),
                        )
                }),
        )
}

fn visible_range_needs_simulation(range: Range<usize>) -> bool {
    StoryId::ALL
        .get(range)
        .is_some_and(|stories| stories.iter().copied().any(story_needs_simulation))
}

fn story_changed_by_delta(story: StoryId, delta: sim::SimulationDelta) -> bool {
    match story {
        StoryId::Loading | StoryId::Tasks => true,
        StoryId::Thinking | StoryId::Search | StoryId::ToolCalls => delta.answer_phase_changed(),
        StoryId::ImageGeneration | StoryId::StreamingText | StoryId::Chat => {
            delta.answer_content_changed()
        }
        StoryId::CodeBlock => delta.code_content_changed() || delta.code_phase_changed(),
        StoryId::All
        | StoryId::Suggestions
        | StoryId::ToolChips
        | StoryId::Orbs
        | StoryId::Todos
        | StoryId::Approval
        | StoryId::Recommendation
        | StoryId::Context
        | StoryId::Insights
        | StoryId::CommandSearch
        | StoryId::SidebarNav
        | StoryId::FineTune
        | StoryId::RecordsTable
        | StoryId::DiffTable
        | StoryId::FilterTable
        | StoryId::ComparisonTable
        | StoryId::PromptBar
        | StoryId::SelectionActions => false,
    }
}

struct ChatStory {
    chat: Entity<Chat>,
    answer: Option<StreamedContent>,
    last_event: SharedString,
    show_welcome: bool,
    _subscription: Subscription,
}

/// Switcher labels for the two demonstrated chat states.
const CHAT_STORY_STATES: &[(&str, &str)] =
    &[("conversation", "Conversation"), ("welcome", "Welcome")];

impl ChatStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prompt = cx.new(|cx| {
            let mut prompt = PromptBar::new("gallery-chat-prompt", window, cx);
            prompt.set_models(
                [
                    PromptModel::new("balanced", "Balanced"),
                    PromptModel::new("fast", "Fast"),
                ],
                cx,
            );
            prompt.set_selected_model("balanced", cx);
            prompt.set_mentions([PromptMention::new("suppliers", "Suppliers")], cx);
            prompt.set_commands(
                [PromptCommand::new("compare", "compare")
                    .description("Compare current supplier context")],
                cx,
            );
            prompt.set_attachments([PromptAttachment::new("pricing", "pricing.md")], cx);
            prompt.set_draft("Ask a follow-up about suppliers", window, cx);
            prompt
        });
        let chat = cx.new(|cx| {
            let mut chat = Chat::new("gallery-chat", prompt, window, cx);
            chat.set_welcome(
                Some(
                    ChatWelcome::new("What should we look into?")
                        .description(
                            "Ask about suppliers, pricing, or delivery risk. Suggestions send \
                             immediately; the composer stays yours.",
                        )
                        .suggestions([
                            Suggestion::new("compare", "Compare supplier prices"),
                            Suggestion::new("risk", "Explain this week's delivery risk"),
                            Suggestion::new("draft", "Draft the order confirmations"),
                        ]),
                ),
                cx,
            );
            chat
        });
        let subscription = cx.subscribe_in(
            &chat,
            window,
            |this, chat, event: &ChatEvent, window, cx| {
                this.last_event = format!("{event:?}").into();
                let prompt = chat.read(cx).prompt_bar().clone();
                match event {
                    ChatEvent::SuggestionSelected { suggestion_id } => {
                        let draft = match suggestion_id.as_ref() {
                            "compare" => "Compare supplier prices",
                            "risk" => "Explain this week's delivery risk",
                            _ => "Draft the order confirmations",
                        };
                        prompt.update(cx, |prompt, cx| {
                            prompt.set_draft(draft, window, cx);
                        });
                        this.show_welcome = false;
                    }
                    ChatEvent::Prompt(PromptBarEvent::ModelChanged { model_id, .. }) => {
                        prompt.update(cx, |prompt, cx| {
                            prompt.set_selected_model(model_id.clone(), cx);
                        });
                    }
                    ChatEvent::Prompt(PromptBarEvent::Submit { .. }) => {
                        prompt.update(cx, |prompt, cx| {
                            prompt.set_progress(ProgressState::Running, cx);
                        });
                    }
                    ChatEvent::Prompt(PromptBarEvent::CancelRequested { .. }) => {
                        prompt.update(cx, |prompt, cx| {
                            prompt.set_progress(ProgressState::Complete, cx);
                        });
                    }
                    ChatEvent::Prompt(_)
                    | ChatEvent::RetryRequested { .. }
                    | ChatEvent::FollowUpSelected { .. }
                    | ChatEvent::CitationActivated { .. }
                    | ChatEvent::MessageCopied { .. }
                    | ChatEvent::RegenerateRequested { .. }
                    | ChatEvent::EditRequested { .. }
                    | ChatEvent::FeedbackSubmitted { .. }
                    | ChatEvent::JumpedToLatest => {}
                }
                cx.notify();
            },
        );
        Self {
            chat,
            answer: None,
            last_event: "Hover a message for actions, or try Retry, a citation, or the composer."
                .into(),
            show_welcome: false,
            _subscription: subscription,
        }
    }

    fn set_state(&mut self, index: usize, cx: &mut Context<Self>) {
        let show_welcome = index == 1;
        if self.show_welcome != show_welcome {
            self.show_welcome = show_welcome;
            // Force the next simulation snapshot to rebuild the transcript.
            self.answer = None;
            cx.notify();
        }
    }

    fn set_answer(
        &mut self,
        answer: StreamedContent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.answer.as_ref() == Some(&answer) {
            return false;
        }
        if self.show_welcome {
            self.chat.update(cx, |chat, cx| {
                chat.set_messages(Arc::from([]), window, cx);
            });
            self.answer = Some(answer);
            return true;
        }
        let live_text = format!(
            "The current supplier comparison follows [[cite:pricing]].\n\n{}",
            answer.text()
        );
        let live_answer = match answer.state() {
            ProgressState::Pending => StreamedContent::pending(live_text),
            ProgressState::Running => StreamedContent::running(live_text),
            ProgressState::Complete => StreamedContent::complete(live_text),
            ProgressState::Failed(reason) => StreamedContent::failed(live_text, reason.clone()),
        };
        let mut messages = vec![ChatMessage::new(
            "reference-note",
            ChatRole::System,
            StreamedContent::done(
                "Beautiful UI keeps Chat compact and bottom-pinned. This original GPUI composition additionally virtualizes by stable application IDs, preserves the top item and pixel offset while reading history, exposes unread state, and delegates every async producer to the application.",
            ),
        )
        .with_appearance(ChatMessageAppearance::new(
            MessageAlignment::Leading,
            MessageBubble::Plain,
        ))];
        for index in 0..18 {
            let role = if index % 2 == 0 {
                ChatRole::User
            } else {
                ChatRole::Assistant
            };
            // Role-driven presentation: user messages trail in a filled
            // bubble, assistant replies lead unframed — the composition the
            // reference demos use. Applications choose per message.
            let appearance = match role {
                ChatRole::User => {
                    ChatMessageAppearance::new(MessageAlignment::Trailing, MessageBubble::Filled)
                }
                _ => ChatMessageAppearance::new(MessageAlignment::Leading, MessageBubble::Plain),
            };
            messages.push(
                ChatMessage::new(
                    format!("history-{index}"),
                    role,
                    StreamedContent::done(format!(
                        "Historical message {index}: compare unit price, delivery window, and inventory risk."
                    )),
                )
                .with_appearance(appearance),
            );
        }
        messages.extend([
            ChatMessage::new(
                "tool-result",
                ChatRole::Tool,
                StreamedContent::done("Loaded pricing.md and suppliers.csv."),
            )
            .author("Catalog lookup"),
            ChatMessage::new(
                "failed-comparison",
                ChatRole::Assistant,
                StreamedContent::failed(
                    "The first comparison stopped after two suppliers.".to_owned(),
                    "Supplier service unavailable",
                ),
            )
            .retryable(true),
            ChatMessage::new(
                "latest-question",
                ChatRole::User,
                StreamedContent::done("Which supplier is the safest choice this week?"),
            )
            .with_appearance(ChatMessageAppearance::new(
                MessageAlignment::Trailing,
                MessageBubble::Filled,
            )),
            ChatMessage::new("live-answer", ChatRole::Assistant, live_answer)
                .with_appearance(ChatMessageAppearance::new(
                    MessageAlignment::Leading,
                    MessageBubble::Plain,
                ))
                .citations([CitationRef::new(
                    "pricing",
                    "Pricing report",
                    "Open the supplier pricing report",
                    "app://reports/supplier-pricing",
                )])
                .sources(["pricing.md", "suppliers.csv"])
                .follow_ups([
                    FollowUp::new("delivery", "Compare delivery windows"),
                    FollowUp::new("risk", "Explain inventory risk"),
                ]),
        ]);
        self.chat.update(cx, |chat, cx| {
            chat.set_messages(Arc::from(messages), window, cx);
        });
        self.answer = Some(answer);
        true
    }
}

impl Render for ChatStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        // A chat panel needs vertical room to show a real conversation —
        // transcript history, tool results, the streaming answer, and the
        // composer. 232px crushed all of that into ~1.5 visible messages;
        // 480px shows the full story arc the way the reference demos do.
        v_flex()
            .gap(tokens.spacing.xs)
            .child(story_state_switcher(
                cx.weak_entity(),
                "chat",
                CHAT_STORY_STATES,
                usize::from(self.show_welcome),
                Self::set_state,
            ))
            .child(
                div()
                    .id("chat-story-host")
                    .debug_selector(|| "chat-story-host".into())
                    .h(px(480.))
                    .max_h(px(480.))
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(tokens.radius.lg)
                    .overflow_hidden()
                    .bg(tokens.colors.surface)
                    .child(self.chat.clone()),
            )
            .child(
                div()
                    .id("chat-story-event")
                    .role(Role::Status)
                    .aria_label(format!("Last chat event: {}", self.last_event))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
            )
    }
}

struct CommandSearchStory {
    ready: Entity<CommandSearch>,
    empty: Entity<CommandSearch>,
    no_results: Entity<CommandSearch>,
    last_event: SharedString,
    _subscription: Subscription,
}

impl CommandSearchStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ready = cx.new(|cx| CommandSearch::new("gallery-command-ready", window, cx));
        ready.update(cx, |search, cx| {
            search.set_items(
                [
                    CommandSearchItem::new("supplier-report", "Open report")
                        .subtitle("Supplier pricing and margin summary")
                        .keywords(["cost", "margin"])
                        .shortcut("Ctrl+R"),
                    CommandSearchItem::new("delivery-calendar", "Open delivery calendar")
                        .subtitle("Review upcoming supplier windows")
                        .keywords(["schedule", "dates"])
                        .shortcut("Ctrl+D"),
                    CommandSearchItem::new("duplicate-report", "Open report")
                        .subtitle("Inventory risk summary")
                        .keywords(["stock", "risk"]),
                    CommandSearchItem::new("offline-sync", "Sync offline catalog")
                        .subtitle("Unavailable while disconnected")
                        .disabled(true),
                ],
                window,
                cx,
            );
        });
        let empty = cx.new(|cx| CommandSearch::new("gallery-command-empty", window, cx));
        let no_results = cx.new(|cx| CommandSearch::new("gallery-command-no-results", window, cx));
        no_results.update(cx, |search, cx| {
            search.set_items(
                [CommandSearchItem::new("available", "Available command")],
                window,
                cx,
            );
            search.set_query("no matching command", window, cx);
        });

        let _subscription = cx.subscribe(&ready, |this, _, event: &CommandSearchEvent, cx| {
            this.last_event = format!("{event:?}").into();
            cx.notify();
        });
        Self {
            ready,
            empty,
            no_results,
            last_event: "Type, use arrow keys, press Enter, or choose a row.".into(),
            _subscription,
        }
    }
}

impl Render for CommandSearchStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        v_flex()
            .gap(tokens.spacing.md)
            .child(
                div()
                    .id("command-search-ready-heading")
                    .role(Role::Heading)
                    .aria_label("Populated command search")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Populated — type margin, delivery, or risk"),
            )
            .child(
                div()
                    .id("command-search-ready-host")
                    .debug_selector(|| "command-search-ready-host".into())
                    .h(px(248.))
                    .max_h(px(248.))
                    .flex_none()
                    .child(self.ready.clone()),
            )
            .child(
                div()
                    .id("command-search-event")
                    .role(Role::Status)
                    .aria_label(format!("Last command-search event: {}", self.last_event))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
            )
            .child(
                h_flex()
                    .items_start()
                    .gap(tokens.spacing.md)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(tokens.spacing.xs)
                            .child(
                                div()
                                    .id("command-search-empty-heading")
                                    .role(Role::Heading)
                                    .aria_label("Empty command catalog")
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("Empty catalog"),
                            )
                            .child(self.empty.clone()),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(tokens.spacing.xs)
                            .child(
                                div()
                                    .id("command-search-no-results-heading")
                                    .role(Role::Heading)
                                    .aria_label("No command results")
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("No results"),
                            )
                            .child(self.no_results.clone()),
                    ),
            )
            .child(
                TextView::markdown(
                    "command-search-reference-note",
                    "**Reference comparison.** Beautiful UI presents a compact search-and-command list. This original GPUI adapter preserves the upstream native editor, filtering, keyboard navigation, focus, and virtual list while adding stable application IDs, typed query/selection/dismissal events, subtitles, shortcut hints, disabled-state semantics, and controlled snapshot replacement.",
                )
                .selectable(true),
            )
    }
}

fn creamery_sidebar_sections() -> [SidebarSection; 3] {
    [
        SidebarSection::new("production", "Production").items([
            SidebarNavItem::new("overview", "Overview").icon(IconName::LayoutDashboard),
            SidebarNavItem::new("orders", "Orders")
                .icon(IconName::ChartPie)
                .badge("12")
                .children([
                    SidebarNavItem::new("all-orders", "All orders"),
                    SidebarNavItem::new("wholesale", "Wholesale"),
                    SidebarNavItem::new("farm-shop", "Farm shop"),
                ]),
            SidebarNavItem::new("inventory", "Inventory")
                .icon(IconName::BookOpen)
                .children([
                    SidebarNavItem::new("milk-cream", "Milk & cream"),
                    SidebarNavItem::new("flavors", "Flavors").children([
                        SidebarNavItem::new("pistachio", "Pistachio reserve").badge("Low"),
                        SidebarNavItem::new("berry", "Summer berry"),
                    ]),
                    SidebarNavItem::new("packaging", "Packaging"),
                ]),
            SidebarNavItem::new("seasonal", "Seasonal forecast")
                .icon(IconName::SquareTerminal)
                .disabled(true),
        ]),
        SidebarSection::new("trade", "Trade").items([
            SidebarNavItem::new("accounts", "Stockists").icon(IconName::BookOpen),
            SidebarNavItem::new("promotions", "Promotions")
                .icon(IconName::ChartPie)
                .badge("New"),
        ]),
        SidebarSection::new("reports", "Reports").items([
            SidebarNavItem::new("daily-report", "Reports").icon(IconName::SquareTerminal),
            SidebarNavItem::new("archive-report", "Reports")
                .icon(IconName::SquareTerminal)
                .children([SidebarNavItem::new("supplier-risk", "Supplier risk")]),
            SidebarNavItem::new("last-item", "Cold-chain audit").icon(IconName::BookOpen),
        ]),
    ]
}

struct SidebarNavStory {
    expanded: Entity<SidebarNav>,
    collapsed: Entity<SidebarNav>,
    filtered: Entity<SidebarNav>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl SidebarNavStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let expanded = cx.new(|cx| SidebarNav::new("creamery-expanded", window, cx));
        let collapsed = cx.new(|cx| SidebarNav::new("creamery-collapsed", window, cx));
        let filtered = cx.new(|cx| SidebarNav::new("creamery-filtered", window, cx));
        for nav in [&expanded, &collapsed, &filtered] {
            nav.update(cx, |nav, cx| {
                nav.set_sections(creamery_sidebar_sections(), cx);
                nav.set_active_item("all-orders", cx);
            });
        }
        collapsed.update(cx, |nav, cx| nav.set_collapsed(true, cx));
        filtered.update(cx, |nav, cx| nav.set_query("pistachio", window, cx));

        let subscriptions = [&expanded, &collapsed, &filtered]
            .into_iter()
            .map(|nav| {
                cx.subscribe(nav, |this, nav, event: &SidebarNavEvent, cx| {
                    if let SidebarNavEvent::Selected { item_id, .. } = event {
                        nav.update(cx, |nav, cx| nav.set_active_item(item_id.clone(), cx));
                    }
                    this.last_event = format!("{event:?}").into();
                    cx.notify();
                })
            })
            .collect();

        Self {
            expanded,
            collapsed,
            filtered,
            last_event: "Choose a row, filter, collapse, or start a new task.".into(),
            _subscriptions: subscriptions,
        }
    }
}

impl Render for SidebarNavStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        v_flex()
            .gap(tokens.spacing.sm)
            .child(
                h_flex()
                    .items_start()
                    .gap(tokens.spacing.sm)
                    .child(
                        div()
                            .id("sidebar-nav-expanded-host")
                            .debug_selector(|| "sidebar-nav-expanded-host".into())
                            .w(tokens.spacing.xxl * 8.)
                            .h(tokens.spacing.xxl * 7.)
                            .child(self.expanded.clone()),
                    )
                    .child(
                        div()
                            .id("sidebar-nav-collapsed-host")
                            .debug_selector(|| "sidebar-nav-collapsed-host".into())
                            .h(tokens.spacing.xxl * 7.)
                            .child(self.collapsed.clone()),
                    )
                    .child(
                        div()
                            .id("sidebar-nav-filtered-host")
                            .debug_selector(|| "sidebar-nav-filtered-host".into())
                            .flex_1()
                            .min_w_0()
                            .h(tokens.spacing.xxl * 7.)
                            .child(self.filtered.clone()),
                    ),
            )
            .child(
                div()
                    .id("sidebar-nav-event")
                    .role(Role::Status)
                    .aria_label(format!("Last sidebar navigation event: {}", self.last_event))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
            )
            .child(
                TextView::markdown(
                    "sidebar-nav-reference-note",
                    "**Reference comparison.** Inspired by Creamery's calm, compact navigation, this original GPUI composition keeps application-owned stable IDs and active state, recursively retains matching ancestors, exposes badges and disabled state, and intentionally scrolls deep content inside each constrained sidebar.",
                )
                .selectable(true),
            )
    }
}

fn gallery_typefaces() -> Vec<FineTuneTypeface> {
    vec![
        FineTuneTypeface::new("inter-regular", "Inter"),
        FineTuneTypeface::new("inter-display", "Inter"),
        FineTuneTypeface::new("jetbrains-mono", "JetBrains Mono"),
    ]
}

struct FineTuneStory {
    populated: Entity<FineTuneCard>,
    constrained: Entity<FineTuneCard>,
    values: HashMap<SharedString, FineTuneValues>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl FineTuneStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let populated_values =
            FineTuneValues::new(320., 180., 24., 0.84, "inter-regular").accent(cx.theme().info);
        let constrained_values =
            FineTuneValues::new(420., 260., 20., 0.68, "inter-regular").accent(cx.theme().success);
        let populated = cx.new(|cx| {
            FineTuneCard::new(
                "gallery-fine-tune-populated",
                populated_values.clone(),
                gallery_typefaces(),
                window,
                cx,
            )
        });
        let constrained = cx.new(|cx| {
            FineTuneCard::new(
                "gallery-fine-tune-constrained",
                constrained_values.clone(),
                gallery_typefaces(),
                window,
                cx,
            )
        });
        let subscriptions = [&populated, &constrained]
            .into_iter()
            .map(|card| {
                cx.subscribe_in(
                    card,
                    window,
                    move |this, card, event: &FineTuneEvent, window, cx| {
                        this.handle_event(card, event, window, cx);
                    },
                )
            })
            .collect();
        let values = HashMap::from([
            ("gallery-fine-tune-populated".into(), populated_values),
            ("gallery-fine-tune-constrained".into(), constrained_values),
        ]);

        Self {
            populated,
            constrained,
            values,
            last_event: "Edit a property, choose a typeface, or apply the snapshot.".into(),
            _subscriptions: subscriptions,
        }
    }

    fn handle_event(
        &mut self,
        card: &Entity<FineTuneCard>,
        event: &FineTuneEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(current) = self.values.get(event.id()).cloned() else {
            return;
        };
        let next = match event {
            FineTuneEvent::WidthChanged { width, .. } => Some(current.with_width(*width)),
            FineTuneEvent::HeightChanged { height, .. } => Some(current.with_height(*height)),
            FineTuneEvent::RadiusChanged { radius, .. } => Some(current.with_radius(*radius)),
            FineTuneEvent::OpacityChanged { opacity, .. } => Some(current.with_opacity(*opacity)),
            FineTuneEvent::TypefaceChanged { typeface_id, .. } => {
                Some(current.with_typeface(typeface_id.clone()))
            }
            FineTuneEvent::AccentChanged { accent, .. } => Some(current.with_accent(*accent)),
            FineTuneEvent::ResetRequested { .. } => {
                Some(FineTuneValues::new(320., 180., 24., 0.72, "inter-regular"))
            }
            FineTuneEvent::ApplyRequested { .. } => None,
        };
        if let Some(next) = next {
            self.values.insert(event.id().clone(), next.clone());
            card.update(cx, |card, cx| card.set_values(next, window, cx));
        }
        self.last_event = format!("{event:?}").into();
        cx.notify();
    }
}

impl Render for FineTuneStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        // The component is a design-property inspector. The story should show
        // it doing its job: one live inspector beside a preview that reflects
        // the edited values in real time — the reference demos' arrangement —
        // plus a compact constrained-height variant for the scroll contract.
        let populated_values = self
            .values
            .get("gallery-fine-tune-populated")
            .cloned()
            .expect("populated fine-tune values should exist");
        let preview_width = px(populated_values.width() as f32);
        let preview_height = px(populated_values.height() as f32);
        let accent = populated_values.accent_color();
        v_flex()
            .gap(tokens.spacing.sm)
            .child(
                h_flex()
                    .items_start()
                    .flex_wrap()
                    .gap(tokens.spacing.md)
                    // The inspector being demonstrated.
                    .child(
                        v_flex()
                            .id("fine-tune-populated-host")
                            .role(Role::Group)
                            .aria_label("Fine-tune inspector")
                            .w(px(320.))
                            .gap(tokens.spacing.xs)
                            .child("Inspector")
                            .child(div().h(px(416.)).child(self.populated.clone())),
                    )
                    // Live preview bound to the same values the inspector
                    // edits: width, height, radius, opacity, typeface, and
                    // accent color all apply here as you drag.
                    .child(
                        v_flex()
                            .id("fine-tune-preview-host")
                            .role(Role::Group)
                            .aria_label("Live preview of the fine-tuned card")
                            .flex_1()
                            .min_w(px(280.))
                            .gap(tokens.spacing.xs)
                            .child("Live preview")
                            .child(
                                div().flex_1().items_start().child(
                                    div()
                                        .id("fine-tune-preview-target")
                                        .debug_selector(|| {
                                            "fine-tune-preview-target".to_owned()
                                        })
                                        .w(preview_width)
                                        .h(preview_height)
                                        .rounded(px(populated_values.radius() as f32))
                                        .opacity(populated_values.opacity())
                                        .bg(cx.theme().primary)
                                        .border_1()
                                        .border_color(cx.theme().border)
                                        .when_some(accent, |this, accent| {
                                            this.child(
                                                div()
                                                    .absolute()
                                                    .bottom_3()
                                                    .right_3()
                                                    .size_4()
                                                    .rounded_full()
                                                    .bg(accent),
                                            )
                                        }),
                                ),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .id("fine-tune-constrained-host")
                    .role(Role::Group)
                    .aria_label("Constrained scrolling Fine-tune card")
                    .flex_wrap()
                    .gap(tokens.spacing.xs)
                    .child("Constrained height · scroll to apply")
                    .child(
                        div()
                            .w_full()
                            .max_w(px(420.))
                            .h(tokens.spacing.xxl * 7.)
                            .overflow_hidden()
                            .child(self.constrained.clone()),
                    ),
            )
            .child(
                div()
                    .id("fine-tune-event")
                    .role(Role::Status)
                    .aria_label(format!("Last Fine-tune event: {}", self.last_event))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
            )
            .child(
                TextView::markdown(
                    "fine-tune-reference-note",
                    "**Reference comparison.** Beautiful UI's compact Fine-tune surface groups width, height, radius, opacity, and typeface. This original GPUI inspector adds an optional named accent color, application-owned stable identities, typed controlled events, duplicate-label-safe typeface selection, and deliberate constrained-height scrolling.",
                )
                .selectable(true),
            )
    }
}

struct PromptBarStory {
    empty: Entity<PromptBar>,
    ready: Entity<PromptBar>,
    multiline: Entity<PromptBar>,
    running: Entity<PromptBar>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl PromptBarStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let empty = cx.new(|cx| PromptBar::new("gallery-prompt-empty", window, cx));
        let ready = cx.new(|cx| {
            Self::configured_prompt(
                "gallery-prompt-ready",
                "Compare @cr",
                ProgressState::Pending,
                true,
                window,
                cx,
            )
        });
        let multiline = cx.new(|cx| {
            Self::configured_prompt(
                "gallery-prompt-multiline",
                "Compare supplier pricing\nand explain the largest variance",
                ProgressState::Pending,
                false,
                window,
                cx,
            )
        });
        let running = cx.new(|cx| {
            Self::configured_prompt(
                "gallery-prompt-running",
                "Prepare the supplier comparison",
                ProgressState::Running,
                false,
                window,
                cx,
            )
        });
        let empty_subscription = cx.subscribe_in(
            &empty,
            window,
            |this, prompt, event: &PromptBarEvent, _, cx| {
                this.on_event(prompt, event, cx);
            },
        );
        let ready_subscription = cx.subscribe_in(
            &ready,
            window,
            |this, prompt, event: &PromptBarEvent, _, cx| {
                this.on_event(prompt, event, cx);
            },
        );
        let multiline_subscription = cx.subscribe_in(
            &multiline,
            window,
            |this, prompt, event: &PromptBarEvent, _, cx| {
                this.on_event(prompt, event, cx);
            },
        );
        let running_subscription = cx.subscribe_in(
            &running,
            window,
            |this, prompt, event: &PromptBarEvent, _, cx| {
                this.on_event(prompt, event, cx);
            },
        );

        Self {
            empty,
            ready,
            multiline,
            running,
            last_event: "Interact with either composer to inspect its typed event.".into(),
            _subscriptions: vec![
                empty_subscription,
                ready_subscription,
                multiline_subscription,
                running_subscription,
            ],
        }
    }

    fn configured_prompt(
        id: &'static str,
        draft: &'static str,
        progress: ProgressState,
        with_attachment: bool,
        window: &mut Window,
        cx: &mut Context<PromptBar>,
    ) -> PromptBar {
        let mut prompt = PromptBar::new(id, window, cx);
        prompt.set_models(
            [
                PromptModel::new("balanced", "Balanced"),
                PromptModel::new("fast", "Fast"),
                PromptModel::new("offline", "Offline").disabled(true),
            ],
            cx,
        );
        prompt.set_selected_model("balanced", cx);
        prompt.set_mentions(
            [
                PromptMention::new("creamery", "Creamery"),
                PromptMention::new("suppliers", "Suppliers"),
            ],
            cx,
        );
        prompt.set_commands(
            [
                PromptCommand::new("compare", "compare")
                    .description("Compare the selected records"),
                PromptCommand::new("summarize", "summarize")
                    .description("Summarize current context"),
            ],
            cx,
        );
        if with_attachment {
            prompt.set_attachments([PromptAttachment::new("pricing", "pricing.md")], cx);
        }
        prompt.set_progress(progress, cx);
        prompt.set_draft(draft, window, cx);
        prompt
    }

    fn on_event(
        &mut self,
        prompt: &Entity<PromptBar>,
        event: &PromptBarEvent,
        cx: &mut Context<Self>,
    ) {
        self.last_event = format!("{event:?}").into();
        match event {
            PromptBarEvent::ModelChanged { model_id, .. } => {
                prompt.update(cx, |prompt, cx| {
                    prompt.set_selected_model(model_id.clone(), cx);
                });
            }
            PromptBarEvent::Submit { .. } => {
                prompt.update(cx, |prompt, cx| {
                    prompt.set_progress(ProgressState::Running, cx);
                });
            }
            PromptBarEvent::CancelRequested { .. } => {
                prompt.update(cx, |prompt, cx| {
                    prompt.set_progress(ProgressState::Complete, cx);
                });
            }
            PromptBarEvent::DraftChanged { .. }
            | PromptBarEvent::MentionSelected { .. }
            | PromptBarEvent::CommandSelected { .. }
            | PromptBarEvent::AttachRequested { .. }
            | PromptBarEvent::AttachmentRemoved { .. }
            | PromptBarEvent::EnhanceRequested { .. } => {}
        }
        cx.notify();
    }
}

impl Render for PromptBarStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        v_flex()
            .gap(tokens.spacing.md)
            .child(
                div()
                    .id("prompt-bar-empty-heading")
                    .debug_selector(|| "prompt-bar-empty-heading".into())
                    .role(Role::Heading)
                    .aria_label("Empty prompt without a model catalog")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Empty without models"),
            )
            .child(self.empty.clone())
            .child(
                div()
                    .id("prompt-bar-ready-heading")
                    .role(Role::Heading)
                    .aria_label("Ready prompt with mention suggestions")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Ready with mention suggestions"),
            )
            .child(self.ready.clone())
            .child(
                div()
                    .id("prompt-bar-multiline-heading")
                    .debug_selector(|| "prompt-bar-multiline-heading".into())
                    .role(Role::Heading)
                    .aria_label("Multiline prompt draft")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Multiline draft"),
            )
            .child(self.multiline.clone())
            .child(
                div()
                    .id("prompt-bar-running-heading")
                    .role(Role::Heading)
                    .aria_label("Running prompt with cancellation")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("Running with cancellation"),
            )
            .child(self.running.clone())
            .child(
                TextView::markdown(
                    "prompt-bar-event-log",
                    format!("**Last typed event.** {}", self.last_event),
                )
                .selectable(true),
            )
            .child(
                TextView::markdown(
                    "prompt-bar-reference-note",
                    "**Reference comparison.** Beautiful UI places its compact `Prompt or tag a flavor with @` composer inside Chat. This GPUI implementation deliberately makes the composer reusable on its own, retains the upstream native textarea for IME and selection, and adds typed model, attachment, command, enhancement, submission, and cancellation events without owning application async work.",
                )
                .selectable(true),
            )
    }
}

struct SelectionActionsStory {
    selection: Entity<SelectionActions>,
    last_event: SharedString,
    _subscription: Subscription,
}

impl SelectionActionsStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let selection = cx.new(|cx| {
            SelectionActions::new(
                "gallery-selection-actions",
                "## Weekly flavor review\n\nMint Chip demand softened while Vanilla recovered. Select any phrase in this readable analysis, then choose an action. Native **Ctrl/Cmd+A** and copy remain available.",
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
        let _subscription =
            cx.subscribe(&selection, |this, _, event: &SelectionActionsEvent, cx| {
                this.last_event = format!("{event:?}").into();
                cx.notify();
            });
        Self {
            selection,
            last_event: "Select text to reveal Ask, Explain, and Rewrite.".into(),
            _subscription,
        }
    }
}

impl Render for SelectionActionsStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        v_flex()
            .gap(tokens.spacing.md)
            .child(
                div()
                    .id("selection-actions-instructions")
                    .role(Role::Note)
                    .aria_label("Selection actions instructions")
                    .text_sm()
                    .text_color(cx.theme().muted_foreground)
                    .child("Drag over the analysis or use Ctrl/Cmd+A, then activate an action."),
            )
            .child(
                div()
                    // Room for the selectable passage plus the action
                    // toolbar without clipping either.
                    .h(px(220.))
                    .max_h(px(220.))
                    .flex_none()
                    .child(self.selection.clone()),
            )
            .child(
                TextView::markdown(
                    "selection-actions-event-log",
                    format!("**Last typed event.** {}", self.last_event),
                )
                .selectable(true),
            )
            .child(
                TextView::markdown(
                    "selection-actions-reference-note",
                    "**Reference comparison.** Beautiful UI presents Ask, Explain, and Rewrite in a compact floating selection toolbar. This original GPUI composition deliberately keeps gpui-component's Markdown selection and native copy behavior, emits stable typed IDs for application-owned work, names every keyboard control, and clamps the token-driven toolbar inside constrained surfaces without motion-dependent meaning.",
                )
                .selectable(true),
            )
    }
}

/// Theme presets available to native and web gallery hosts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryTheme {
    /// The default light theme from gpui-component.
    Light,
    /// The default dark theme from gpui-component.
    Dark,
    /// The bundled high-contrast review theme.
    Contrast,
    /// Deep violet-on-ink showcase theme.
    MidnightViolet,
    /// Muted Nordic blue-grey showcase theme.
    NordFrost,
    /// Warm ember-and-charcoal showcase theme.
    EmberDusk,
    /// Warm paper-white teal-accent light showcase theme.
    PaperLight,
}

impl GalleryTheme {
    fn next(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Contrast,
            Self::Contrast => Self::MidnightViolet,
            Self::MidnightViolet => Self::NordFrost,
            Self::NordFrost => Self::EmberDusk,
            Self::EmberDusk => Self::PaperLight,
            Self::PaperLight => Self::Light,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Contrast => "Contrast",
            Self::MidnightViolet => "Midnight Violet",
            Self::NordFrost => "Nord Frost",
            Self::EmberDusk => "Ember Dusk",
            Self::PaperLight => "Paper Light",
        }
    }

    /// The registered registry name for this preset, if it is a bundled
    /// JSON theme rather than an upstream default.
    fn registry_name(self) -> Option<&'static str> {
        match self {
            Self::Light | Self::Dark => None,
            Self::Contrast => Some(CONTRAST_THEME_NAME),
            Self::MidnightViolet => Some("Midnight Violet"),
            Self::NordFrost => Some("Nord Frost"),
            Self::EmberDusk => Some("Ember Dusk"),
            Self::PaperLight => Some("Paper Light"),
        }
    }
}

/// Applies a complete gallery preset, including its registered token configuration.
pub fn apply_gallery_theme(preset: GalleryTheme, window: Option<&mut Window>, cx: &mut App) {
    let (mode, config) = match preset {
        GalleryTheme::Light => (
            ThemeMode::Light,
            ThemeRegistry::global(cx).default_light_theme().clone(),
        ),
        GalleryTheme::Dark => (
            ThemeMode::Dark,
            ThemeRegistry::global(cx).default_dark_theme().clone(),
        ),
        other => {
            let Some(name) = other.registry_name() else {
                return;
            };
            let Some(config) = ThemeRegistry::global(cx).themes().get(name).cloned() else {
                return;
            };
            (
                if config.mode == ThemeMode::Light {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                },
                config,
            )
        }
    };

    let theme = Theme::global_mut(cx);
    if mode.is_dark() {
        theme.dark_theme = config;
    } else {
        theme.light_theme = config;
    }
    Theme::change(mode, window, cx);
}

fn records_story_columns() -> Vec<RecordColumn> {
    vec![
        RecordColumn::new("supplier", "Supplier")
            .sortable(true)
            .fixed(true)
            .width(px(240.)),
        RecordColumn::new("region", "Region")
            .sortable(true)
            .width(px(130.)),
        RecordColumn::new("products", "Products").width(px(220.)),
        RecordColumn::new("status", "Status")
            .sortable(true)
            .width(px(140.)),
    ]
}

fn records_story_rows() -> Vec<RecordRow> {
    vec![
        RecordRow::new("alpenrose", "Alpenrose Dairy").cells([
            RecordCell::new("supplier", "Alpenrose Dairy"),
            RecordCell::new("region", "Northwest"),
            RecordCell::tags("products", ["Milk", "Cream"]),
            RecordCell::status("status", "Ready", RecordStatusTone::Positive),
        ]),
        RecordRow::new("tillamook", "Tillamook County Creamery").cells([
            RecordCell::new("supplier", "Tillamook County Creamery"),
            RecordCell::new("region", "Pacific"),
            RecordCell::tags("products", ["Cheese", "Ice cream"]),
            RecordCell::status("status", "Review", RecordStatusTone::Caution),
        ]),
        RecordRow::new("cascade", "Cascade Cultured Foods")
            .cells([
                RecordCell::new("supplier", "Cascade Cultured Foods"),
                RecordCell::new("region", "Mountain"),
                RecordCell::tags("products", ["Yogurt", "Kefir"]),
                RecordCell::status("status", "Paused", RecordStatusTone::Neutral),
            ])
            .disabled(true),
        RecordRow::new("redwood", "Redwood Organic Dairy").cells([
            RecordCell::new("supplier", "Redwood Organic Dairy"),
            RecordCell::new("region", "West"),
            RecordCell::tags("products", ["Butter", "Cream"]),
            RecordCell::status("status", "Blocked", RecordStatusTone::Critical),
        ]),
    ]
}

fn records_story_many_rows() -> Arc<[RecordRow]> {
    (0..100)
        .map(|index| {
            RecordRow::new(format!("supplier-{index}"), format!("Supplier {index}")).cells([
                RecordCell::new("supplier", format!("Supplier {index}")),
                RecordCell::new("region", format!("Region {}", index % 8)),
                RecordCell::tags("products", ["Milk", "Cream"]),
                RecordCell::status("status", "Ready", RecordStatusTone::Positive),
            ])
        })
        .collect::<Vec<_>>()
        .into()
}

fn configured_records_table(
    id: &'static str,
    label: &'static str,
    records: Progressive<Arc<[RecordRow]>>,
    window: &mut Window,
    cx: &mut Context<RecordsTable>,
) -> RecordsTable {
    let mut table = RecordsTable::new(id, label, window, cx);
    table.set_columns(records_story_columns(), window, cx);
    table.set_records(records, window, cx);
    table
}

struct RecordsTableStory {
    populated: Entity<RecordsTable>,
    loading: Entity<RecordsTable>,
    failed: Entity<RecordsTable>,
    empty: Entity<RecordsTable>,
    disabled: Entity<RecordsTable>,
    selected: Entity<RecordsTable>,
    constrained: Entity<RecordsTable>,
    active_state: TableStoryState,
    records: Vec<RecordRow>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl RecordsTableStory {
    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(state) = TableStoryState::ALL.get(index).copied() {
            self.active_state = state;
            cx.notify();
        }
    }
}

impl RecordsTableStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows: Arc<[RecordRow]> = records_story_rows().into();
        let populated = cx.new(|cx| {
            configured_records_table(
                "gallery-records-populated",
                "Supplier records",
                Progressive::complete(rows.clone()),
                window,
                cx,
            )
        });
        let loading = cx.new(|cx| {
            configured_records_table(
                "gallery-records-loading",
                "Loading supplier records",
                Progressive::running(Arc::from([])),
                window,
                cx,
            )
        });
        let failed = cx.new(|cx| {
            configured_records_table(
                "gallery-records-error",
                "Unavailable supplier records",
                Progressive::failed(Arc::from([]), "Supplier service is unavailable"),
                window,
                cx,
            )
        });
        let empty = cx.new(|cx| {
            configured_records_table(
                "gallery-records-empty",
                "Empty supplier records",
                Progressive::complete(Arc::from([])),
                window,
                cx,
            )
        });
        let disabled = cx.new(|cx| {
            configured_records_table(
                "gallery-records-disabled",
                "Disabled supplier record",
                Progressive::complete(Arc::from([records_story_rows()[2].clone()])),
                window,
                cx,
            )
        });
        let selected = cx.new(|cx| {
            let mut table = configured_records_table(
                "gallery-records-selected",
                "Selected supplier record",
                Progressive::complete(rows.clone()),
                window,
                cx,
            );
            table.set_selected_row("tillamook", window, cx);
            table
        });
        let constrained = cx.new(|cx| {
            configured_records_table(
                "gallery-records-constrained",
                "Constrained supplier records",
                Progressive::complete(records_story_many_rows()),
                window,
                cx,
            )
        });

        let mut subscriptions = Vec::new();
        for table in [&populated, &selected] {
            subscriptions.push(cx.subscribe_in(
                table,
                window,
                |this, table, event: &RecordsTableEvent, window, cx| {
                    this.last_event = format!("{event:?}").into();
                    match event {
                        RecordsTableEvent::SelectionRequested { row_id, .. } => table
                            .update(cx, |table, cx| {
                                table.set_selected_row(row_id.clone(), window, cx)
                            }),
                        RecordsTableEvent::SortRequested {
                            column_id,
                            direction,
                            ..
                        } => {
                            if let Some(direction) = direction {
                                this.records.sort_by(|left, right| {
                                    let ordering = left
                                        .cell(column_id)
                                        .map(RecordCell::value)
                                        .cmp(&right.cell(column_id).map(RecordCell::value));
                                    match direction {
                                        RecordSortDirection::Ascending => ordering,
                                        RecordSortDirection::Descending => ordering.reverse(),
                                    }
                                });
                            } else {
                                this.records = records_story_rows();
                            }
                            let records: Arc<[RecordRow]> = this.records.clone().into();
                            table.update(cx, |table, cx| {
                                table.set_records(Progressive::complete(records), window, cx);
                                table.set_sort(column_id.clone(), *direction, window, cx);
                            });
                        }
                        RecordsTableEvent::ActivationRequested { .. } => {}
                    }
                    cx.notify();
                },
            ));
        }

        Self {
            populated,
            loading,
            failed,
            empty,
            disabled,
            selected,
            constrained,
            records: records_story_rows(),
            last_event: "Select a row or sort a column to inspect its typed event.".into(),
            _subscriptions: subscriptions,
            active_state: TableStoryState::Populated,
        }
    }

    fn state(
        selector: &'static str,
        title: &'static str,
        table: Entity<RecordsTable>,
    ) -> impl IntoElement {
        v_flex()
            .id(selector)
            .debug_selector(move || selector.into())
            .flex_none()
            .gap_1()
            .child(
                div()
                    .id(format!("{selector}-heading"))
                    .role(Role::Heading)
                    .aria_label(title)
                    .text_xs()
                    .child(title),
            )
            .child(div().h(px(210.)).child(table))
    }
}

impl Render for RecordsTableStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let switcher = story_state_switcher(
            cx.weak_entity(),
            "records-story",
            TableStoryState::LABELS,
            self.active_state.index(),
            Self::set_active_state,
        );
        let active: AnyElement = match self.active_state {
            TableStoryState::Populated => Self::state(
                "records-story-populated",
                "Populated and sortable",
                self.populated.clone(),
            )
            .into_any_element(),
            TableStoryState::Loading => {
                Self::state("records-story-loading", "Loading", self.loading.clone())
                    .into_any_element()
            }
            TableStoryState::Error => {
                Self::state("records-story-error", "Error", self.failed.clone()).into_any_element()
            }
            TableStoryState::Empty => {
                Self::state("records-story-empty", "Empty", self.empty.clone()).into_any_element()
            }
            TableStoryState::Disabled => Self::state(
                "records-story-disabled",
                "Disabled row",
                self.disabled.clone(),
            )
            .into_any_element(),
            TableStoryState::Selected => Self::state(
                "records-story-selected",
                "Controlled selection",
                self.selected.clone(),
            )
            .into_any_element(),
            TableStoryState::Constrained => v_flex()
                .id("records-story-constrained")
                .debug_selector(|| "records-story-constrained".into())
                .flex_none()
                .gap_1()
                .child(
                    div()
                        .id("records-story-constrained-heading")
                        .role(Role::Heading)
                        .aria_label("Constrained height and width")
                        .text_xs()
                        .child("Constrained height and width"),
                )
                .child(
                    div()
                        .w(px(520.))
                        .h(px(180.))
                        .child(self.constrained.clone()),
                )
                .into_any_element(),
        };

        v_flex().gap_3()
            .child(switcher)
            .child(active)
            .child(TextView::markdown("records-story-event-log", format!("**Last typed event.** {}", self.last_event)).selectable(true))
            .child(div().id("records-story-reference-note").debug_selector(|| "records-story-reference-note".into())
                .child(TextView::markdown("records-story-reference-copy", "**Reference comparison.** Beautiful UI's records table establishes the compact density and pinned identity column. This GPUI version keeps application-owned sorting and selection, stable row and column IDs, selectable cells, semantic state, and two-axis virtualization.").selectable(true)))
    }
}

fn diff_story_columns() -> Vec<DiffColumn> {
    vec![
        DiffColumn::new("flavor", "Flavor")
            .width(px(240.))
            .fixed(true)
            .sortable(true),
        DiffColumn::new("category", "Category")
            .width(px(220.))
            .sortable(true),
        DiffColumn::new("supplier", "Supplier")
            .width(px(240.))
            .sortable(true),
    ]
}

fn diff_story_rows() -> Vec<DiffRow> {
    vec![
        DiffRow::new("rocky-road", "Rocky Road", DiffChangeKind::Changed).cells([
            DiffCell::unchanged("flavor", "Rocky Road"),
            DiffCell::changed("category", "Classic", "Seasonal"),
            DiffCell::unchanged("supplier", "aurora-scoops"),
        ]),
        DiffRow::new("bubblegum", "Bubblegum", DiffChangeKind::Removed).cells([
            DiffCell::removed("flavor", "Bubblegum"),
            DiffCell::removed("category", "Retro"),
            DiffCell::removed("supplier", "kumo-creamery"),
        ]),
        DiffRow::new("mint-chip", "Mint Chip", DiffChangeKind::Changed).cells([
            DiffCell::unchanged("flavor", "Mint Chip"),
            DiffCell::unchanged("category", "Classic"),
            DiffCell::changed("supplier", "kumo-creamery", "maple-orbit"),
        ]),
        DiffRow::new("pistachio", "Pistachio", DiffChangeKind::Added).cells([
            DiffCell::added("flavor", "Pistachio"),
            DiffCell::added("category", "Seasonal"),
            DiffCell::added("supplier", "maple-orbit"),
        ]),
    ]
}

fn diff_story_many_rows() -> Arc<[DiffRow]> {
    (0..100)
        .map(|index| {
            let kind = match index % 4 {
                0 => DiffChangeKind::Added,
                1 => DiffChangeKind::Removed,
                2 => DiffChangeKind::Changed,
                _ => DiffChangeKind::Unchanged,
            };
            let flavor = match kind {
                DiffChangeKind::Added => DiffCell::added("flavor", format!("Flavor {index}")),
                DiffChangeKind::Removed => DiffCell::removed("flavor", format!("Flavor {index}")),
                DiffChangeKind::Changed => DiffCell::changed(
                    "flavor",
                    format!("Flavor {index}"),
                    format!("Seasonal {index}"),
                ),
                DiffChangeKind::Unchanged => {
                    DiffCell::unchanged("flavor", format!("Flavor {index}"))
                }
            };
            DiffRow::new(
                format!("proposal-{index}"),
                format!("Proposal {index}"),
                kind,
            )
            .cells([
                flavor,
                DiffCell::unchanged("category", format!("Category {}", index % 8)),
                DiffCell::changed(
                    "supplier",
                    format!("supplier-{}", index % 11),
                    format!("supplier-{}", (index + 1) % 11),
                ),
            ])
        })
        .collect::<Vec<_>>()
        .into()
}

fn configured_diff_table(
    id: &'static str,
    label: &'static str,
    rows: Progressive<Arc<[DiffRow]>>,
    window: &mut Window,
    cx: &mut Context<DiffTable>,
) -> DiffTable {
    let mut table = DiffTable::new(id, label, window, cx);
    table.set_columns(diff_story_columns(), window, cx);
    table.set_rows(rows, window, cx);
    table
}

struct DiffTableStory {
    populated: Entity<DiffTable>,
    loading: Entity<DiffTable>,
    failed: Entity<DiffTable>,
    empty: Entity<DiffTable>,
    disabled: Entity<DiffTable>,
    selected: Entity<DiffTable>,
    constrained: Entity<DiffTable>,
    active_state: TableStoryState,
    rows: Vec<DiffRow>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl DiffTableStory {
    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(state) = TableStoryState::ALL.get(index).copied() {
            self.active_state = state;
            cx.notify();
        }
    }
}

impl DiffTableStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows: Arc<[DiffRow]> = diff_story_rows().into();
        let populated = cx.new(|cx| {
            configured_diff_table(
                "gallery-diff-populated",
                "Proposed menu cleanup",
                Progressive::complete(rows.clone()),
                window,
                cx,
            )
        });
        let loading = cx.new(|cx| {
            configured_diff_table(
                "gallery-diff-loading",
                "Loading proposed menu edits",
                Progressive::running(Arc::from([])),
                window,
                cx,
            )
        });
        let failed = cx.new(|cx| {
            configured_diff_table(
                "gallery-diff-error",
                "Unavailable proposed menu edits",
                Progressive::failed(Arc::from([]), "Menu proposal service is unavailable"),
                window,
                cx,
            )
        });
        let empty = cx.new(|cx| {
            configured_diff_table(
                "gallery-diff-empty",
                "Empty proposed menu edits",
                Progressive::complete(Arc::from([])),
                window,
                cx,
            )
        });
        let disabled = cx.new(|cx| {
            configured_diff_table(
                "gallery-diff-disabled",
                "Disabled menu proposal",
                Progressive::complete(Arc::from([diff_story_rows()[1].clone().disabled(true)])),
                window,
                cx,
            )
        });
        let selected = cx.new(|cx| {
            let mut table = configured_diff_table(
                "gallery-diff-selected",
                "Selected menu proposal",
                Progressive::complete(rows.clone()),
                window,
                cx,
            );
            table.set_selected_row("mint-chip", window, cx);
            table
        });
        let constrained = cx.new(|cx| {
            configured_diff_table(
                "gallery-diff-constrained",
                "Constrained proposed menu edits",
                Progressive::complete(diff_story_many_rows()),
                window,
                cx,
            )
        });

        let mut subscriptions = Vec::new();
        for table in [&populated, &selected] {
            subscriptions.push(cx.subscribe_in(
                table,
                window,
                |this, table, event: &DiffTableEvent, window, cx| {
                    this.last_event = format!("{event:?}").into();
                    match event {
                        DiffTableEvent::SelectionRequested { row_id, .. } => table
                            .update(cx, |table, cx| {
                                table.set_selected_row(row_id.clone(), window, cx)
                            }),
                        DiffTableEvent::SortRequested {
                            column_id,
                            direction,
                            ..
                        } => {
                            if let Some(direction) = direction {
                                this.rows.sort_by(|left, right| {
                                    let left_value = left
                                        .cell(column_id)
                                        .and_then(|cell| cell.after().or_else(|| cell.before()))
                                        .unwrap_or_default();
                                    let right_value = right
                                        .cell(column_id)
                                        .and_then(|cell| cell.after().or_else(|| cell.before()))
                                        .unwrap_or_default();
                                    let ordering = left_value.cmp(right_value);
                                    match direction {
                                        DiffSortDirection::Ascending => ordering,
                                        DiffSortDirection::Descending => ordering.reverse(),
                                    }
                                });
                            } else {
                                this.rows = diff_story_rows();
                            }
                            let rows: Arc<[DiffRow]> = this.rows.clone().into();
                            table.update(cx, |table, cx| {
                                table.set_rows(Progressive::complete(rows), window, cx);
                                table.set_sort(column_id.clone(), *direction, window, cx);
                            });
                        }
                        DiffTableEvent::DecisionRequested { row_id, action, .. } => {
                            if let Some(row) =
                                this.rows.iter_mut().find(|row| row.id() == row_id.as_ref())
                            {
                                let state = match action {
                                    DiffProposalAction::Accept => DiffProposalState::Accepted,
                                    DiffProposalAction::Reject => DiffProposalState::Rejected,
                                };
                                *row = row.clone().state(state);
                            }
                            let rows: Arc<[DiffRow]> = this.rows.clone().into();
                            table.update(cx, |table, cx| {
                                table.set_rows(Progressive::complete(rows), window, cx)
                            });
                        }
                        DiffTableEvent::ReviewRequested { .. } => {}
                    }
                    cx.notify();
                },
            ));
        }

        Self {
            populated,
            loading,
            failed,
            empty,
            disabled,
            selected,
            constrained,
            rows: diff_story_rows(),
            last_event: "Select, sort, review, accept, or reject a proposal.".into(),
            _subscriptions: subscriptions,
            active_state: TableStoryState::Populated,
        }
    }

    fn state(
        selector: &'static str,
        title: &'static str,
        table: Entity<DiffTable>,
    ) -> impl IntoElement {
        v_flex()
            .id(selector)
            .debug_selector(move || selector.into())
            .flex_none()
            .gap_1()
            .child(
                div()
                    .id(format!("{selector}-heading"))
                    .role(Role::Heading)
                    .aria_label(title)
                    .text_xs()
                    .child(title),
            )
            .child(div().h(px(230.)).child(table))
    }
}

impl Render for DiffTableStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let switcher = story_state_switcher(
            cx.weak_entity(),
            "diff-story",
            TableStoryState::LABELS,
            self.active_state.index(),
            Self::set_active_state,
        );
        let active: AnyElement = match self.active_state {
            TableStoryState::Populated => Self::state(
                "diff-story-populated",
                "Populated and sortable",
                self.populated.clone(),
            )
            .into_any_element(),
            TableStoryState::Loading => {
                Self::state("diff-story-loading", "Loading", self.loading.clone())
                    .into_any_element()
            }
            TableStoryState::Error => {
                Self::state("diff-story-error", "Error", self.failed.clone()).into_any_element()
            }
            TableStoryState::Empty => {
                Self::state("diff-story-empty", "Empty", self.empty.clone()).into_any_element()
            }
            TableStoryState::Disabled => Self::state(
                "diff-story-disabled",
                "Disabled proposal",
                self.disabled.clone(),
            )
            .into_any_element(),
            TableStoryState::Selected => Self::state(
                "diff-story-selected",
                "Controlled selection and decision",
                self.selected.clone(),
            )
            .into_any_element(),
            TableStoryState::Constrained => v_flex()
                .id("diff-story-constrained")
                .debug_selector(|| "diff-story-constrained".into())
                .flex_none()
                .gap_1()
                .child(
                    div()
                        .id("diff-story-constrained-heading")
                        .role(Role::Heading)
                        .aria_label("Constrained height and width")
                        .text_xs()
                        .child("Constrained height and width"),
                )
                .child(
                    div()
                        .w(px(520.))
                        .h(px(180.))
                        .child(self.constrained.clone()),
                )
                .into_any_element(),
        };

        v_flex()
            .gap_3()
            .child(switcher)
            .child(active)
            .child(
                TextView::markdown(
                    "diff-story-event-log",
                    format!("**Last typed event.** {}", self.last_event),
                )
                .selectable(true),
            )
            .child(
                div()
                    .id("diff-story-reference-note")
                    .debug_selector(|| "diff-story-reference-note".into())
                    .child(
                        TextView::markdown(
                            "diff-story-reference-copy",
                            "**Reference comparison.** Beautiful UI presents AI-proposed menu edits as a compact table. This original GPUI composition adds explicit before/after labels, stable proposal and column IDs, application-owned decisions and sorting, selectable cells, direct semantics, and two-axis virtualization.",
                        )
                        .selectable(true),
                    ),
            )
    }
}

fn filter_story_columns() -> [FilterColumn; 4] {
    [
        FilterColumn::new("task", "Task name")
            .width(px(230.))
            .fixed(true),
        FilterColumn::new("date", "Date")
            .width(px(110.))
            .sortable(true),
        FilterColumn::new("status", "Status")
            .width(px(130.))
            .sortable(true),
        FilterColumn::new("advisor", "Advisor").width(px(190.)),
    ]
}

fn filter_story_rows() -> Vec<FilterRow> {
    [
        (
            "mango",
            "Restock mango sorbet",
            "Dec 03",
            "To do",
            "Mango Moon Gelato",
        ),
        (
            "sesame",
            "Churn black sesame",
            "Sep 22",
            "In Progress",
            "Kumo Creamery",
        ),
        (
            "menu",
            "Print summer menu",
            "Jan 02",
            "To do",
            "Coral Coast Sorbet",
        ),
        (
            "batch",
            "Taste-test batch 42",
            "Nov 08",
            "In Progress",
            "Maple Orbit",
        ),
        (
            "cones",
            "Order waffle cones",
            "Apr 14",
            "Completed",
            "Aurora Scoops",
        ),
    ]
    .into_iter()
    .map(|(id, task, date, status, advisor)| {
        let tone = match status {
            "Completed" => RecordStatusTone::Positive,
            "In Progress" => RecordStatusTone::Caution,
            _ => RecordStatusTone::Neutral,
        };
        FilterRow::new(id, task).cells([
            FilterCell::new("task", task),
            FilterCell::new("date", date),
            FilterCell::status("status", status, tone),
            FilterCell::new("advisor", advisor),
        ])
    })
    .collect()
}

fn filter_story_definitions(active: &str, rows: &[FilterRow]) -> Vec<FilterDefinition> {
    let count = |status: &str| {
        rows.iter()
            .filter(|row| {
                row.cell("status")
                    .is_some_and(|cell| cell.value() == status)
            })
            .count()
    };
    [
        ("all", "All", rows.len()),
        ("todo", "To do", count("To do")),
        ("progress", "In Progress", count("In Progress")),
        ("completed", "Completed", count("Completed")),
    ]
    .into_iter()
    .map(|(id, label, count)| FilterDefinition::new(id, label, count).active(id == active))
    .collect()
}

fn filter_story_projection(rows: &[FilterRow], active: &str) -> Arc<[FilterRow]> {
    rows.iter()
        .filter(|row| match active {
            "todo" => row
                .cell("status")
                .is_some_and(|cell| cell.value() == "To do"),
            "progress" => row
                .cell("status")
                .is_some_and(|cell| cell.value() == "In Progress"),
            "completed" => row
                .cell("status")
                .is_some_and(|cell| cell.value() == "Completed"),
            _ => true,
        })
        .cloned()
        .collect::<Vec<_>>()
        .into()
}

#[derive(Clone)]
struct FilterStoryProjection {
    active_filter: SharedString,
    sort_column: Option<SharedString>,
    sort_direction: Option<FilterSortDirection>,
}

impl Default for FilterStoryProjection {
    fn default() -> Self {
        Self {
            active_filter: "all".into(),
            sort_column: None,
            sort_direction: None,
        }
    }
}

fn filter_story_project_rows(
    rows: &[FilterRow],
    projection: &FilterStoryProjection,
) -> Arc<[FilterRow]> {
    let mut rows = filter_story_projection(rows, &projection.active_filter)
        .iter()
        .cloned()
        .collect::<Vec<_>>();
    if let (Some(column_id), Some(direction)) = (&projection.sort_column, projection.sort_direction)
    {
        rows.sort_by(|left, right| {
            let left = left
                .cell(column_id)
                .map(FilterCell::value)
                .unwrap_or_default();
            let right = right
                .cell(column_id)
                .map(FilterCell::value)
                .unwrap_or_default();
            let ordering = left.cmp(right);
            match direction {
                FilterSortDirection::Ascending => ordering,
                FilterSortDirection::Descending => ordering.reverse(),
            }
        });
    }
    rows.into()
}

fn reduce_filter_story_projection(
    projections: &mut HashMap<SharedString, FilterStoryProjection>,
    event: &FilterTableEvent,
) {
    match event {
        FilterTableEvent::FilterRequested {
            id,
            filter_id,
            active,
        } => {
            projections.entry(id.clone()).or_default().active_filter = if *active {
                filter_id.clone()
            } else {
                "all".into()
            };
        }
        FilterTableEvent::SortRequested {
            id,
            column_id,
            direction,
        } => {
            let projection = projections.entry(id.clone()).or_default();
            projection.sort_column = direction.map(|_| column_id.clone());
            projection.sort_direction = *direction;
        }
        FilterTableEvent::SelectionRequested { .. }
        | FilterTableEvent::ActivationRequested { .. } => {}
    }
}

fn filter_story_many_rows() -> Arc<[FilterRow]> {
    (0..1_000)
        .map(|index| {
            let status = match index % 3 {
                0 => "To do",
                1 => "In Progress",
                _ => "Completed",
            };
            FilterRow::new(format!("task-{index}"), format!("Task {index}")).cells([
                FilterCell::new("task", format!("Task {index}")),
                FilterCell::new("date", format!("Aug {:02}", index % 28 + 1)),
                FilterCell::status("status", status, RecordStatusTone::Neutral),
                FilterCell::new("advisor", format!("Advisor {}", index % 9)),
            ])
        })
        .collect::<Vec<_>>()
        .into()
}

fn configured_filter_table(
    id: &'static str,
    label: &'static str,
    rows: Progressive<Arc<[FilterRow]>>,
    filters: Vec<FilterDefinition>,
    window: &mut Window,
    cx: &mut Context<FilterTable>,
) -> FilterTable {
    let mut table = FilterTable::new(id, label, window, cx);
    table.set_columns(filter_story_columns(), window, cx);
    table.set_filters(filters, cx);
    table.set_rows(rows, cx);
    table
}

struct FilterTableStory {
    populated: Entity<FilterTable>,
    loading: Entity<FilterTable>,
    failed: Entity<FilterTable>,
    empty: Entity<FilterTable>,
    disabled: Entity<FilterTable>,
    selected: Entity<FilterTable>,
    constrained: Entity<FilterTable>,
    performance_only: bool,
    active_state: TableStoryState,
    rows: Vec<FilterRow>,
    #[cfg(feature = "performance")]
    performance_rows: Arc<[FilterRow]>,
    projections: HashMap<SharedString, FilterStoryProjection>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl FilterTableStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let rows = filter_story_rows();
        let populated_rows: Arc<[FilterRow]> = rows.clone().into();
        let populated = cx.new(|cx| {
            configured_filter_table(
                "gallery-filter-populated",
                "Live task filters",
                Progressive::complete(populated_rows),
                filter_story_definitions("all", &rows),
                window,
                cx,
            )
        });
        let loading = cx.new(|cx| {
            configured_filter_table(
                "gallery-filter-loading",
                "Loading filtered tasks",
                Progressive::running(Arc::from([])),
                filter_story_definitions("all", &rows),
                window,
                cx,
            )
        });
        let failed = cx.new(|cx| {
            configured_filter_table(
                "gallery-filter-error",
                "Unavailable filtered tasks",
                Progressive::failed(Arc::from([]), "Task service is unavailable"),
                filter_story_definitions("all", &rows),
                window,
                cx,
            )
        });
        let empty = cx.new(|cx| {
            configured_filter_table(
                "gallery-filter-empty",
                "Empty filtered tasks",
                Progressive::complete(Arc::from([])),
                filter_story_definitions("completed", &[]),
                window,
                cx,
            )
        });
        let disabled = cx.new(|cx| {
            configured_filter_table(
                "gallery-filter-disabled",
                "Disabled task row and filter",
                Progressive::complete(Arc::from([rows[0].clone().disabled(true)])),
                vec![
                    FilterDefinition::new("todo", "To do", 1)
                        .active(true)
                        .disabled(true),
                ],
                window,
                cx,
            )
        });
        let selected = cx.new(|cx| {
            let mut table = configured_filter_table(
                "gallery-filter-selected",
                "Selected filtered task",
                Progressive::complete(rows.clone().into()),
                filter_story_definitions("all", &rows),
                window,
                cx,
            );
            table.set_selected_row("batch", window, cx);
            table
        });
        let many_rows = filter_story_many_rows();
        let constrained = cx.new(|cx| {
            configured_filter_table(
                "gallery-filter-constrained",
                "Constrained filtered tasks",
                Progressive::complete(many_rows.clone()),
                filter_story_definitions("all", &many_rows),
                window,
                cx,
            )
        });

        let mut subscriptions = Vec::new();
        for table in [&populated, &selected] {
            subscriptions.push(cx.subscribe_in(
                table,
                window,
                |this, table, event: &FilterTableEvent, window, cx| {
                    this.last_event = format!("{event:?}").into();
                    match event {
                        FilterTableEvent::FilterRequested {
                            id,
                            filter_id: _,
                            active: _,
                        } => {
                            reduce_filter_story_projection(&mut this.projections, event);
                            let projection = this
                                .projections
                                .get(id)
                                .expect("the reducer must install projection state");
                            let definitions =
                                filter_story_definitions(&projection.active_filter, &this.rows);
                            let rows = filter_story_project_rows(&this.rows, projection);
                            let sort = projection
                                .sort_column
                                .clone()
                                .zip(projection.sort_direction);
                            table.update(cx, |table, cx| {
                                table.set_filters(definitions, cx);
                                table.set_rows(Progressive::complete(rows), cx);
                                if let Some((column_id, direction)) = sort {
                                    table.set_sort(column_id, Some(direction), window, cx);
                                }
                            });
                        }
                        FilterTableEvent::SelectionRequested { row_id, .. } => table
                            .update(cx, |table, cx| {
                                table.set_selected_row(row_id.clone(), window, cx)
                            }),
                        FilterTableEvent::SortRequested {
                            id,
                            column_id,
                            direction,
                        } => {
                            reduce_filter_story_projection(&mut this.projections, event);
                            let projection = this
                                .projections
                                .get(id)
                                .expect("the reducer must install projection state");
                            let rows = filter_story_project_rows(&this.rows, projection);
                            table.update(cx, |table, cx| {
                                table.set_rows(Progressive::complete(rows), cx);
                                table.set_sort(column_id.clone(), *direction, window, cx);
                            });
                        }
                        FilterTableEvent::ActivationRequested { .. } => {}
                    }
                    cx.notify();
                },
            ));
        }
        let projections = [
            SharedString::from("gallery-filter-populated"),
            SharedString::from("gallery-filter-selected"),
        ]
        .into_iter()
        .map(|id| (id, FilterStoryProjection::default()))
        .collect();
        Self {
            populated,
            loading,
            failed,
            empty,
            disabled,
            selected,
            constrained,
            performance_only: false,
            active_state: TableStoryState::Populated,
            rows,
            #[cfg(feature = "performance")]
            performance_rows: many_rows,
            projections,
            last_event: "Choose a status, row, action, or sort order.".into(),
            _subscriptions: subscriptions,
        }
    }

    #[cfg(feature = "performance")]
    fn set_performance_only(&mut self, cx: &mut Context<Self>) {
        if !self.performance_only {
            self.performance_only = true;
            cx.notify();
        }
    }

    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(state) = TableStoryState::ALL.get(index).copied() {
            self.active_state = state;
            cx.notify();
        }
    }

    #[cfg(feature = "performance")]
    fn set_performance_projection(&mut self, filtered: bool, cx: &mut Context<Self>) {
        self.set_performance_only(cx);
        let projection: Arc<[FilterRow]> = if filtered {
            self.performance_rows
                .iter()
                .filter(|row| {
                    row.cell("status")
                        .is_some_and(|cell| cell.value() == "Completed")
                })
                .cloned()
                .rev()
                .collect::<Vec<_>>()
                .into()
        } else {
            self.performance_rows.clone()
        };
        self.constrained.update(cx, |table, cx| {
            table.set_rows(Progressive::complete(projection), cx);
        });
    }

    #[cfg(feature = "performance")]
    fn performance_counts(&self, cx: &App) -> (usize, usize) {
        self.constrained.read_with(cx, |table, cx| {
            (table.visible_row_count(cx), table.animating_row_count(cx))
        })
    }

    fn state(
        selector: &'static str,
        title: &'static str,
        table: Entity<FilterTable>,
    ) -> impl IntoElement {
        v_flex()
            .id(selector)
            .debug_selector(move || selector.into())
            .flex_none()
            .gap_1()
            .child(
                div()
                    .id(format!("{selector}-heading"))
                    .role(Role::Heading)
                    .aria_label(title)
                    .text_xs()
                    .child(title),
            )
            .child(div().h(px(250.)).child(table))
    }
}

impl Render for FilterTableStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.performance_only {
            return Self::state(
                "filter-story-constrained",
                "1,000-row filter + reorder",
                self.constrained.clone(),
            )
            .into_any_element();
        }

        let switcher = story_state_switcher(
            cx.weak_entity(),
            "filter-story",
            TableStoryState::LABELS,
            self.active_state.index(),
            Self::set_active_state,
        );
        let active: AnyElement = match self.active_state {
            TableStoryState::Populated => Self::state(
                "filter-story-populated",
                "Populated, filterable, and sortable",
                self.populated.clone(),
            )
            .into_any_element(),
            TableStoryState::Loading => {
                Self::state("filter-story-loading", "Loading", self.loading.clone())
                    .into_any_element()
            }
            TableStoryState::Error => {
                Self::state("filter-story-error", "Error", self.failed.clone()).into_any_element()
            }
            TableStoryState::Empty => {
                Self::state("filter-story-empty", "Empty", self.empty.clone()).into_any_element()
            }
            TableStoryState::Disabled => Self::state(
                "filter-story-disabled",
                "Disabled filter and row",
                self.disabled.clone(),
            )
            .into_any_element(),
            TableStoryState::Selected => Self::state(
                "filter-story-selected",
                "Controlled selection",
                self.selected.clone(),
            )
            .into_any_element(),
            TableStoryState::Constrained => v_flex()
                .id("filter-story-constrained")
                .debug_selector(|| "filter-story-constrained".into())
                .flex_none()
                .gap_1()
                .child(
                    div()
                        .id("filter-story-constrained-heading")
                        .role(Role::Heading)
                        .aria_label("Constrained height and width")
                        .text_xs()
                        .child("Constrained height and width"),
                )
                .child(
                    div()
                        .w(px(520.))
                        .h(px(190.))
                        .child(self.constrained.clone()),
                )
                .into_any_element(),
        };

        v_flex()
            .gap_3()
            .child(switcher)
            .child(active)
            .child(
                TextView::markdown(
                    "filter-story-event-log",
                    format!("**Last typed event.** {}", self.last_event),
                )
                .selectable(true),
            )
            .child(
                div()
                    .id("filter-story-reference-note")
                    .debug_selector(|| "filter-story-reference-note".into())
                    .child(
                        TextView::markdown(
                            "filter-story-reference-copy",
                            "**Reference comparison.** Beautiful UI uses status chips to reorganize a compact task table. This original GPUI composition keeps filter definitions and ordered rows application-owned, adds stable IDs and typed intent, selectable cells, direct semantics, two-axis virtualization, and visible-only finite reorder motion that snaps under reduced motion.",
                        )
                        .selectable(true),
                    ),
            )
            .into_any_element()
    }
}

fn comparison_story_snapshot(item_states: &[ComparisonItemState]) -> ComparisonSnapshot {
    let labels = [
        "Starter",
        "Business",
        "Enterprise with dedicated regional support",
    ];
    let items = labels.into_iter().enumerate().map(|(index, label)| {
        ComparisonItem::new(format!("plan-{index}"), label)
            .description(format!("Plan {} details", index + 1))
            .state(
                item_states
                    .get(index)
                    .copied()
                    .unwrap_or(ComparisonItemState::Default),
            )
    });
    ComparisonSnapshot::try_new(
        items,
        [
            ComparisonFeature::new("price", "Monthly price").values([
                ComparisonValue::new("plan-0", "$12"),
                ComparisonValue::new("plan-1", "$24"),
                ComparisonValue::new("plan-2", "Custom"),
            ]),
            ComparisonFeature::new("seats", "Included team seats").values([
                ComparisonValue::new("plan-0", "3"),
                ComparisonValue::new("plan-1", "25"),
                ComparisonValue::new("plan-2", "Unlimited"),
            ]),
            ComparisonFeature::new("support", "Priority support").values([
                ComparisonValue::included("plan-0", false),
                ComparisonValue::included("plan-1", true),
                ComparisonValue::included("plan-2", true),
            ]),
        ],
    )
    .expect("gallery comparison fixture must satisfy the bounded contract")
}

fn configured_comparison_table(
    id: &str,
    label: &str,
    snapshot: Progressive<ComparisonSnapshot>,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut Context<ComparisonTable>,
) -> ComparisonTable {
    let mut table = ComparisonTable::new(id, label, window, cx);
    table.set_snapshot(snapshot, window, cx);
    if let Some(selected) = selected {
        table.set_selected_item(selected, window, cx);
    }
    table
}

struct ComparisonTableStory {
    populated: Entity<ComparisonTable>,
    loading: Entity<ComparisonTable>,
    failed: Entity<ComparisonTable>,
    empty: Entity<ComparisonTable>,
    disabled: Entity<ComparisonTable>,
    selected: Entity<ComparisonTable>,
    /// Built lazily: the maximum 12×128 grid costs hundreds of milliseconds
    /// per draw and is only materialized when its state is selected.
    constrained: Option<Entity<ComparisonTable>>,
    active_state: TableStoryState,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl ComparisonTableStory {
    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(state) = TableStoryState::ALL.get(index).copied() else {
            return;
        };
        self.active_state = state;
        cx.notify();
    }
}

impl ComparisonTableStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let ordinary = comparison_story_snapshot(&[
            ComparisonItemState::Default,
            ComparisonItemState::Highlighted,
            ComparisonItemState::Default,
        ]);
        let populated = cx.new(|cx| {
            configured_comparison_table(
                "gallery-comparison-populated",
                "Plan comparison",
                Progressive::complete(ordinary.clone()),
                None,
                window,
                cx,
            )
        });
        let loading = cx.new(|cx| {
            configured_comparison_table(
                "gallery-comparison-loading",
                "Loading plan comparison",
                Progressive::running(ordinary.clone()),
                None,
                window,
                cx,
            )
        });
        let failed = cx.new(|cx| {
            configured_comparison_table(
                "gallery-comparison-error",
                "Unavailable plan comparison",
                Progressive::failed(ordinary.clone(), "Pricing service is unavailable"),
                None,
                window,
                cx,
            )
        });
        let empty_snapshot = ComparisonSnapshot::try_new([], [])
            .expect("an empty gallery comparison is structurally valid");
        let empty = cx.new(|cx| {
            configured_comparison_table(
                "gallery-comparison-empty",
                "Empty plan comparison",
                Progressive::complete(empty_snapshot),
                None,
                window,
                cx,
            )
        });
        let disabled_snapshot = comparison_story_snapshot(&[
            ComparisonItemState::Default,
            ComparisonItemState::Highlighted,
            ComparisonItemState::Disabled,
        ]);
        let disabled = cx.new(|cx| {
            configured_comparison_table(
                "gallery-comparison-disabled",
                "Disabled plan comparison",
                Progressive::complete(disabled_snapshot),
                None,
                window,
                cx,
            )
        });
        let selected = cx.new(|cx| {
            configured_comparison_table(
                "gallery-comparison-selected",
                "Selected plan comparison",
                Progressive::complete(ordinary.clone()),
                Some("plan-1"),
                window,
                cx,
            )
        });

        let mut subscriptions = Vec::new();
        for table in [&populated, &selected] {
            subscriptions.push(cx.subscribe_in(
                table,
                window,
                |this, table, event: &ComparisonTableEvent, window, cx| {
                    let ComparisonTableEvent::SelectionRequested { item_id, .. } = event;
                    this.last_event = format!("{event:?}").into();
                    table.update(cx, |table, cx| {
                        table.set_selected_item(item_id.clone(), window, cx);
                    });
                    cx.notify();
                },
            ));
        }

        Self {
            populated,
            loading,
            failed,
            empty,
            disabled,
            selected,
            constrained: None,
            active_state: TableStoryState::Populated,
            last_event: "Choose a plan.".into(),
            _subscriptions: subscriptions,
        }
    }

    fn state(
        selector: &'static str,
        title: &'static str,
        table: Entity<ComparisonTable>,
    ) -> impl IntoElement {
        v_flex()
            .id(selector)
            .debug_selector(move || selector.into())
            .flex_none()
            .gap_1()
            .child(
                div()
                    .id(format!("{selector}-heading"))
                    .role(Role::Heading)
                    .aria_label(title)
                    .text_xs()
                    .child(title),
            )
            .child(div().h(px(220.)).child(table))
    }
}

/// Builds the maximum 12-item by 128-feature gallery snapshot on demand.
fn comparison_story_max_grid() -> ComparisonSnapshot {
    let wide_items = (0..12).map(|index| {
        ComparisonItem::new(
            format!("wide-{index}"),
            format!("Regional plan {} with a long name", index + 1),
        )
        .state(if index == 9 {
            ComparisonItemState::Highlighted
        } else {
            ComparisonItemState::Default
        })
    });
    ComparisonSnapshot::try_new(
        wide_items,
        (0..128).map(|feature_index| {
            ComparisonFeature::new(
                format!("wide-feature-{feature_index}"),
                format!("Regional capability {}", feature_index + 1),
            )
            .description("Selectable supporting detail for this capability")
            .values((0..12).map(|item_index| {
                ComparisonValue::new(
                    format!("wide-{item_index}"),
                    format!("Tier {}", feature_index + item_index + 1),
                )
            }))
        }),
    )
    .expect("the maximum-width gallery comparison must remain bounded")
}

impl Render for ComparisonTableStory {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let owner = cx.weak_entity();
        let switcher = story_state_switcher(
            owner,
            "comparison-story",
            TableStoryState::LABELS,
            self.active_state.index(),
            Self::set_active_state,
        );
        let active: AnyElement = match self.active_state {
            TableStoryState::Populated => Self::state(
                "comparison-story-populated",
                "Populated and highlighted",
                self.populated.clone(),
            )
            .into_any_element(),
            TableStoryState::Loading => {
                Self::state("comparison-story-loading", "Loading", self.loading.clone())
                    .into_any_element()
            }
            TableStoryState::Error => {
                Self::state("comparison-story-error", "Error", self.failed.clone())
                    .into_any_element()
            }
            TableStoryState::Empty => {
                Self::state("comparison-story-empty", "Empty", self.empty.clone())
                    .into_any_element()
            }
            TableStoryState::Disabled => Self::state(
                "comparison-story-disabled",
                "Disabled item",
                self.disabled.clone(),
            )
            .into_any_element(),
            TableStoryState::Selected => Self::state(
                "comparison-story-selected",
                "Controlled selection",
                self.selected.clone(),
            )
            .into_any_element(),
            TableStoryState::Constrained => {
                let constrained = self.constrained.clone().unwrap_or_else(|| {
                    let snapshot = comparison_story_max_grid();
                    cx.new(|cx| {
                        configured_comparison_table(
                            "gallery-comparison-constrained",
                            "Constrained plan comparison",
                            Progressive::complete(snapshot),
                            None,
                            window,
                            cx,
                        )
                    })
                });
                self.constrained = Some(constrained.clone());
                v_flex()
                    .id("comparison-story-constrained")
                    .debug_selector(|| "comparison-story-constrained".into())
                    .gap_1()
                    .child(
                        div()
                            .id("comparison-story-constrained-heading")
                            .role(Role::Heading)
                            .aria_label("Constrained width and height")
                            .text_xs()
                            .child("Constrained width and height"),
                    )
                    .child(div().w(px(520.)).h(px(190.)).child(constrained))
                    .into_any_element()
            }
        };

        v_flex()
            .gap_3()
            .child(switcher)
            .child(active)
            .child(
                TextView::markdown(
                    "comparison-story-event-log",
                    format!("**Last typed event.** {}", self.last_event),
                )
                .selectable(true),
            )
            .child(
                div()
                    .id("comparison-story-reference-note")
                    .debug_selector(|| "comparison-story-reference-note".into())
                    .child(
                        TextView::markdown(
                            "comparison-story-reference-copy",
                            "**Reference comparison.** Beautiful UI establishes a compact feature-by-plan layout. This original GPUI composition deliberately validates a bounded 12-item by 128-feature snapshot, keeps highlighting and selection application-owned, exposes stable typed intent and direct table semantics, preserves selectable values, and provides keyboard-driven horizontal reachability.",
                        )
                        .selectable(true),
                    ),
            )
    }
}

/// Stateful component gallery shared by native and web launchers.
pub struct Gallery {
    selected: StoryId,
    catalog_list: ListState,
    insight_scroll: ScrollHandle,
    prompt_bar_scroll: ScrollHandle,
    selection_actions_scroll: ScrollHandle,
    records_table_scroll: ScrollHandle,
    diff_table_scroll: ScrollHandle,
    filter_table_scroll: ScrollHandle,
    comparison_table_scroll: ScrollHandle,
    #[cfg(feature = "performance")]
    performance_filter_story: Option<WeakEntity<FilterTableStory>>,
    #[cfg(feature = "performance")]
    performance_viewport: Option<StoryId>,
    visible_range: Range<usize>,
    simulation_task: Option<Task<()>>,
    sim: sim::Simulation,
    trace_open: bool,
    tool_call_open: HashMap<SharedString, bool>,
    tool_group_open: Option<bool>,
    tool_approval: ToolApproval,
    last_suggestion: Option<SharedString>,
    theme: GalleryTheme,
    /// Active middle-click autoscroll session (catalog view only).
    autoscroll: Option<Autoscroll>,
    /// Wheel acceleration state for the catalog feed.
    wheel: WheelAccelerator,
    /// Frame driver for an active autoscroll session.
    #[cfg(any(test, feature = "performance"))]
    scan_simulation_suspended: bool,
}

/// One middle-click autoscroll gesture anchored to a window position.
impl Gallery {
    /// Creates the gallery for one selected story or the complete catalog.
    pub fn new(selected: StoryId, cx: &mut Context<Self>) -> Self {
        Self::new_with_theme(selected, None, cx)
    }

    fn new_with_theme(
        selected: StoryId,
        theme: Option<GalleryTheme>,
        cx: &mut Context<Self>,
    ) -> Self {
        let catalog_list = ListState::new(StoryId::ALL.len(), ListAlignment::Top, px(320.))
            .with_uniform_item_height(px(320.));
        let gallery = cx.weak_entity();
        catalog_list.set_scroll_handler(move |event, _, cx| {
            gallery
                .update(cx, |gallery, cx| {
                    gallery.update_visible_range(event.visible_range.clone(), cx);
                })
                .ok();
        });

        let visible_range = 0..1;
        let simulation_needed = if selected == StoryId::All {
            visible_range_needs_simulation(visible_range.clone())
        } else {
            story_needs_simulation(selected)
        };

        Self {
            selected,
            catalog_list,
            insight_scroll: ScrollHandle::new(),
            prompt_bar_scroll: ScrollHandle::new(),
            selection_actions_scroll: ScrollHandle::new(),
            records_table_scroll: ScrollHandle::new(),
            diff_table_scroll: ScrollHandle::new(),
            filter_table_scroll: ScrollHandle::new(),
            comparison_table_scroll: ScrollHandle::new(),
            #[cfg(feature = "performance")]
            performance_filter_story: None,
            #[cfg(feature = "performance")]
            performance_viewport: None,
            visible_range,
            simulation_task: simulation_needed.then(|| Self::spawn_simulation(cx)),
            sim: sim::Simulation::new(),
            trace_open: true,
            tool_call_open: HashMap::new(),
            tool_group_open: None,
            tool_approval: ToolApproval::Requested,
            last_suggestion: None,
            theme: theme.unwrap_or_else(|| {
                if cx.theme().is_dark() {
                    GalleryTheme::Dark
                } else {
                    GalleryTheme::Light
                }
            }),
            autoscroll: None,
            wheel: WheelAccelerator::new(),
            #[cfg(any(test, feature = "performance"))]
            scan_simulation_suspended: false,
        }
    }

    fn spawn_simulation(cx: &Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(sim::TICK_INTERVAL).await;
                let alive = this.update(cx, |this, cx| {
                    let delta = this.sim.tick();
                    if this.visible_stories_changed(delta) {
                        this.remeasure_visible_stories(delta);
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    fn update_visible_range(&mut self, visible_range: Range<usize>, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }

        self.visible_range = visible_range;
        #[cfg(any(test, feature = "performance"))]
        let simulation_needed = !self.scan_simulation_suspended
            && visible_range_needs_simulation(self.visible_range.clone());
        #[cfg(not(any(test, feature = "performance")))]
        let simulation_needed = visible_range_needs_simulation(self.visible_range.clone());
        match (simulation_needed, self.simulation_task.is_some()) {
            (true, false) => self.simulation_task = Some(Self::spawn_simulation(cx)),
            (false, true) => self.simulation_task = None,
            _ => {}
        }
    }

    fn visible_stories_changed(&self, delta: sim::SimulationDelta) -> bool {
        if self.selected != StoryId::All {
            return story_changed_by_delta(self.selected, delta);
        }

        StoryId::ALL
            .get(self.visible_range.clone())
            .is_some_and(|stories| {
                stories
                    .iter()
                    .copied()
                    .any(|story| story_changed_by_delta(story, delta))
            })
    }

    fn remeasure_visible_stories(&self, delta: sim::SimulationDelta) {
        if self.selected != StoryId::All {
            return;
        }

        for index in self.visible_range.clone() {
            if StoryId::ALL
                .get(index)
                .copied()
                .is_some_and(|story| story_changed_by_delta(story, delta))
            {
                self.catalog_list.remeasure_items(index..index + 1);
            }
        }
    }

    /// Updates the displayed preset after a gallery host changes the theme.
    pub fn set_theme_preset(&mut self, theme: GalleryTheme, cx: &mut Context<Self>) {
        self.theme = theme;
        cx.notify();
    }

    /// Moves the complete catalog to a representative story.
    ///
    /// Native performance tooling uses this to measure different catalog
    /// regions without relying on platform-specific synthetic wheel input.
    #[cfg(any(test, feature = "performance"))]
    pub fn scroll_catalog_to(&mut self, story: StoryId, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        let Some(index) = StoryId::ALL
            .iter()
            .position(|candidate| *candidate == story)
        else {
            return;
        };

        self.catalog_list.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: px(0.),
        });
        self.update_visible_range(index..(index + 3).min(StoryId::ALL.len()), cx);
        cx.notify();
    }

    /// Advances the catalog scan by `distance` and reports whether it moved.
    ///
    /// Catalog-scan harness seam: scrolls the shared list, re-derives the
    /// simulated visible range exactly as a real scroll would, and requests a
    /// redraw. Returns `false` once the list can no longer advance.
    #[cfg(any(test, feature = "performance"))]
    pub fn advance_catalog_scan(&mut self, distance: Pixels, cx: &mut Context<Self>) -> bool {
        if self.selected != StoryId::All {
            return false;
        }
        let before = self.catalog_list.logical_scroll_top();
        self.catalog_list.scroll_by(distance);
        let after = self.catalog_list.logical_scroll_top();
        let moved =
            after.item_ix != before.item_ix || after.offset_in_item != before.offset_in_item;
        // Always request a redraw: scrolling into unmeasured content saturates
        // `scroll_by` at the measured frontier, and only a subsequent layout
        // pass measures new items and extends the reachable range.
        let first = after.item_ix;
        self.update_visible_range(first..(first + 3).min(StoryId::ALL.len()), cx);
        cx.notify();
        moved
    }

    /// Story region the catalog currently focuses, for scan attribution.
    ///
    /// The scan measures whole frames, so a frame is attributed to the region
    /// containing the catalog's scroll top rather than the single story that
    /// happens to intersect the viewport center.
    #[cfg(any(test, feature = "performance"))]
    pub fn scan_focus_region(&self) -> usize {
        if self.selected != StoryId::All {
            return 0;
        }
        self.catalog_list.logical_scroll_top().item_ix
    }

    /// Suspends or resumes the simulated agent activity for the catalog scan.
    /// Simulated streams change row heights while the scan scrolls, which
    /// pins the virtual-list anchor and races the scripted traversal.
    /// Suspending the simulation makes the scan deterministic; the flag must
    /// be cleared before handing the gallery back to interactive review.
    #[cfg(any(test, feature = "performance"))]
    pub fn set_scan_simulation_suspended(&mut self, suspended: bool, cx: &mut Context<Self>) {
        self.scan_simulation_suspended = suspended;
        if suspended {
            self.simulation_task = None;
        } else if self.simulation_task.is_none()
            && visible_range_needs_simulation(self.visible_range.clone())
        {
            self.simulation_task = Some(Self::spawn_simulation(cx));
        }
    }

    /// Scrolls the catalog by one page in `direction` (keyboard paging).
    pub fn scroll_catalog_page(&mut self, direction: f32, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        self.catalog_list
            .scroll_by(px(direction * PAGE_FRACTION * 320.));
        let first = self.catalog_list.logical_scroll_top().item_ix;
        self.update_visible_range(first..(first + 3).min(StoryId::ALL.len()), cx);
        cx.notify();
    }

    /// Jumps the catalog to its start or end (Home/End keys).
    pub fn scroll_catalog_edge(&mut self, to_end: bool, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        if to_end {
            self.catalog_list.scroll_to_end();
            let first = self.catalog_list.logical_scroll_top().item_ix;
            self.update_visible_range(first..(first + 3).min(StoryId::ALL.len()), cx);
            cx.notify();
        } else {
            self.catalog_list.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(0.),
            });
            self.update_visible_range(0..3.min(StoryId::ALL.len()), cx);
            cx.notify();
        }
    }

    /// Handles a wheel event over the catalog feed.
    ///
    /// Discrete `Lines` notches get cadence acceleration from the library's
    /// [`WheelAccelerator`], applied on top of the base scroll the `List`
    /// element already performed. Trackpad `Pixels` deltas pass through
    /// untouched.
    fn handle_catalog_wheel(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        let ScrollDelta::Lines(lines) = event.delta else {
            return;
        };
        let boost = self.wheel.line_notch(lines.y, event.modifiers.alt);
        if boost != Pixels::ZERO {
            self.catalog_list.scroll_by(boost);
            cx.notify();
        }
    }

    /// Begins a middle-click autoscroll session at the pointer anchor.
    ///
    /// Spawns a frame-paced task that scrolls toward the latest pointer
    /// position until the session is cancelled.
    pub fn start_autoscroll(&mut self, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        if self.autoscroll.replace(Autoscroll::start(anchor)).is_none() {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor()
                        .timer(Duration::from_millis(16))
                        .await;
                    let alive = this.update(cx, |gallery, cx| {
                        if gallery.autoscroll.is_none() {
                            return false;
                        }
                        gallery.tick_autoscroll(0.016, cx);
                        gallery.autoscroll.is_some()
                    });
                    if alive.is_err() || !alive.unwrap_or(false) {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.notify();
    }

    /// Records the current pointer position for an active autoscroll session.
    pub fn track_autoscroll_pointer(&mut self, position: Point<Pixels>, _cx: &mut Context<Self>) {
        if let Some(session) = self.autoscroll.as_mut() {
            session.track(position);
        }
    }

    /// Ends any active autoscroll session (middle click again, Escape, click).
    pub fn cancel_autoscroll(&mut self, cx: &mut Context<Self>) {
        if self.autoscroll.take().is_some() {
            cx.notify();
        }
    }

    /// Advances autoscroll by `delta_seconds` toward the tracked pointer.
    ///
    /// Distance comes from the library's [`Autoscroll`] speed curve; the
    /// gallery only applies it to its list.
    fn tick_autoscroll(&mut self, delta_seconds: f32, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        let Some(session) = &mut self.autoscroll else {
            return;
        };
        let distance = session.tick(delta_seconds);
        if distance == Pixels::ZERO {
            return;
        }
        self.catalog_list.scroll_by(distance);
        let first = self.catalog_list.logical_scroll_top().item_ix;
        self.update_visible_range(first..(first + 3).min(StoryId::ALL.len()), cx);
        cx.notify();
    }

    /// Whether an autoscroll session is currently active.
    pub fn autoscroll_active(&self) -> bool {
        self.autoscroll.is_some()
    }

    /// Scrolls the catalog to its absolute end for the scan's tail phase.
    ///
    /// The deepest reachable scroll top clamps when the remaining content is
    /// shorter than the viewport, so the final stories can never present at
    /// scroll top; this seam guarantees the catalog bottom is on screen.
    #[cfg(any(test, feature = "performance"))]
    pub fn scroll_catalog_to_end(&mut self, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        self.catalog_list.scroll_to_end();
        let first = self.catalog_list.logical_scroll_top().item_ix;
        self.update_visible_range(first..(first + 3).min(StoryId::ALL.len()), cx);
        cx.notify();
    }

    /// Prepares one isolated native performance viewport from a stable story identity.
    ///
    /// The dedicated Streaming Text viewport measures ongoing progressive
    /// invalidation. Chat therefore stops only the gallery-owned fake producer
    /// after its transition so its equal steady sample window measures the
    /// populated virtual transcript and composer rather than duplicating the
    /// streaming scenario.
    #[cfg(any(test, feature = "performance"))]
    pub fn prepare_performance_viewport(&mut self, story: StoryId, cx: &mut Context<Self>) {
        #[cfg(feature = "performance")]
        {
            self.performance_viewport = Some(story);
            if story == StoryId::FilterTable
                && let Some(filter_story) = self
                    .performance_filter_story
                    .as_ref()
                    .and_then(WeakEntity::upgrade)
            {
                filter_story.update(cx, FilterTableStory::set_performance_only);
            }
        }
        self.selected = story;
        let simulation_needed = story_needs_simulation(story);
        match (simulation_needed, self.simulation_task.is_some()) {
            (true, false) => self.simulation_task = Some(Self::spawn_simulation(cx)),
            (false, true) => self.simulation_task = None,
            _ => {}
        }
        if story == StoryId::FilterTable {
            self.filter_table_scroll.scroll_to_bottom();
        }
        if story == StoryId::Chat {
            self.simulation_task.take();
        }
        cx.notify();
    }

    /// Replaces the controlled 1,000-row performance projection.
    #[cfg(feature = "performance")]
    pub fn set_performance_filter_projection(&mut self, filtered: bool, cx: &mut Context<Self>) {
        if let Some(story) = self
            .performance_filter_story
            .as_ref()
            .and_then(WeakEntity::upgrade)
        {
            story.update(cx, |story, cx| {
                story.set_performance_projection(filtered, cx);
            });
        }
    }

    /// Returns visible constructed rows and rows currently carrying motion state.
    #[cfg(feature = "performance")]
    pub fn performance_filter_counts(&self, cx: &App) -> Option<(usize, usize)> {
        self.performance_filter_story
            .as_ref()
            .and_then(WeakEntity::upgrade)
            .map(|story| story.read_with(cx, FilterTableStory::performance_counts))
    }

    fn shows(&self, story: StoryId) -> bool {
        self.selected == StoryId::All || self.selected == story
    }

    fn section<E>(
        &self,
        story: StoryId,
        title: &'static str,
        content: impl FnOnce() -> E,
        cx: &Context<Self>,
    ) -> AnyElement
    where
        E: IntoElement,
    {
        if !self.shows(story) {
            return div().hidden().into_any_element();
        }

        story_frame(story, self.selected == StoryId::All)
            .debug_selector(move || format!("story-{}", story.slug()))
            .w_full()
            .max_w(px(640.))
            .px_6()
            .py_4()
            .gap_3()
            .child(
                div()
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(title),
            )
            .child(content())
            .into_any_element()
    }

    fn render_catalog_story(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(story) = StoryId::ALL.get(index).copied() else {
            return div().hidden().into_any_element();
        };
        self.render_story(story, window, cx)
    }

    fn handle_tool_call_event(
        &mut self,
        event: &ToolCallEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ToolCallEvent::Toggled { id, open } => {
                self.tool_call_open.insert(id.clone(), *open);
            }
            ToolCallEvent::Approved { .. } => self.tool_approval = ToolApproval::Approved,
            ToolCallEvent::Rejected { .. } => self.tool_approval = ToolApproval::Rejected,
        }
        cx.notify();
    }

    fn render_story(
        &mut self,
        story: StoryId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let elapsed = self.sim.elapsed();

        match story {
            StoryId::All => div().hidden().into_any_element(),
            StoryId::Loading => self.section(
                story,
                "Loading state",
                || {
                    LoadingState::new()
                        .label("Reasoning about supplier pricing")
                        .elapsed(elapsed)
                },
                cx,
            ),
            StoryId::ToolChips => self.section(
                story,
                "Tool chips",
                || {
                    h_flex()
                        .flex_wrap()
                        .gap_2()
                        .child(
                            ToolChip::new("chip-1", "read pricing.md").status(ToolStatus::Success),
                        )
                        .child(
                            ToolChip::new("chip-2", "edit suppliers.rs")
                                .status(ToolStatus::Running)
                                .detail("+12 −3"),
                        )
                        .child(
                            ToolChip::new("chip-3", "run cargo test").status(ToolStatus::Pending),
                        )
                        .child(
                            ToolChip::new("chip-4", "query prices-db")
                                .status(ToolStatus::Failed)
                                .detail("timeout"),
                        )
                },
                cx,
            ),
            StoryId::Tasks => self.section(
                story,
                "Task rows",
                || {
                    v_flex()
                        .child(TaskRow::new(&Progressive::complete(
                            TaskSnapshot::new("index-catalog", "Index supplier catalog")
                                .detail("3,214 rows"),
                        )))
                        .child(TaskRow::new(&Progressive::running(
                            TaskSnapshot::new("compare-prices", "Compare unit prices")
                                .elapsed(elapsed),
                        )))
                        .child(TaskRow::new(&Progressive::pending(TaskSnapshot::new(
                            "draft-emails",
                            "Draft supplier emails",
                        ))))
                        .child(TaskRow::new(&Progressive::failed(
                            TaskSnapshot::new("sync-history", "Sync order history"),
                            "auth expired",
                        )))
                },
                cx,
            ),
            StoryId::Thinking => self.section(
                story,
                "Thinking",
                || {
                    let trace = ThinkingTrace::new()
                        .thought_for(Duration::from_secs(9))
                        .steps([
                            ThinkingStep::new("Reading the supplier schema")
                                .status(StepStatus::Done),
                            ThinkingStep::new("Comparing unit prices")
                                .detail("Alpenrose is **7% cheaper** at equal volume.")
                                .status(StepStatus::Done),
                            ThinkingStep::new("Checking delivery constraints"),
                        ]);
                    let trace = if self.sim.answer.is_streaming() {
                        Progressive::running(trace)
                    } else {
                        Progressive::complete(trace)
                    };
                    v_flex()
                        .gap_8()
                        .child(
                            Thinking::new("trace", &trace)
                                .open(self.trace_open)
                                .on_event(cx.listener(
                                    |this, event: &ThinkingEvent, _, cx| {
                                        let ThinkingEvent::Toggled { open, .. } = event;
                                        this.trace_open = *open;
                                        cx.notify();
                                    },
                                )),
                        )
                        .child(
                            v_flex()
                                .gap_3()
                                .child(
                                    div()
                                        .text_xs()
                                        .font_semibold()
                                        .text_color(cx.theme().muted_foreground)
                                        .child("THINKING — PROSE"),
                                )
                                .child(Thinking::new(
                                    "trace-prose",
                                    &Progressive::complete(
                                        ThinkingTrace::new()
                                            .thought_for(Duration::from_secs(4))
                                            .prose(
                                                "The March sheet supersedes prior quotes, so the \
                                                 comparison should use *current* volume tiers. \
                                                 Delivery windows match, which removes the main \
                                                 switching risk.",
                                            ),
                                    ),
                                )
                                .open(true)),
                        )
                },
                cx,
            ),
            StoryId::Orbs => self.section(
                story,
                "Orbs",
                || {
                    let tokens = cx.theme().semantic_tokens();
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            h_flex()
                                .items_center()
                                .flex_wrap()
                                .gap(tokens.spacing.lg)
                                .children(OrbVariant::ALL.map(|variant| {
                                    v_flex()
                                        .items_center()
                                        .gap(tokens.spacing.xs)
                                        .child(
                                            div()
                                                .id(ElementId::NamedInteger(
                                                    format!("orbs-variant-{}", variant.slug())
                                                        .into(),
                                                    0,
                                                ))
                                                .p(tokens.spacing.md)
                                                .rounded(tokens.radius.lg)
                                                .border_1()
                                                .border_color(cx.theme().border)
                                                .bg(tokens.colors.surface)
                                                .child(
                                                    Orbs::new()
                                                        .variant(variant)
                                                        .diameter(px(56.)),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .text_xs()
                                                .text_color(cx.theme().muted_foreground)
                                                .child(match variant {
                                                    OrbVariant::Radial => "Radial",
                                                    OrbVariant::Diagonal => "Diagonal",
                                                    OrbVariant::Comet => "Comet",
                                                    OrbVariant::Column => "Column",
                                                    OrbVariant::Scattered => "Scattered",
                                                }),
                                        )
                                })),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("ambient thinking indicator — pick the choreography that matches your product's voice"),
                        )
                },
                cx,
            ),
            StoryId::Search => self.section(
                story,
                "Web search",
                || {
                    SearchResults::new("search", "alpenrose wholesale pricing")
                        .searching(self.sim.answer.is_streaming())
                        .results(if self.sim.answer.is_streaming() {
                            Vec::new()
                        } else {
                            vec![
                                SearchResult::new(
                                    "alpenrose-wholesale",
                                    "Alpenrose Dairy — Wholesale Programs",
                                )
                                .domain("alpenrose.com"),
                                SearchResult::new(
                                    "dairy-price-index",
                                    "2026 Dairy Supplier Price Index",
                                )
                                .domain("dairyreport.org"),
                                SearchResult::new(
                                    "portland-distributors",
                                    "Portland-area distributors compared",
                                )
                                .domain("nwfoodtrade.com"),
                            ]
                        })
                        .on_event(cx.listener(|_, event: &SearchResultsEvent, _, _| {
                            println!("search event: {event:?}");
                        }))
                },
                cx,
            ),
            StoryId::Todos => self.section(
                story,
                "To-do list",
                || {
                    TodoList::new("plan")
                        .title("Supplier switch plan")
                        .items([
                            TodoItem::new("contract-terms", "Pull current contract terms").done(),
                            TodoItem::new("compare-prices", "Compare Q3 unit prices").done(),
                            TodoItem::new("draft-timeline", "Draft transition timeline")
                                .status(TodoStatus::Active),
                            TodoItem::new("cold-chain", "Confirm cold-chain capacity"),
                            TodoItem::new("confirm-orders", "Send order confirmations"),
                        ])
                        .on_event(cx.listener(|_, event: &TodoListEvent, _, _| {
                            println!("todo event: {event:?}");
                        }))
                },
                cx,
            ),
            StoryId::ImageGeneration => self.section(
                story,
                "Image generation",
                || {
                    // The demo placeholder must not read as a partially
                    // rendered image: a flat muted canvas behind the pulsing
                    // icon keeps the progress state honest until pixels
                    // arrive, matching Beautiful UI / AICSS treatments.
                    ImageGeneration::new("gen")
                        .label("Label sketch: alpine meadow, morning light")
                        .progress(self.sim.progress())
                },
                cx,
            ),
            StoryId::StreamingText => self.section(
                story,
                "Streaming text",
                || {
                    let cited_answer = Progressive::complete(
                        "Pistachio margins lead vanilla by eight points [[cite:margin-report]], while the supply forecast remains stable [[cite:supply-forecast]]."
                            .into(),
                    );
                    v_flex()
                        .gap_4()
                        .child(
                            TextView::markdown(
                                "streaming-text-reference-note",
                                "**Reference comparison.** Beautiful UI pairs an inline `scoopdata.io` source with a separate 10-source footer and follow-ups. gpui-ai deliberately adds stable typed citation routing and keyboard/AccessKit companion links because the pinned Markdown glyph link is pointer-only.",
                            )
                            .selectable(true),
                        )
                        .child(
                            StreamingText::new("answer", &self.sim.answer)
                                .sources(["pricing.md", "suppliers.csv", "orders 2026"])
                                .follow_ups([
                                    FollowUp::new(
                                        "compare-delivery",
                                        "Compare delivery times",
                                    ),
                                    FollowUp::new("price-history", "Show price history"),
                                ])
                                .on_event(cx.listener(
                                    |_, event: &StreamingTextEvent, _, _| {
                                        println!("streaming-text event: {event:?}");
                                    },
                                )),
                        )
                        .child(
                            StreamingText::new("cited-answer", &cited_answer)
                                .citations([
                                    CitationRef::new(
                                        "margin-report",
                                        "Margin report",
                                        "Open the monthly margin report",
                                        "app://reports/monthly-margin",
                                    ),
                                    CitationRef::new(
                                        "supply-forecast",
                                        "Supply forecast",
                                        "Open the supply forecast",
                                        "app://reports/supply-forecast",
                                    ),
                                ])
                                .sources(["margin-report.csv", "supply-forecast.md"])
                                .on_event(cx.listener(
                                    |_, event: &StreamingTextEvent, _, _| {
                                        println!("citation event: {event:?}");
                                    },
                                )),
                        )
                },
                cx,
            ),
            StoryId::Chat => {
                let answer = self.sim.answer.clone();
                let chat_story = window.use_keyed_state(
                    "chat-story-state",
                    cx,
                    ChatStory::new,
                );
                chat_story.update(cx, |story, cx| {
                    let _ = story.set_answer(answer, window, cx);
                });
                self.section(
                    story,
                    "Chat",
                    || chat_story,
                    cx,
                )
            }
            StoryId::Suggestions => self.section(
                story,
                "Suggestions",
                || {
                    let tokens = cx.theme().semantic_tokens();
                    let status: SharedString = match &self.last_suggestion {
                        Some(id) => format!("Selected: {id}").into(),
                        None => "Choose a suggestion".into(),
                    };
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            Suggestions::new("starter-suggestions")
                                .items([
                                    Suggestion::new("compare", "Compare supplier prices")
                                        .description("Sends the prompt immediately"),
                                    Suggestion::new("risk", "Explain this week's delivery risk"),
                                    Suggestion::new("draft", "Draft the order confirmations"),
                                    Suggestion::new("history", "Show the price history"),
                                ])
                                .on_event(cx.listener(|this, event: &SuggestionsEvent, _, cx| {
                                    let SuggestionsEvent::Selected { id } = event;
                                    this.last_suggestion = Some(id.clone());
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("suggestions-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                        .child(
                            TextView::markdown(
                                "suggestions-reference-note",
                                "**Reference comparison.** AI Elements and assistant-ui show starter prompts as a single scrolling row that sends on click. gpui-ai wraps chips onto the available width, ripples them in with a staggered reveal, keeps every chip a named keyboard-reachable button, and reports a stable ID so the application decides whether to send or merely fill the composer.",
                            )
                            .selectable(true),
                        )
                },
                cx,
            ),
            StoryId::CommandSearch => {
                let command_story = window.use_keyed_state(
                    "command-search-story-state",
                    cx,
                    CommandSearchStory::new,
                );
                self.section(
                    story,
                    "Command search",
                    || command_story,
                    cx,
                )
            }
            StoryId::SidebarNav => {
                let sidebar_story = window.use_keyed_state(
                    "sidebar-nav-story-state",
                    cx,
                    SidebarNavStory::new,
                );
                self.section(story, "Sidebar navigation", || sidebar_story, cx)
            }
            StoryId::FineTune => {
                let fine_tune_story = window.use_keyed_state(
                    "fine-tune-story-state",
                    cx,
                    FineTuneStory::new,
                );
                self.section(story, "Fine-tune card", || fine_tune_story, cx)
            }
            StoryId::RecordsTable => {
                let records_story = window.use_keyed_state(
                    "records-table-story-state",
                    cx,
                    RecordsTableStory::new,
                );
                self.section(
                    story,
                    "Records table",
                    || {
                        v_flex()
                            .id("records-table-story-scroll")
                            .debug_selector(|| "records-table-story-scroll".into())
                            .h(px(256.))
                            .max_h(px(256.))
                            .flex_none()
                            .track_scroll(&self.records_table_scroll)
                            .overflow_y_scrollbar()
                            .child(records_story)
                    },
                    cx,
                )
            }
            StoryId::DiffTable => {
                let diff_story = window.use_keyed_state(
                    "diff-table-story-state",
                    cx,
                    DiffTableStory::new,
                );
                self.section(
                    story,
                    "Diff table",
                    || {
                        v_flex()
                            .id("diff-table-story-scroll")
                            .debug_selector(|| "diff-table-story-scroll".into())
                            .h(px(256.))
                            .max_h(px(256.))
                            .flex_none()
                            .track_scroll(&self.diff_table_scroll)
                            .overflow_y_scrollbar()
                            .child(diff_story)
                    },
                    cx,
                )
            }
            StoryId::FilterTable => {
                let filter_story = window.use_keyed_state(
                    "filter-table-story-state",
                    cx,
                    FilterTableStory::new,
                );
                #[cfg(feature = "performance")]
                {
                    self.performance_filter_story = Some(filter_story.downgrade());
                    if self.performance_viewport == Some(StoryId::FilterTable) {
                        filter_story.update(cx, FilterTableStory::set_performance_only);
                    }
                }
                self.section(
                    story,
                    "Filter table",
                    || {
                        v_flex()
                            .id("filter-table-story-scroll")
                            .debug_selector(|| "filter-table-story-scroll".into())
                            .h(px(256.))
                            .max_h(px(256.))
                            .flex_none()
                            .track_scroll(&self.filter_table_scroll)
                            .overflow_y_scrollbar()
                            .child(filter_story)
                    },
                    cx,
                )
            }
            StoryId::ComparisonTable => {
                let comparison_story = window.use_keyed_state(
                    "comparison-table-story-state",
                    cx,
                    ComparisonTableStory::new,
                );
                self.section(
                    story,
                    "Comparison table",
                    || {
                        v_flex()
                            .id("comparison-table-story-scroll")
                            .debug_selector(|| "comparison-table-story-scroll".into())
                            // 256px left the table barely a few rows tall and
                            // forced horizontal squeeze; 400px shows headers
                            // plus several feature rows comfortably.
                            .h(px(400.))
                            .max_h(px(400.))
                            .flex_none()
                            .track_scroll(&self.comparison_table_scroll)
                            .overflow_y_scrollbar()
                            .child(comparison_story)
                    },
                    cx,
                )
            }
            StoryId::CodeBlock => self.section(
                story,
                "Code block",
                || CodeBlock::streamed("code", &self.sim.code).language("rust"),
                cx,
            ),
            StoryId::Approval => self.section(
                story,
                "Approval card",
                || {
                    ApprovalCard::new("gate", "Send order confirmation to 3 suppliers?")
                        .description("Emails will go out immediately and cannot be recalled.")
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_1p5()
                                .child(
                                    ToolChip::new("gate-chip-1", "email alpenrose")
                                        .status(ToolStatus::Pending),
                                )
                                .child(
                                    ToolChip::new("gate-chip-2", "email tillamook")
                                        .status(ToolStatus::Pending),
                                ),
                        )
                        .on_event(cx.listener(|_, event: &ApprovalEvent, _, _| {
                            println!("approval event: {event:?}");
                        }))
                },
                cx,
            ),
            StoryId::Recommendation => self.section(
                story,
                "Recommendation card",
                || {
                    RecommendationCard::new("rec", "Switch supplier to Alpenrose Dairy")
                        .description("Lower unit cost at equal volume; delivery risk unchanged.")
                        .confidence(0.87)
                        .alternatives(["Keep current supplier", "Split volume 60/40"])
                        .on_event(cx.listener(|_, event: &RecommendationEvent, _, _| {
                            println!("recommendation event: {event:?}");
                        }))
                },
                cx,
            ),
            StoryId::Context => self.section(
                story,
                "Context cards",
                || {
                    v_flex()
                        .gap_2()
                        .child(
                            ContextCard::new("ctx-1", "pricing.md")
                                .snippet(
                                    "Enterprise volume pricing is renegotiated quarterly; \
                                     the March sheet supersedes all prior quotes.",
                                )
                                .relevance(0.92)
                                .on_event(cx.listener(|_, event: &ContextCardEvent, _, _| {
                                    println!("context event: {event:?}");
                                })),
                        )
                        .child(
                            ContextCard::new("ctx-2", "suppliers.csv")
                                .snippet("Alpenrose Dairy, Portland OR, net-30, 4.8 rating")
                                .relevance(0.81),
                        )
                },
                cx,
            ),
            StoryId::Insights => self.section(
                story,
                "Insight card",
                || {
                    v_flex()
                        .id("insight-story-scroll")
                        .debug_selector(|| "insight-story-scroll".into())
                        // 256px clipped the sparkline charts mid-curve; 420px
                        // shows the full card — metrics, charts, and footer.
                        .h(px(420.))
                        .max_h(px(420.))
                        .flex_none()
                        .gap_2()
                        .track_scroll(&self.insight_scroll)
                        .overflow_y_scrollbar()
                        .child(
                            InsightCard::new(
                                "demand-insight",
                                "Demand changed across top flavors",
                            )
                            .body(
                                "Mint Chip softened this week while Vanilla and Strawberry gained. \
                                 The mix has shifted enough to review the next production run.",
                            )
                            .page(1, 3)
                            .metrics([
                                InsightMetric::new("mint", "Mint Chip", "$2,377.66")
                                    .change("down 4.41%", InsightTrend::Down),
                                InsightMetric::new("vanilla", "Vanilla", "$3,104.20")
                                    .change("up 8.17%", InsightTrend::Up),
                                InsightMetric::new("strawberry", "Strawberry", "$1,842.50")
                                    .change("flat 0.08%", InsightTrend::Flat),
                            ])
                            .series(
                                "Weekly flavor demand",
                                [
                                    InsightPoint::new("Mon", 18.0),
                                    InsightPoint::new("Tue", 22.0),
                                    InsightPoint::new("Wed", 20.0),
                                    InsightPoint::new("Thu", 27.0),
                                    InsightPoint::new("Fri", 25.0),
                                    InsightPoint::new("Sat", 31.0),
                                ],
                            )
                            .chart_summary(
                                "Weekly flavor demand rose overall from 18 orders on Monday to 31 on Saturday, with a midweek dip.",
                            )
                            .follow_up("Rebalance the next flavor run")
                            .on_event(cx.listener(|_, event: &InsightEvent, _, _| {
                                println!("insight event: {event:?}");
                            })),
                        )
                        .child(
                            TextView::markdown(
                                "insight-reference-note",
                                "**Reference comparison.** Beautiful UI uses two compact value tiles and a canvas sparkline. This GPUI version deliberately shows three text-labeled trend tiles, an accessible GPUI line chart, named Previous/Next controls, and constrained scrolling so the same content remains understandable and reachable without color or pointer input.",
                            )
                            .selectable(true),
                        )
                        .child(
                            div()
                                .debug_selector(|| "insight-story-end".into())
                                .h(px(1.)),
                        )
                },
                cx,
            ),
            StoryId::PromptBar => {
                let prompt_story = window.use_keyed_state(
                    "prompt-bar-story-state",
                    cx,
                    PromptBarStory::new,
                );
                self.section(
                    story,
                    "Prompt bar",
                    || {
                        v_flex()
                            .id("prompt-bar-story-scroll")
                            .debug_selector(|| "prompt-bar-story-scroll".into())
                            // 256px clipped suggestion rows mid-card.
                            .h(px(360.))
                            .max_h(px(360.))
                            .flex_none()
                            .gap_2()
                            .track_scroll(&self.prompt_bar_scroll)
                            .overflow_y_scrollbar()
                            .child(prompt_story)
                            .child(
                                div()
                                    .debug_selector(|| "prompt-bar-story-end".into())
                                    .h(px(1.)),
                            )
                    },
                    cx,
                )
            }
            StoryId::ToolCalls => self.section(
                story,
                "Tool calls",
                || {
                    let running = self.sim.answer.is_streaming();
                    let read = Progressive::complete(
                        ToolInvocation::new("read-pricing", "read_file")
                            .summary("pricing.md")
                            .input("{\n  \"path\": \"pricing.md\",\n  \"range\": [1, 214]\n}")
                            .output(
                                "Read **214 lines**.\n\n| Supplier | Unit cost | Tier |\n|---|---|---|\n| Alpenrose Dairy | $3.12 | 500+ |\n| Tillamook | $3.36 | 500+ |",
                            )
                            .elapsed(Duration::from_millis(340)),
                    );
                    let search_call = ToolInvocation::new("search-suppliers", "web_search")
                        .summary("alpenrose wholesale pricing")
                        .icon(IconName::Globe)
                        .input("{\n  \"query\": \"alpenrose wholesale pricing\",\n  \"limit\": 3\n}");
                    let search = if running {
                        Progressive::running(search_call)
                    } else {
                        Progressive::complete(
                            search_call
                                .output(
                                    "3 results — alpenrose.com, dairyreport.org, nwfoodtrade.com",
                                )
                                .elapsed(Duration::from_millis(1800)),
                        )
                    };
                    let email = Progressive::pending(
                        ToolInvocation::new("send-confirmations", "send_email")
                            .summary("3 suppliers")
                            .input(
                                "{\n  \"to\": [\"orders@alpenrose.com\", \"sales@tillamook.com\"],\n  \"subject\": \"Order confirmation — week 35\"\n}",
                            )
                            .approval(self.tool_approval),
                    );
                    let failed = Progressive::failed(
                        ToolInvocation::new("query-prices", "query_db")
                            .summary("prices-db")
                            .input("SELECT unit_cost FROM prices WHERE week = 35")
                            .input_language("sql")
                            .elapsed(Duration::from_millis(2100)),
                        "Connection timed out after 2s",
                    );
                    let configure = |mut call: ToolCall, id: &str| {
                        if let Some(open) = self.tool_call_open.get(id) {
                            call = call.open(*open);
                        }
                        call
                    };
                    let mut group = ToolGroup::new("tool-group").count(2).active(running);
                    if let Some(open) = self.tool_group_open {
                        group = group.open(open);
                    }
                    v_flex()
                        .gap_4()
                        .child(
                            group
                                .on_event(cx.listener(|this, event: &ToolGroupEvent, _, cx| {
                                    let ToolGroupEvent::Toggled { open, .. } = event;
                                    this.tool_group_open = Some(*open);
                                    cx.notify();
                                }))
                                .child(
                                    configure(ToolCall::new(&read), "read-pricing").on_event(
                                        cx.listener(Self::handle_tool_call_event),
                                    ),
                                )
                                .child(
                                    configure(ToolCall::new(&search), "search-suppliers")
                                        .on_event(cx.listener(Self::handle_tool_call_event)),
                                ),
                        )
                        .child(
                            configure(ToolCall::new(&email), "send-confirmations")
                                .on_event(cx.listener(Self::handle_tool_call_event)),
                        )
                        .child(
                            configure(ToolCall::new(&failed), "query-prices")
                                .on_event(cx.listener(Self::handle_tool_call_event)),
                        )
                        .child(
                            TextView::markdown(
                                "tool-calls-reference-note",
                                "**Reference comparison.** AI Elements and assistant-ui render each tool call as a collapsible card with a status badge, input, output, and inline Allow/Deny. gpui-ai keeps that shape, shares one status vocabulary with chips and task rows, adds a controlled group whose title shimmers while calls run, opens automatically only when a call needs attention, and reports every decision as a typed event.",
                            )
                            .selectable(true),
                        )
                },
                cx,
            ),
            StoryId::SelectionActions => {
                let selection_story = window.use_keyed_state(
                    "selection-actions-story-state",
                    cx,
                    SelectionActionsStory::new,
                );
                self.section(
                    story,
                    "Selection actions",
                    || {
                        v_flex()
                            .id("selection-actions-story-scroll")
                            .debug_selector(|| "selection-actions-story-scroll".into())
                            .h(px(256.))
                            .max_h(px(256.))
                            .flex_none()
                            .gap_2()
                            .track_scroll(&self.selection_actions_scroll)
                            .overflow_y_scrollbar()
                            .child(selection_story)
                            .child(
                                div()
                                    .debug_selector(|| "selection-actions-story-end".into())
                                    .h(px(1.)),
                            )
                    },
                    cx,
                )
            }
        }
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.selected == StoryId::All {
            div()
                .id("gallery-scroll")
                .debug_selector(|| "gallery-scroll".into())
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                // The wheel cursor signals the feed accepts scroll input;
                // an active autoscroll session shows the grab affordance.
                .cursor(CursorStyle::Arrow)
                .when(self.autoscroll.is_some(), |region| {
                    region.cursor(CursorStyle::OpenHand)
                })
                // Middle-click autoscroll: middle press starts or cancels,
                // any other press cancels.
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        if this.autoscroll.is_some() {
                            this.cancel_autoscroll(cx);
                        } else {
                            this.start_autoscroll(event.position, cx);
                        }
                    }),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.cancel_autoscroll(cx);
                    }),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(|this, _, _, cx| {
                        this.cancel_autoscroll(cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                    this.track_autoscroll_pointer(event.position, cx);
                }))
                .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                    this.handle_catalog_wheel(event, cx);
                }))
                .on_action(cx.listener(|this, _: &ScrollHome, _, cx| {
                    this.scroll_catalog_edge(false, cx);
                }))
                .on_action(cx.listener(|this, _: &ScrollEnd, _, cx| {
                    this.scroll_catalog_edge(true, cx);
                }))
                .on_action(cx.listener(|this, _: &PageUp, _, cx| {
                    this.scroll_catalog_page(-1., cx);
                }))
                .on_action(cx.listener(|this, _: &PageDown, _, cx| {
                    this.scroll_catalog_page(1., cx);
                }))
                .on_action(cx.listener(|this, _: &CancelAutoscroll, _, cx| {
                    this.cancel_autoscroll(cx);
                }))
                .child(
                    story_list_frame()
                        .size_full()
                        .child(
                            list(
                                self.catalog_list.clone(),
                                cx.processor(Self::render_catalog_story),
                            )
                            .size_full(),
                        )
                        .vertical_scrollbar(&self.catalog_list),
                )
                .into_any_element()
        } else {
            div()
                .id("gallery-scroll")
                .debug_selector(|| "gallery-scroll".into())
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(self.render_story(self.selected, window, cx))
                .into_any_element()
        };

        v_flex()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .w_full()
                    .max_w(px(640.))
                    .mx_auto()
                    .p_6()
                    .items_center()
                    .justify_between()
                    .child(div().font_semibold().child(self.selected.title()))
                    .child(
                        Button::new("theme")
                            .outline()
                            .label(format!("Theme: {}", self.theme.label()))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.theme = this.theme.next();
                                apply_gallery_theme(this.theme, Some(window), cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(content)
    }
}

/// Initializes the component and theme globals used by every gallery host.
pub fn init(cx: &mut App) {
    gpui_ai::init(cx);
    ThemeRegistry::global_mut(cx)
        .load_themes_from_str(CONTRAST_THEME)
        .expect("embedded gallery theme must be valid");
    // Showcase themes share the website's downloadable theme pack.
    ThemeRegistry::global_mut(cx)
        .load_themes_from_str(SHOWCASE_THEMES_JSON)
        .expect("embedded showcase themes must be valid");
    cx.bind_keys([
        KeyBinding::new("pageup", PageUp, Some("gallery-scroll")),
        KeyBinding::new("pagedown", PageDown, Some("gallery-scroll")),
        KeyBinding::new("home", ScrollHome, Some("gallery-scroll")),
        KeyBinding::new("end", ScrollEnd, Some("gallery-scroll")),
        KeyBinding::new("escape", CancelAutoscroll, Some("gallery-scroll")),
    ]);
}

/// Opens a gallery window for `selected`.
pub fn open_gallery(selected: StoryId, cx: &mut App) {
    open_gallery_window(selected, None, cx);
}

/// Opens a gallery window with an explicit visual-review theme.
pub fn open_gallery_with_theme(
    selected: StoryId,
    theme: GalleryTheme,
    cx: &mut App,
) -> Entity<Gallery> {
    open_gallery_window(selected, Some(theme), cx)
}

fn open_gallery_window(
    selected: StoryId,
    theme: Option<GalleryTheme>,
    cx: &mut App,
) -> Entity<Gallery> {
    let view = cx.new(|cx| Gallery::new_with_theme(selected, theme, cx));
    let root_view = view.clone();
    cx.open_window(WindowOptions::default(), move |window, cx| {
        if let Some(theme) = theme {
            apply_gallery_theme(theme, Some(window), cx);
        }
        cx.new(|cx| Root::new(root_view, window, cx).bg(cx.theme().background))
    })
    .expect("failed to open gallery window");
    view
}

#[cfg(test)]
mod tests {
    use super::{
        ChatStory, Gallery, GalleryTheme, filter_story_project_rows, filter_story_rows,
        reduce_filter_story_projection,
    };
    use crate::StoryId;
    use gpui::{
        AppContext as _, Context, Element as _, IntoElement as _, Modifiers, MouseButton, Render,
        Role, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, Window, accesskit,
        point, px, size,
    };
    use gpui_ai::{
        prelude::{FilterRow, FilterSortDirection, FilterTableEvent},
        stream::StreamedContent,
    };
    use gpui_component::ActiveTheme as _;
    use std::{cell::RefCell, collections::HashMap, rc::Rc};

    struct GalleryTestRoot {
        gallery: gpui::Entity<Gallery>,
    }

    impl Render for GalleryTestRoot {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            self.gallery.clone()
        }
    }

    fn all_stories(cx: &mut TestAppContext) -> (gpui::Entity<Gallery>, &mut VisualTestContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(|_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::All, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        (gallery, cx)
    }

    fn scroll(cx: &mut VisualTestContext, dy: f32) {
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(100.), px(200.)),
            delta: ScrollDelta::Pixels(point(px(0.), px(dy))),
            ..Default::default()
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[test]
    fn gallery_theme_cycle_covers_all_review_presets() {
        let expected: [GalleryTheme; 7] = [
            GalleryTheme::Light,
            GalleryTheme::Dark,
            GalleryTheme::Contrast,
            GalleryTheme::MidnightViolet,
            GalleryTheme::NordFrost,
            GalleryTheme::EmberDusk,
            GalleryTheme::PaperLight,
        ];
        for pair in expected.windows(2) {
            assert_eq!(pair[0].next(), pair[1]);
        }
        // The cycle closes.
        assert_eq!(
            expected[expected.len() - 1].next(),
            expected[0],
            "the theme cycle must return to Light"
        );
    }

    #[gpui::test]
    fn contrast_theme_installs_the_contrast_registry_config(cx: &mut TestAppContext) {
        cx.update(super::init);
        cx.update(|cx| {
            super::apply_gallery_theme(GalleryTheme::Contrast, None, cx);
            assert_eq!(
                cx.theme().dark_theme.name.as_ref(),
                super::CONTRAST_THEME_NAME
            );
        });
    }

    #[gpui::test]
    fn all_stories_exposes_a_vertical_scrollbar(cx: &mut TestAppContext) {
        let (_, cx) = all_stories(cx);

        assert!(cx.debug_bounds("scrollbar-overlay").is_some());
    }

    #[gpui::test]
    fn catalog_wheel_acceleration_scales_with_cadence_and_resets(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);

        // Notches must land over the feed; negative dy scrolls down.
        // One simulated event proves the render-tree wiring end to end
        // (List applies its base scroll, our bubble-phase handler sees it).
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(100.), px(200.)),
            delta: ScrollDelta::Lines(point(0., -1.)),
            ..Default::default()
        });
        let after_first = gallery.read_with(cx, |gallery: &Gallery, _| {
            let top = gallery.catalog_list.logical_scroll_top();
            top.item_ix as f32 * 320. + top.offset_in_item.as_f32()
        });
        assert!(after_first > 0., "the first notch should scroll the feed");

        // Drive the remaining notches directly so the acceleration math is
        // exercised deterministically, independent of hit-test plumbing.
        let notch = ScrollWheelEvent {
            position: point(px(100.), px(200.)),
            delta: ScrollDelta::Lines(point(0., -1.)),
            ..Default::default()
        };
        for _ in 0..8 {
            gallery.update(cx, |gallery, cx| gallery.handle_catalog_wheel(&notch, cx));
        }
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let after_ramp = gallery.read_with(cx, |gallery: &Gallery, _| {
            let top = gallery.catalog_list.logical_scroll_top();
            top.item_ix as f32 * 320. + top.offset_in_item.as_f32()
        });
        assert!(
            after_ramp > after_first,
            "accelerated notches should travel farther than the first"
        );

        // A direction reversal resets cadence: the opposite notch moves back
        // up by roughly one base step without panic.
        let reverse = ScrollWheelEvent {
            position: point(px(100.), px(200.)),
            delta: ScrollDelta::Lines(point(0., 1.)),
            ..Default::default()
        };
        gallery.update(cx, |gallery, cx| gallery.handle_catalog_wheel(&reverse, cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let after_reverse = gallery.read_with(cx, |gallery: &Gallery, _| {
            let top = gallery.catalog_list.logical_scroll_top();
            top.item_ix as f32 * 320. + top.offset_in_item.as_f32()
        });
        assert!(
            after_reverse < after_ramp,
            "reverse notch should move the feed back up"
        );
    }

    #[gpui::test]
    fn catalog_row_estimates_invalidate_when_rem_size_changes(cx: &mut TestAppContext) {
        // The design guides require anything cached from resolved layout to
        // key on rem size: the catalog's uniform story-row estimate must be
        // re-derived, not stale, after a base-font (zoom) change.
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);

        let scroll_top_at = |cx: &mut VisualTestContext, rem: f32| -> f32 {
            let window = cx
                .windows()
                .first()
                .copied()
                .expect("test window should exist");
            cx.update_window(window, |_, window, _| window.set_rem_size(px(rem)))
                .expect("window update should succeed");
            gallery.update(cx, |gallery, cx| {
                gallery.catalog_list.remeasure();
                cx.notify();
            });
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let top = gallery.read_with(cx, |gallery: &Gallery, _| {
                gallery.catalog_list.logical_scroll_top()
            });
            top.item_ix as f32 * 320. + top.offset_in_item.as_f32()
        };

        let before = scroll_top_at(cx, 16.);
        // Scroll into the feed so measured rows exist.
        gallery.update(cx, |gallery, cx| {
            gallery.scroll_catalog_page(2., cx);
        });
        cx.run_until_parked();

        let default_scale = scroll_top_at(cx, 16.);
        let zoomed = scroll_top_at(cx, 20.);
        let back = scroll_top_at(cx, 16.);

        // The list stays navigable across zoom changes and returns to its
        // prior position when the base font is restored — proof that
        // measurement was refreshed rather than cached against one rem size.
        assert!(default_scale.is_finite() && zoomed.is_finite() && back.is_finite());
        assert!(
            (back - default_scale).abs() < 1_000.,
            "restoring the base font should restore the scroll geometry"
        );
        let _ = before;
    }

    #[gpui::test]
    fn catalog_paging_and_edge_navigation_move_the_feed(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);
        let before = gallery.read_with(cx, |gallery: &Gallery, _| {
            gallery.catalog_list.logical_scroll_top().item_ix
        });

        gallery.update(cx, |gallery, cx| gallery.scroll_catalog_page(1., cx));
        let after_page_down = gallery.read_with(cx, |gallery: &Gallery, _| {
            gallery.catalog_list.logical_scroll_top().item_ix
        });
        assert!(after_page_down > before, "PageDown should advance the feed");

        gallery.update(cx, |gallery, cx| gallery.scroll_catalog_edge(true, cx));
        let at_end = gallery.read_with(cx, |gallery: &Gallery, _| {
            gallery.catalog_list.logical_scroll_top().item_ix
        });
        assert!(at_end >= after_page_down, "End should reach the last story");

        gallery.update(cx, |gallery, cx| gallery.scroll_catalog_edge(false, cx));
        let at_home = gallery.read_with(cx, |gallery: &Gallery, _| {
            gallery.catalog_list.logical_scroll_top().item_ix
        });
        assert_eq!(at_home, 0, "Home should return to the first story");

        gallery.update(cx, |gallery, cx| gallery.scroll_catalog_page(-1., cx));
        let clamped = gallery.read_with(cx, |gallery: &Gallery, _| {
            gallery.catalog_list.logical_scroll_top().item_ix
        });
        assert_eq!(clamped, 0, "PageUp at home stays clamped");
    }

    #[gpui::test]
    fn direct_records_table_story_exercises_required_controlled_states(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::RecordsTable, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(560.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // Only the active state renders; every state must be reachable
        // through the switcher toolbar.
        for (index, selector) in [
            "records-story-populated",
            "records-story-loading",
            "records-story-error",
            "records-story-empty",
            "records-story-disabled",
            "records-story-selected",
            "records-story-constrained",
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                activate_story_state(cx, "records-story", index);
            }
            assert!(
                cx.debug_bounds(selector).is_some(),
                "records story should exercise {selector}"
            );
        }
        assert!(
            cx.debug_bounds("records-story-reference-note").is_some(),
            "records story should exercise records-story-reference-note"
        );
    }

    /// Clicks one switcher control and settles the redraw.
    fn activate_story_state(cx: &mut VisualTestContext, slug: &str, index: usize) {
        let labels = [
            "Populated",
            "Loading",
            "Error",
            "Empty",
            "Disabled",
            "Selected",
            "Constrained",
        ];
        // debug_bounds requires a 'static str; leaking in a test is fine.
        let selector: &'static str =
            Box::leak(format!("{slug}-state-{}", labels[index].to_lowercase()).into_boxed_str());
        let bounds = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("switcher control {} should be visible", labels[index]));
        let center = bounds.center();
        cx.run_until_parked();
        cx.simulate_mouse_move(center, None, Modifiers::default());
        cx.simulate_mouse_down(center, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(center, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[gpui::test]
    fn constrained_records_table_story_keeps_its_reference_end_reachable(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::RecordsTable, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let viewport = cx
            .debug_bounds("records-table-story-scroll")
            .expect("the direct story should expose its overflow region");
        let initial_end = cx
            .debug_bounds("records-story-reference-note")
            .expect("the reference note should remain rendered");
        assert!(
            initial_end.top() >= viewport.bottom(),
            "the end should start below the constrained viewport: end={initial_end:?}, viewport={viewport:?}"
        );

        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        gallery.update(cx, |gallery, cx| {
            gallery.records_table_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let end = cx
            .debug_bounds("records-story-reference-note")
            .expect("the reference note should remain rendered after scrolling");
        assert!(
            end.bottom() <= viewport.bottom() && end.bottom() > viewport.top(),
            "{end:?} must fit in {viewport:?}"
        );
    }

    #[gpui::test]
    fn direct_diff_table_story_exercises_required_controlled_states(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::DiffTable, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(560.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // Only the active state renders; every state must be reachable
        // through the switcher toolbar.
        for (index, selector) in [
            "diff-story-populated",
            "diff-story-loading",
            "diff-story-error",
            "diff-story-empty",
            "diff-story-disabled",
            "diff-story-selected",
            "diff-story-constrained",
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                activate_story_state(cx, "diff-story", index);
            }
            assert!(
                cx.debug_bounds(selector).is_some(),
                "diff story should exercise {selector}"
            );
        }
        assert!(
            cx.debug_bounds("diff-story-reference-note").is_some(),
            "diff story should exercise diff-story-reference-note"
        );
    }

    #[gpui::test]
    fn constrained_diff_table_story_keeps_its_reference_end_reachable(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::DiffTable, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let viewport = cx
            .debug_bounds("diff-table-story-scroll")
            .expect("the direct diff story should expose its overflow region");
        let initial_end = cx
            .debug_bounds("diff-story-reference-note")
            .expect("the diff reference note should remain rendered");
        assert!(
            initial_end.top() >= viewport.bottom(),
            "the end should start below the constrained viewport: end={initial_end:?}, viewport={viewport:?}"
        );

        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        gallery.update(cx, |gallery, cx| {
            gallery.diff_table_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let end = cx
            .debug_bounds("diff-story-reference-note")
            .expect("the diff reference note should remain rendered after scrolling");
        assert!(
            end.bottom() <= viewport.bottom() && end.bottom() > viewport.top(),
            "{end:?} must fit in {viewport:?}"
        );
    }

    #[gpui::test]
    fn direct_filter_table_story_exercises_required_controlled_states(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::FilterTable, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(560.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // Only the active state renders; every state must be reachable
        // through the switcher toolbar.
        for (index, selector) in [
            "filter-story-populated",
            "filter-story-loading",
            "filter-story-error",
            "filter-story-empty",
            "filter-story-disabled",
            "filter-story-selected",
            "filter-story-constrained",
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                activate_story_state(cx, "filter-story", index);
            }
            assert!(
                cx.debug_bounds(selector).is_some(),
                "filter story should exercise {selector}"
            );
        }
        assert!(
            cx.debug_bounds("filter-story-reference-note").is_some(),
            "filter story should exercise filter-story-reference-note"
        );
    }

    #[gpui::test]
    fn constrained_filter_table_story_keeps_its_reference_end_reachable(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::FilterTable, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let viewport = cx
            .debug_bounds("filter-table-story-scroll")
            .expect("the direct filter story should expose its overflow region");
        let initial_end = cx
            .debug_bounds("filter-story-reference-note")
            .expect("the filter reference note should remain rendered");
        assert!(initial_end.top() >= viewport.bottom());

        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        gallery.update(cx, |gallery, cx| {
            gallery.filter_table_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let end = cx
            .debug_bounds("filter-story-reference-note")
            .expect("the filter reference note should remain rendered after scrolling");
        assert!(
            end.bottom() <= viewport.bottom() && end.bottom() > viewport.top(),
            "{end:?} must fit in {viewport:?}"
        );
    }

    #[gpui::test]
    fn direct_comparison_table_story_exercises_required_controlled_states(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::ComparisonTable, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(560.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // Only the active state renders; every state must be reachable
        // through the switcher toolbar. The constrained state lazily builds
        // the maximum 12x128 grid.
        for (index, selector) in [
            "comparison-story-populated",
            "comparison-story-loading",
            "comparison-story-error",
            "comparison-story-empty",
            "comparison-story-disabled",
            "comparison-story-selected",
            "comparison-story-constrained",
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                activate_story_state(cx, "comparison-story", index);
            }
            assert!(
                cx.debug_bounds(selector).is_some(),
                "comparison story should exercise {selector}"
            );
        }
        assert!(
            cx.debug_bounds("comparison-story-reference-note").is_some(),
            "comparison story should exercise comparison-story-reference-note"
        );
    }

    #[gpui::test]
    fn constrained_comparison_table_story_keeps_its_reference_end_reachable(
        cx: &mut TestAppContext,
    ) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::ComparisonTable, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        // The story host is 400px tall; a 320px window guarantees the
        // reference note starts out of view so the scroll contract is
        // actually exercised.
        cx.simulate_resize(size(px(700.), px(360.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let viewport = cx
            .debug_bounds("comparison-table-story-scroll")
            .expect("the direct comparison story should expose its overflow region");
        let initial_end = cx
            .debug_bounds("comparison-story-reference-note")
            .expect("the comparison reference note should remain rendered");

        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        gallery.update(cx, |gallery, cx| {
            gallery.comparison_table_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let end = cx
            .debug_bounds("comparison-story-reference-note")
            .expect("the comparison reference note should remain rendered after scrolling");
        assert!(
            end.bottom() <= viewport.bottom() && end.bottom() > viewport.top(),
            "{end:?} must fit in {viewport:?}"
        );
        // Scrolling to the bottom must have moved the note (or it was already
        // fully visible in a tall viewport); either way the end must be
        // reachable and rendered.
        let _ = initial_end;
    }

    #[test]
    fn filter_story_reducer_keeps_table_instances_independent_and_reapplies_sort() {
        let mut projections = HashMap::new();
        reduce_filter_story_projection(
            &mut projections,
            &FilterTableEvent::SortRequested {
                id: "gallery-filter-populated".into(),
                column_id: "date".into(),
                direction: Some(FilterSortDirection::Descending),
            },
        );
        reduce_filter_story_projection(
            &mut projections,
            &FilterTableEvent::FilterRequested {
                id: "gallery-filter-populated".into(),
                filter_id: "todo".into(),
                active: true,
            },
        );
        reduce_filter_story_projection(
            &mut projections,
            &FilterTableEvent::FilterRequested {
                id: "gallery-filter-selected".into(),
                filter_id: "completed".into(),
                active: true,
            },
        );

        let populated = projections
            .get("gallery-filter-populated")
            .expect("populated projection should exist");
        assert_eq!(populated.active_filter, "todo");
        assert_eq!(populated.sort_column.as_deref(), Some("date"));
        assert_eq!(
            populated.sort_direction,
            Some(FilterSortDirection::Descending)
        );
        let projected = filter_story_project_rows(&filter_story_rows(), populated);
        assert_eq!(
            projected.iter().map(FilterRow::id).collect::<Vec<_>>(),
            ["menu", "mango"]
        );

        let selected = projections
            .get("gallery-filter-selected")
            .expect("selected projection should exist");
        assert_eq!(selected.active_filter, "completed");
        assert_eq!(selected.sort_column, None);
        assert_eq!(selected.sort_direction, None);
    }

    #[gpui::test]
    fn all_stories_virtualizes_distant_rows_until_they_enter_view(cx: &mut TestAppContext) {
        let (_, cx) = all_stories(cx);
        let viewport_height = cx.update(|window, _| window.viewport_size().height);
        assert!(cx.debug_bounds("story-loading").is_some());
        assert!(
            cx.debug_bounds("story-selection-actions").is_none(),
            "the final story should not be constructed before it nears the viewport"
        );

        for _ in StoryId::ALL {
            if cx.debug_bounds("story-selection-actions").is_some() {
                break;
            }
            scroll(cx, -10_000.);
        }

        let scrolled = cx
            .debug_bounds("story-selection-actions")
            .expect("selection actions story should render after scrolling to it");
        assert!(scrolled.top() < viewport_height);
        assert!(
            cx.debug_bounds("story-loading").is_none(),
            "the first animated story should leave the render tree when it is distant"
        );
    }

    #[gpui::test]
    fn constrained_catalog_keeps_the_end_of_the_insight_story_reachable(cx: &mut TestAppContext) {
        let (gallery, cx) = all_stories(cx);
        cx.simulate_resize(size(px(900.), px(560.)));
        gallery.update(cx, |gallery, cx| {
            gallery.scroll_catalog_to(StoryId::Insights, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let scroll = cx
            .debug_bounds("insight-story-scroll")
            .expect("the catalog insight story should expose its overflow region");
        gallery.update(cx, |gallery, cx| {
            gallery.insight_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let end = cx
            .debug_bounds("insight-story-end")
            .expect("the insight story end marker should remain rendered");
        assert!(
            end.bottom() <= scroll.bottom(),
            "{end:?} must fit in {scroll:?}"
        );
    }

    #[gpui::test]
    fn constrained_direct_insight_story_keeps_its_end_reachable(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::Insights, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        gallery.update(cx, |gallery, cx| {
            gallery.insight_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let story = cx
            .debug_bounds("insight-story-scroll")
            .expect("the direct insight scroll region should remain rendered");
        let end = cx
            .debug_bounds("insight-story-end")
            .expect("the direct insight end marker should remain rendered");
        assert!(
            end.bottom() <= story.bottom(),
            "{end:?} must fit in {story:?}"
        );
    }

    #[gpui::test]
    fn constrained_direct_prompt_bar_story_keeps_its_end_reachable(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::PromptBar, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert!(
            cx.debug_bounds("prompt-bar-empty-heading").is_some(),
            "the shared story should render an explicit empty state"
        );
        assert!(
            cx.debug_bounds("prompt-bar-multiline-heading").is_some(),
            "the shared story should render an explicit multiline state"
        );

        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        gallery.update(cx, |gallery, cx| {
            gallery.prompt_bar_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let story = cx
            .debug_bounds("prompt-bar-story-scroll")
            .expect("the prompt bar story should expose its overflow region");
        let end = cx
            .debug_bounds("prompt-bar-story-end")
            .expect("the prompt bar story end marker should remain rendered");
        assert!(
            end.bottom() <= story.bottom(),
            "{end:?} must fit in {story:?}"
        );
    }

    #[gpui::test]
    fn constrained_direct_chat_keeps_latest_message_and_composer_reachable(
        cx: &mut TestAppContext,
    ) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::Chat, cx));
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let host = cx
            .debug_bounds("chat-story-host")
            .expect("the constrained chat host should render");
        let latest = cx
            .debug_bounds("chat-message-live-answer")
            .expect("tail-follow should keep the latest message rendered");
        let composer = cx
            .debug_bounds("chat-composer")
            .expect("the composed prompt should remain rendered");
        assert!(
            latest.bottom() > host.top() && latest.top() < host.bottom(),
            "{latest:?} must intersect the visible transcript in {host:?}"
        );
        assert!(
            composer.bottom() <= host.bottom(),
            "{composer:?} must fit in {host:?}"
        );
    }

    #[gpui::test]
    fn chat_story_skips_unchanged_answer_rebuilds(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (story, cx) = cx.add_window_view(ChatStory::new);
        let answer = StreamedContent::running("Live supplier comparison".to_owned());

        let first = cx.update(|window, cx| {
            story.update(cx, |story, cx| story.set_answer(answer.clone(), window, cx))
        });
        let repeated = cx.update(|window, cx| {
            story.update(cx, |story, cx| story.set_answer(answer, window, cx))
        });

        assert!(first);
        assert!(!repeated);
    }

    #[gpui::test]
    fn constrained_catalog_keeps_the_end_of_the_prompt_bar_story_reachable(
        cx: &mut TestAppContext,
    ) {
        let (gallery, cx) = all_stories(cx);
        cx.simulate_resize(size(px(900.), px(560.)));
        gallery.update(cx, |gallery, cx| {
            gallery.scroll_catalog_to(StoryId::PromptBar, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let scroll = cx
            .debug_bounds("prompt-bar-story-scroll")
            .expect("the catalog prompt bar story should expose its overflow region");
        gallery.update(cx, |gallery, cx| {
            gallery.prompt_bar_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let end = cx
            .debug_bounds("prompt-bar-story-end")
            .expect("the prompt bar story end marker should remain rendered");
        assert!(
            end.bottom() <= scroll.bottom(),
            "{end:?} must fit in {scroll:?}"
        );
    }

    #[gpui::test]
    fn constrained_direct_selection_actions_story_keeps_its_end_reachable(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::SelectionActions, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let gallery = result
            .borrow_mut()
            .take()
            .expect("gallery should be captured");
        gallery.update(cx, |gallery, cx| {
            gallery.selection_actions_scroll.scroll_to_bottom();
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let story = cx
            .debug_bounds("selection-actions-story-scroll")
            .expect("the selection actions story should expose its overflow region");
        let end = cx
            .debug_bounds("selection-actions-story-end")
            .expect("the selection actions story end marker should remain rendered");
        assert!(
            end.bottom() <= story.bottom(),
            "{end:?} must fit in {story:?}"
        );
    }

    #[test]
    fn virtualized_story_semantics_use_stable_domain_identity() {
        let list = super::story_list_frame().into_element();
        let mut list_node = accesskit::Node::new(Role::Unknown);
        let list_role = list.a11y_role();
        list.write_a11y_info(&mut list_node);

        assert_eq!(list_role, Some(Role::List));
        assert_eq!(list_node.author_id(), Some("gallery.story-list"));
        assert_eq!(list_node.label(), Some("Component stories"));

        let row = super::story_frame(StoryId::Context, true).into_element();
        let mut row_node = accesskit::Node::new(Role::Unknown);
        let row_role = row.a11y_role();
        row.write_a11y_info(&mut row_node);

        assert_eq!(row_role, Some(Role::ListItem));
        assert_eq!(row_node.author_id(), Some("gallery.story.context"));
        assert_eq!(row_node.label(), Some("Context cards"));

        let direct = super::story_frame(StoryId::Context, false).into_element();
        assert_eq!(direct.a11y_role(), Some(Role::Group));
    }

    #[gpui::test]
    fn static_direct_story_does_not_advance_simulation(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery = cx.new(|cx| Gallery::new(StoryId::Approval, cx));
        cx.run_until_parked();

        cx.executor().advance_clock(super::sim::TICK_INTERVAL);
        cx.run_until_parked();

        assert_eq!(
            gallery.read_with(cx, |gallery, _| gallery.sim.elapsed()),
            std::time::Duration::ZERO
        );
    }

    #[test]
    fn simulation_ticks_only_invalidate_stories_with_visible_changes() {
        let mut simulation = super::sim::Simulation::new();
        let first_tick = simulation.tick();

        assert!(super::story_changed_by_delta(StoryId::Loading, first_tick));
        assert!(super::story_changed_by_delta(StoryId::Tasks, first_tick));
        assert!(super::story_changed_by_delta(
            StoryId::ImageGeneration,
            first_tick
        ));
        assert!(super::story_changed_by_delta(
            StoryId::StreamingText,
            first_tick
        ));
        assert!(super::story_changed_by_delta(StoryId::Chat, first_tick));
        assert!(!super::story_changed_by_delta(StoryId::Search, first_tick));
        assert!(!super::story_changed_by_delta(
            StoryId::Thinking,
            first_tick
        ));
        assert!(!super::story_changed_by_delta(
            StoryId::CodeBlock,
            first_tick
        ));
        assert!(!super::story_changed_by_delta(
            StoryId::Approval,
            first_tick
        ));
    }

    #[gpui::test]
    fn static_catalog_viewport_suspends_and_resumes_one_simulation(cx: &mut TestAppContext) {
        let (gallery, cx) = all_stories(cx);
        cx.simulate_resize(size(px(800.), px(360.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        for _ in StoryId::ALL {
            scroll(cx, -10_000.);
        }
        assert!(cx.debug_bounds("story-selection-actions").is_some());

        let (paused_at, task_running) = gallery.read_with(cx, |gallery, _| {
            (gallery.sim.elapsed(), gallery.simulation_task.is_some())
        });
        assert!(!task_running, "static rows should release the owned task");
        cx.executor().advance_clock(super::sim::TICK_INTERVAL * 3);
        cx.run_until_parked();
        assert_eq!(
            gallery.read_with(cx, |gallery, _| gallery.sim.elapsed()),
            paused_at,
            "static rows should not keep the simulation timer alive"
        );

        for _ in StoryId::ALL {
            scroll(cx, 10_000.);
        }
        assert!(cx.debug_bounds("story-loading").is_some());

        cx.executor().advance_clock(super::sim::TICK_INTERVAL);
        cx.run_until_parked();
        assert_eq!(
            gallery.read_with(cx, |gallery, _| gallery.sim.elapsed()),
            paused_at + super::sim::TICK_INTERVAL,
            "returning to dynamic rows should start exactly one simulation task"
        );
    }

    #[gpui::test]
    fn performance_viewport_control_uses_story_identity(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery = cx.new(|cx| Gallery::new(StoryId::All, cx));

        gallery.update(cx, |gallery, cx| {
            gallery.scroll_catalog_to(StoryId::StreamingText, cx);
        });

        let streaming = StoryId::ALL
            .iter()
            .position(|story| *story == StoryId::StreamingText)
            .expect("streaming text is a catalog story");
        gallery.read_with(cx, |gallery, _| {
            assert_eq!(gallery.catalog_list.logical_scroll_top().item_ix, streaming);
            assert_eq!(gallery.visible_range, streaming..streaming + 3);
            assert!(gallery.simulation_task.is_some());
        });
    }

    #[gpui::test]
    fn performance_chat_viewport_freezes_only_the_gallery_demo_producer(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery = cx.new(|cx| Gallery::new(StoryId::All, cx));

        gallery.update(cx, |gallery, cx| {
            gallery.prepare_performance_viewport(StoryId::Chat, cx);
        });

        gallery.read_with(cx, |gallery, _| {
            assert_eq!(gallery.selected, StoryId::Chat);
            assert!(gallery.simulation_task.is_none());
        });
    }

    #[gpui::test]
    fn performance_viewport_isolates_the_story_under_measurement(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery = cx.new(|cx| Gallery::new(StoryId::All, cx));

        gallery.update(cx, |gallery, cx| {
            gallery.prepare_performance_viewport(StoryId::FilterTable, cx);
        });

        gallery.read_with(cx, |gallery, _| {
            assert_eq!(gallery.selected, StoryId::FilterTable);
            assert!(gallery.shows(StoryId::FilterTable));
            assert!(!gallery.shows(StoryId::ComparisonTable));
        });
    }

    #[cfg(feature = "performance")]
    #[gpui::test]
    fn performance_filter_mode_is_active_before_the_first_measured_projection(
        cx: &mut TestAppContext,
    ) {
        cx.update(super::init);
        let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::All, cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        gallery.update(cx, |gallery, cx| {
            gallery.prepare_performance_viewport(StoryId::FilterTable, cx);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        gallery.read_with(cx, |gallery, cx| {
            let story = gallery
                .performance_filter_story
                .as_ref()
                .and_then(|story| story.upgrade())
                .expect("the isolated Filter viewport should construct its story");
            assert!(story.read(cx).performance_only);
        });
    }
}
