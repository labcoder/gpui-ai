//! Native component gallery for mighty-gpui.
//!
//! One story per component, driven by simulated agent activity (fake token
//! streams, task lifecycles). All simulation lives here — the library
//! components only ever see data.

use crate::{StoryId, sim};

use gpui::*;
use gpui_component::{
    ActiveTheme as _, Root, StyledExt as _,
    button::Button,
    h_flex,
    scroll::ScrollableElement as _,
    text::TextView,
    theme::{Theme, ThemeMode, ThemeRegistry},
    v_flex,
};
use mighty_gpui::prelude::*;
use std::{ops::Range, time::Duration};

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
        StoryId::ImageGeneration | StoryId::StreamingText => delta.answer_content_changed(),
        StoryId::CodeBlock => delta.code_content_changed() || delta.code_phase_changed(),
        StoryId::All
        | StoryId::ToolChips
        | StoryId::Orbs
        | StoryId::Todos
        | StoryId::Approval
        | StoryId::Recommendation
        | StoryId::Context
        | StoryId::Insights
        | StoryId::PromptBar => false,
    }
}

struct PromptBarStory {
    ready: Entity<PromptBar>,
    running: Entity<PromptBar>,
    last_event: SharedString,
    _subscriptions: Vec<Subscription>,
}

impl PromptBarStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
        let ready_subscription = cx.subscribe_in(
            &ready,
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
            ready,
            running,
            last_event: "Interact with either composer to inspect its typed event.".into(),
            _subscriptions: vec![ready_subscription, running_subscription],
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

/// Stateful component gallery shared by native and web launchers.
pub struct Gallery {
    selected: StoryId,
    catalog_list: ListState,
    insight_scroll: ScrollHandle,
    prompt_bar_scroll: ScrollHandle,
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
                    StreamingText::new("answer", &self.sim.answer)
                        .sources(["pricing.md", "suppliers.csv", "orders 2026"])
                        .follow_ups([
                            FollowUp::new("compare-delivery", "Compare delivery times"),
                            FollowUp::new("price-history", "Show price history"),
                        ])
                        .on_event(cx.listener(|_, event: &StreamingTextEvent, _, _| {
                            println!("streaming-text event: {event:?}");
                        }))
                },
                cx,
            ),
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
        }
    }
}

impl Render for Gallery {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.selected == StoryId::All {
            div()
                .id("gallery-scroll")
                .flex_1()
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
                .flex_1()
                .overflow_y_scrollbar()
                .child(self.render_story(self.selected, window, cx))
                .into_any_element()
        };

        v_flex()
            .size_full()
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
    use super::{Gallery, GalleryTheme};
    use crate::StoryId;
    use gpui::{
        AppContext as _, Element as _, IntoElement as _, Role, ScrollDelta, ScrollWheelEvent,
        TestAppContext, VisualTestContext, accesskit, point, px, size,
    };
    use gpui_component::Root;
    use std::{cell::RefCell, rc::Rc};

    fn all_stories(cx: &mut TestAppContext) -> (gpui::Entity<Gallery>, &mut VisualTestContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(|window, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::All, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            Root::new(gallery, window, cx)
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
    fn all_stories_virtualizes_distant_rows_until_they_enter_view(cx: &mut TestAppContext) {
        let (_, cx) = all_stories(cx);
        let viewport_height = cx.update(|window, _| window.viewport_size().height);
        assert!(cx.debug_bounds("story-loading").is_some());
        assert!(
            cx.debug_bounds("story-prompt-bar").is_none(),
            "the final story should not be constructed before it nears the viewport"
        );

        for _ in StoryId::ALL {
            if cx.debug_bounds("story-prompt-bar").is_some() {
                break;
            }
            scroll(cx, -10_000.);
        }

        let scrolled = cx
            .debug_bounds("story-prompt-bar")
            .expect("prompt bar story should render after scrolling to it");
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
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::Insights, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            Root::new(gallery, window, cx)
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
        let (_, cx) = cx.add_window_view(move |window, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::PromptBar, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            Root::new(gallery, window, cx)
        });
        cx.simulate_resize(size(px(700.), px(400.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

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
        assert!(cx.debug_bounds("story-prompt-bar").is_some());

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
}
