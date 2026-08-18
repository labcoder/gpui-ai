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
    theme::{Theme, ThemeMode, ThemeRegistry},
    v_flex,
};
use mighty_gpui::prelude::*;
use std::time::Duration;

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
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(sim::TICK_INTERVAL).await;
                let alive = this.update(cx, |this, cx| {
                    this.sim.tick();
                    cx.notify();
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        Self {
            selected,
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

    /// Updates the displayed preset after a gallery host changes the theme.
    pub fn set_theme_preset(&mut self, theme: GalleryTheme, cx: &mut Context<Self>) {
        self.theme = theme;
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

        v_flex()
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
}

impl Render for Gallery {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let elapsed = self.sim.elapsed();

        div()
            .id("gallery-scroll")
            .size_full()
            .overflow_y_scroll()
            .bg(cx.theme().background)
            .flex()
            .justify_center()
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(640.))
                    .p_6()
                    .gap_8()
                    .child(
                        h_flex()
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
                    .child(self.section(
                        StoryId::Loading,
                        "LOADING STATE",
                        || {
                            LoadingState::new()
                                .label("Reasoning about supplier pricing")
                                .elapsed(elapsed)
                        },
                        cx,
                    ))
                    .child(self.section(
                        StoryId::ToolChips,
                        "TOOL CHIPS",
                        || {
                            h_flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    ToolChip::new("chip-1", "read pricing.md")
                                        .status(ToolStatus::Success),
                                )
                                .child(
                                    ToolChip::new("chip-2", "edit suppliers.rs")
                                        .status(ToolStatus::Running)
                                        .detail("+12 −3"),
                                )
                                .child(
                                    ToolChip::new("chip-3", "run cargo test")
                                        .status(ToolStatus::Pending),
                                )
                                .child(
                                    ToolChip::new("chip-4", "query prices-db")
                                        .status(ToolStatus::Failed)
                                        .detail("timeout"),
                                )
                        },
                        cx,
                    ))
                    .child(self.section(
                        StoryId::Tasks,
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
                    ))
                    .child(self.section(
                        StoryId::Thinking,
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
                            Thinking::new("trace", &trace)
                                .open(self.trace_open)
                                .on_event(cx.listener(|this, event: &ThinkingEvent, _, cx| {
                                    let ThinkingEvent::Toggled { open, .. } = event;
                                    this.trace_open = *open;
                                    cx.notify();
                                }))
                        },
                        cx,
                    ))
                    .child(self.section(
                        StoryId::Thinking,
                        "THINKING — PROSE",
                        || {
                            let trace = Progressive::complete(
                                ThinkingTrace::new()
                                    .thought_for(Duration::from_secs(4))
                                    .prose(
                                        "The March sheet supersedes prior quotes, so the \
                                     comparison should use *current* volume tiers. \
                                     Delivery windows match, which removes the main \
                                     switching risk.",
                                    ),
                            );
                            Thinking::new("trace-prose", &trace).open(true)
                        },
                        cx,
                    ))
                    .child(self.section(
                        StoryId::Orbs,
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
                    ))
                    .child(self.section(
                        StoryId::Search,
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
                    ))
                    .child(self.section(
                        StoryId::Todos,
                        "TO-DO LIST",
                        || {
                            TodoList::new("plan")
                                .title("Supplier switch plan")
                                .items([
                                    TodoItem::new("contract-terms", "Pull current contract terms")
                                        .done(),
                                    TodoItem::new("compare-prices", "Compare Q3 unit prices")
                                        .done(),
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
                    ))
                    .child(self.section(
                        StoryId::ImageGeneration,
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
                    ))
                    .child(self.section(
                        StoryId::StreamingText,
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
                    ))
                    .child(self.section(
                        StoryId::CodeBlock,
                        "CODE BLOCK",
                        || CodeBlock::streamed("code", &self.sim.code).language("rust"),
                        cx,
                    ))
                    .child(self.section(
                        StoryId::Approval,
                        "APPROVAL CARD",
                        || {
                            ApprovalCard::new("gate", "Send order confirmation to 3 suppliers?")
                                .description(
                                    "Emails will go out immediately and cannot be recalled.",
                                )
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
                    ))
                    .child(self.section(
                        StoryId::Recommendation,
                        "RECOMMENDATION CARD",
                        || {
                            RecommendationCard::new("rec", "Switch supplier to Alpenrose Dairy")
                                .description(
                                    "Lower unit cost at equal volume; delivery risk unchanged.",
                                )
                                .confidence(0.87)
                                .alternatives(["Keep current supplier", "Split volume 60/40"])
                                .on_event(cx.listener(|_, event: &RecommendationEvent, _, _| {
                                    println!("recommendation event: {event:?}");
                                }))
                        },
                        cx,
                    ))
                    .child(self.section(
                        StoryId::Context,
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
                                        .on_event(cx.listener(
                                            |_, event: &ContextCardEvent, _, _| {
                                                println!("context event: {event:?}");
                                            },
                                        )),
                                )
                                .child(
                                    ContextCard::new("ctx-2", "suppliers.csv")
                                        .snippet("Alpenrose Dairy, Portland OR, net-30, 4.8 rating")
                                        .relevance(0.81),
                                )
                        },
                        cx,
                    )),
            )
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
    use super::GalleryTheme;

    #[test]
    fn gallery_theme_cycle_covers_all_review_presets() {
        assert_eq!(GalleryTheme::Light.next(), GalleryTheme::Dark);
        assert_eq!(GalleryTheme::Dark.next(), GalleryTheme::Contrast);
        assert_eq!(GalleryTheme::Contrast.next(), GalleryTheme::Light);
    }
}
