//! Native component gallery for mighty-gpui.
//!
//! One story per component, driven by simulated agent activity (fake token
//! streams, task lifecycles). All simulation lives here — the library
//! components only ever see data.

use crate::{StoryId, sim};

use gpui::*;
use gpui_component::{
    ActiveTheme as _, IconName, Root, StyledExt as _,
    button::Button,
    h_flex,
    scroll::ScrollableElement as _,
    text::TextView,
    theme::{Theme, ThemeMode, ThemeRegistry},
    v_flex,
};
use mighty_gpui::prelude::*;
use std::{collections::HashMap, ops::Range, sync::Arc, time::Duration};

const CONTRAST_THEME: &str = r##"{
  "name": "mighty-gpui gallery themes",
  "themes": [{
    "name": "Mighty Contrast",
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

const CONTRAST_THEME_NAME: &str = "Mighty Contrast";

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
        StoryId::Thinking | StoryId::Search => delta.answer_phase_changed(),
        StoryId::ImageGeneration | StoryId::StreamingText | StoryId::Chat => {
            delta.answer_content_changed()
        }
        StoryId::CodeBlock => delta.code_content_changed() || delta.code_phase_changed(),
        StoryId::All
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
        | StoryId::PromptBar
        | StoryId::SelectionActions => false,
    }
}

struct ChatStory {
    chat: Entity<Chat>,
    answer: Option<StreamedContent>,
    last_event: SharedString,
    _subscription: Subscription,
}

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
        let chat = cx.new(|cx| Chat::new("gallery-chat", prompt, window, cx));
        let subscription =
            cx.subscribe_in(&chat, window, |this, chat, event: &ChatEvent, _, cx| {
                this.last_event = format!("{event:?}").into();
                let prompt = chat.read(cx).prompt_bar().clone();
                match event {
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
                    | ChatEvent::JumpedToLatest => {}
                }
                cx.notify();
            });
        Self {
            chat,
            answer: None,
            last_event: "Try Retry, a citation, a follow-up, or the composer.".into(),
            _subscription: subscription,
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
        )];
        for index in 0..18 {
            let role = if index % 2 == 0 {
                ChatRole::User
            } else {
                ChatRole::Assistant
            };
            messages.push(ChatMessage::new(
                format!("history-{index}"),
                role,
                StreamedContent::done(format!(
                    "Historical message {index}: compare unit price, delivery window, and inventory risk."
                )),
            ));
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
            ),
            ChatMessage::new("live-answer", ChatRole::Assistant, live_answer)
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
        v_flex()
            .gap(tokens.spacing.xs)
            .child(
                div()
                    .id("chat-story-host")
                    .debug_selector(|| "chat-story-host".into())
                    .h(px(232.))
                    .max_h(px(232.))
                    .flex_none()
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
                    .child("POPULATED — TYPE MARGIN, DELIVERY, OR RISK"),
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
                                    .child("EMPTY CATALOG"),
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
                                    .child("NO RESULTS"),
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
    replacement: Entity<FineTuneCard>,
    no_accent: Entity<FineTuneCard>,
    constrained: Entity<FineTuneCard>,
    values: HashMap<SharedString, FineTuneValues>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl FineTuneStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let populated_values =
            FineTuneValues::new(320., 180., 24., 0.84, "inter-regular").accent(cx.theme().info);
        let replacement_values =
            FineTuneValues::new(640., 360., 32., 0.55, "inter-display").accent(cx.theme().warning);
        let no_accent_values = FineTuneValues::new(280., 180., 14., 1., "jetbrains-mono");
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
        let replacement = cx.new(|cx| {
            FineTuneCard::new(
                "gallery-fine-tune-replacement",
                FineTuneValues::new(240., 160., 8., 1., "inter-regular"),
                [FineTuneTypeface::new("inter-regular", "Inter")],
                window,
                cx,
            )
        });
        replacement.update(cx, |card, cx| {
            card.set_values(replacement_values.clone(), window, cx);
            card.set_typefaces(gallery_typefaces(), cx);
        });
        let no_accent = cx.new(|cx| {
            FineTuneCard::new(
                "gallery-fine-tune-no-accent",
                no_accent_values.clone(),
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
        let subscriptions = [&populated, &replacement, &no_accent, &constrained]
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
            ("gallery-fine-tune-replacement".into(), replacement_values),
            ("gallery-fine-tune-no-accent".into(), no_accent_values),
            ("gallery-fine-tune-constrained".into(), constrained_values),
        ]);

        Self {
            populated,
            replacement,
            no_accent,
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
        let card_width = tokens.spacing.xxl * 9.;
        let full_height = tokens.spacing.xxl * 13.;
        let constrained_height = tokens.spacing.xxl * 7.;
        v_flex()
            .gap(tokens.spacing.sm)
            .child(
                h_flex()
                    .items_start()
                    .flex_wrap()
                    .gap(tokens.spacing.sm)
                    .children([
                        v_flex()
                            .id("fine-tune-populated-host")
                            .role(Role::Group)
                            .aria_label("Populated Fine-tune card")
                            .w(card_width)
                            .gap(tokens.spacing.xs)
                            .child("POPULATED · DUPLICATE TYPEFACE LABELS")
                            .child(div().h(full_height).child(self.populated.clone())),
                        v_flex()
                            .id("fine-tune-replacement-host")
                            .role(Role::Group)
                            .aria_label("Controlled replacement Fine-tune card")
                            .w(card_width)
                            .gap(tokens.spacing.xs)
                            .child("CONTROLLED REPLACEMENT")
                            .child(div().h(full_height).child(self.replacement.clone())),
                        v_flex()
                            .id("fine-tune-no-accent-host")
                            .role(Role::Group)
                            .aria_label("Fine-tune card without an accent")
                            .w(card_width)
                            .gap(tokens.spacing.xs)
                            .child("NO ACCENT")
                            .child(div().h(full_height).child(self.no_accent.clone())),
                        v_flex()
                            .id("fine-tune-constrained-host")
                            .role(Role::Group)
                            .aria_label("Constrained scrolling Fine-tune card")
                            .w(card_width)
                            .gap(tokens.spacing.xs)
                            .child("CONSTRAINED HEIGHT · SCROLL TO APPLY")
                            .child(
                                div()
                                    .h(constrained_height)
                                    .overflow_hidden()
                                    .child(self.constrained.clone()),
                            ),
                    ]),
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
                    .child("EMPTY WITHOUT MODELS"),
            )
            .child(self.empty.clone())
            .child(
                div()
                    .id("prompt-bar-ready-heading")
                    .role(Role::Heading)
                    .aria_label("Ready prompt with mention suggestions")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("READY WITH MENTION SUGGESTIONS"),
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
                    .child("MULTILINE DRAFT"),
            )
            .child(self.multiline.clone())
            .child(
                div()
                    .id("prompt-bar-running-heading")
                    .role(Role::Heading)
                    .aria_label("Running prompt with cancellation")
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("RUNNING WITH CANCELLATION"),
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
                    .h(px(168.))
                    .max_h(px(168.))
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
}

impl GalleryTheme {
    fn next(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Contrast,
            Self::Contrast => Self::Light,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Contrast => "Contrast",
        }
    }
}

fn apply_gallery_theme(preset: GalleryTheme, window: &mut Window, cx: &mut App) {
    let (mode, config) = match preset {
        GalleryTheme::Light => (
            ThemeMode::Light,
            ThemeRegistry::global(cx).default_light_theme().clone(),
        ),
        GalleryTheme::Dark => (
            ThemeMode::Dark,
            ThemeRegistry::global(cx).default_dark_theme().clone(),
        ),
        GalleryTheme::Contrast => {
            let Some(config) = ThemeRegistry::global(cx)
                .themes()
                .get(CONTRAST_THEME_NAME)
                .cloned()
            else {
                return;
            };
            (ThemeMode::Dark, config)
        }
    };

    let theme = Theme::global_mut(cx);
    if mode.is_dark() {
        theme.dark_theme = config;
    } else {
        theme.light_theme = config;
    }
    Theme::change(mode, Some(window), cx);
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
    records: Vec<RecordRow>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex().gap_3()
            .child(Self::state("records-story-populated", "POPULATED AND SORTABLE", self.populated.clone()))
            .child(Self::state("records-story-loading", "LOADING", self.loading.clone()))
            .child(Self::state("records-story-error", "ERROR", self.failed.clone()))
            .child(Self::state("records-story-empty", "EMPTY", self.empty.clone()))
            .child(Self::state("records-story-disabled", "DISABLED ROW", self.disabled.clone()))
            .child(Self::state("records-story-selected", "CONTROLLED SELECTION", self.selected.clone()))
            .child(v_flex().id("records-story-constrained").debug_selector(|| "records-story-constrained".into()).flex_none().gap_1()
                .child(div().id("records-story-constrained-heading").role(Role::Heading).aria_label("Constrained height and width").text_xs().child("CONSTRAINED HEIGHT AND WIDTH"))
                .child(div().w(px(520.)).h(px(180.)).child(self.constrained.clone())))
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
    rows: Vec<DiffRow>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
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
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .gap_3()
            .child(Self::state(
                "diff-story-populated",
                "POPULATED AND SORTABLE",
                self.populated.clone(),
            ))
            .child(Self::state(
                "diff-story-loading",
                "LOADING",
                self.loading.clone(),
            ))
            .child(Self::state(
                "diff-story-error",
                "ERROR",
                self.failed.clone(),
            ))
            .child(Self::state(
                "diff-story-empty",
                "EMPTY",
                self.empty.clone(),
            ))
            .child(Self::state(
                "diff-story-disabled",
                "DISABLED PROPOSAL",
                self.disabled.clone(),
            ))
            .child(Self::state(
                "diff-story-selected",
                "CONTROLLED SELECTION AND DECISION",
                self.selected.clone(),
            ))
            .child(
                v_flex()
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
                            .child("CONSTRAINED HEIGHT AND WIDTH"),
                    )
                    .child(
                        div()
                            .w(px(520.))
                            .h(px(180.))
                            .child(self.constrained.clone()),
                    ),
            )
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

/// Stateful component gallery shared by native and web launchers.
pub struct Gallery {
    selected: StoryId,
    catalog_list: ListState,
    insight_scroll: ScrollHandle,
    prompt_bar_scroll: ScrollHandle,
    selection_actions_scroll: ScrollHandle,
    records_table_scroll: ScrollHandle,
    diff_table_scroll: ScrollHandle,
    visible_range: Range<usize>,
    simulation_task: Option<Task<()>>,
    sim: sim::Simulation,
    trace_open: bool,
    theme: GalleryTheme,
}

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
            visible_range,
            simulation_task: simulation_needed.then(|| Self::spawn_simulation(cx)),
            sim: sim::Simulation::new(),
            trace_open: true,
            theme: theme.unwrap_or_else(|| {
                if cx.theme().is_dark() {
                    GalleryTheme::Dark
                } else {
                    GalleryTheme::Light
                }
            }),
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

    /// Prepares one native performance viewport from a stable story identity.
    ///
    /// The dedicated Streaming Text viewport measures ongoing progressive
    /// invalidation. Chat therefore stops only the gallery-owned fake producer
    /// after its transition so its equal steady sample window measures the
    /// populated virtual transcript and composer rather than duplicating the
    /// streaming scenario.
    #[cfg(any(test, feature = "performance"))]
    pub fn prepare_performance_viewport(&mut self, story: StoryId, cx: &mut Context<Self>) {
        self.scroll_catalog_to(story, cx);
        if story == StoryId::Chat {
            self.simulation_task.take();
        }
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
                "LOADING STATE",
                || {
                    LoadingState::new()
                        .label("Reasoning about supplier pricing")
                        .elapsed(elapsed)
                },
                cx,
            ),
            StoryId::ToolChips => self.section(
                story,
                "TOOL CHIPS",
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
                "TASK ROWS",
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
                "THINKING",
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
                "ORBS",
                || {
                    h_flex()
                        .items_center()
                        .gap_6()
                        .child(Orbs::new())
                        .child(Orbs::new().diameter(px(64.)))
                        .child(
                            div()
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child("ambient thinking indicator"),
                        )
                },
                cx,
            ),
            StoryId::Search => self.section(
                story,
                "WEB SEARCH",
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
                "TO-DO LIST",
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
                "IMAGE GENERATION",
                || {
                    ImageGeneration::new("gen")
                        .label("Label sketch: alpine meadow, morning light")
                        .progress(self.sim.progress())
                        .image(
                            v_flex()
                                .size_full()
                                .child(div().flex_1().bg(cx.theme().info.opacity(0.35)))
                                .child(div().flex_1().bg(cx.theme().cyan.opacity(0.35)))
                                .child(div().flex_1().bg(cx.theme().success.opacity(0.25))),
                        )
                },
                cx,
            ),
            StoryId::StreamingText => self.section(
                story,
                "STREAMING TEXT",
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
                                "**Reference comparison.** Beautiful UI pairs an inline `scoopdata.io` source with a separate 10-source footer and follow-ups. mighty-gpui deliberately adds stable typed citation routing and keyboard/AccessKit companion links because the pinned Markdown glyph link is pointer-only.",
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
                    "CHAT",
                    || chat_story,
                    cx,
                )
            }
            StoryId::CommandSearch => {
                let command_story = window.use_keyed_state(
                    "command-search-story-state",
                    cx,
                    CommandSearchStory::new,
                );
                self.section(
                    story,
                    "COMMAND SEARCH",
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
                self.section(story, "SIDEBAR NAVIGATION", || sidebar_story, cx)
            }
            StoryId::FineTune => {
                let fine_tune_story = window.use_keyed_state(
                    "fine-tune-story-state",
                    cx,
                    FineTuneStory::new,
                );
                self.section(story, "FINE-TUNE CARD", || fine_tune_story, cx)
            }
            StoryId::RecordsTable => {
                let records_story = window.use_keyed_state(
                    "records-table-story-state",
                    cx,
                    RecordsTableStory::new,
                );
                self.section(
                    story,
                    "RECORDS TABLE",
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
                    "DIFF TABLE",
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
            StoryId::CodeBlock => self.section(
                story,
                "CODE BLOCK",
                || CodeBlock::streamed("code", &self.sim.code).language("rust"),
                cx,
            ),
            StoryId::Approval => self.section(
                story,
                "APPROVAL CARD",
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
                "RECOMMENDATION CARD",
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
                "CONTEXT CARDS",
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
                "INSIGHT CARD",
                || {
                    v_flex()
                        .id("insight-story-scroll")
                        .debug_selector(|| "insight-story-scroll".into())
                        .h(px(256.))
                        .max_h(px(256.))
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
                    "PROMPT BAR",
                    || {
                        v_flex()
                            .id("prompt-bar-story-scroll")
                            .debug_selector(|| "prompt-bar-story-scroll".into())
                            .h(px(256.))
                            .max_h(px(256.))
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
            StoryId::SelectionActions => {
                let selection_story = window.use_keyed_state(
                    "selection-actions-story-state",
                    cx,
                    SelectionActionsStory::new,
                );
                self.section(
                    story,
                    "SELECTION ACTIONS",
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
                                apply_gallery_theme(this.theme, window, cx);
                                cx.notify();
                            })),
                    ),
            )
            .child(content)
    }
}

/// Initializes the component and theme globals used by every gallery host.
pub fn init(cx: &mut App) {
    mighty_gpui::init(cx);
    ThemeRegistry::global_mut(cx)
        .load_themes_from_str(CONTRAST_THEME)
        .expect("embedded gallery theme must be valid");
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
            apply_gallery_theme(theme, window, cx);
        }
        cx.new(|cx| Root::new(root_view, window, cx).bg(cx.theme().background))
    })
    .expect("failed to open gallery window");
    view
}

#[cfg(test)]
mod tests {
    use super::{ChatStory, Gallery, GalleryTheme};
    use crate::StoryId;
    use gpui::{
        AppContext as _, Context, Element as _, IntoElement as _, Render, Role, ScrollDelta,
        ScrollWheelEvent, TestAppContext, VisualTestContext, Window, accesskit, point, px, size,
    };
    use mighty_gpui::stream::StreamedContent;
    use std::{cell::RefCell, rc::Rc};

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
        assert_eq!(GalleryTheme::Light.next(), GalleryTheme::Dark);
        assert_eq!(GalleryTheme::Dark.next(), GalleryTheme::Contrast);
        assert_eq!(GalleryTheme::Contrast.next(), GalleryTheme::Light);
    }

    #[gpui::test]
    fn all_stories_exposes_a_vertical_scrollbar(cx: &mut TestAppContext) {
        let (_, cx) = all_stories(cx);

        assert!(cx.debug_bounds("scrollbar-overlay").is_some());
    }

    #[gpui::test]
    fn direct_records_table_story_exercises_required_controlled_states(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::RecordsTable, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(560.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        for selector in [
            "records-story-populated",
            "records-story-loading",
            "records-story-error",
            "records-story-empty",
            "records-story-disabled",
            "records-story-selected",
            "records-story-constrained",
            "records-story-reference-note",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "records story should exercise {selector}"
            );
        }
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
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        for selector in [
            "diff-story-populated",
            "diff-story-loading",
            "diff-story-error",
            "diff-story-empty",
            "diff-story-disabled",
            "diff-story-selected",
            "diff-story-constrained",
            "diff-story-reference-note",
        ] {
            assert!(
                cx.debug_bounds(selector).is_some(),
                "diff story should exercise {selector}"
            );
        }
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

        gallery.read_with(cx, |gallery, _| {
            assert_eq!(gallery.catalog_list.logical_scroll_top().item_ix, 8);
            assert_eq!(gallery.visible_range, 8..11);
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
            assert_eq!(gallery.catalog_list.logical_scroll_top().item_ix, 9);
            assert_eq!(gallery.visible_range, 9..12);
            assert!(gallery.simulation_task.is_none());
        });
    }
}
