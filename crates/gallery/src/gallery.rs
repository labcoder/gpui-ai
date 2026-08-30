//! Native component gallery for gpui-ai.
//!
//! One story per component, driven by simulated agent activity (fake token
//! streams, task lifecycles). All simulation lives here — the library
//! components only ever see data.

use crate::dock_composition::DockCompositionStory;
use crate::motion_lab::MotionLabStory;
use crate::{StoryId, sim};

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui::{Image, ImageFormat};
use gpui_ai::cues::{self, Cue, CueSubscription};
use gpui_ai::prelude::*;
use gpui_component::Sizable as _;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_component::{
    ActiveTheme as _, IconName, Root, StyledExt as _,
    button::{Button, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    text::TextView,
    theme::{Theme, ThemeConfig, ThemeMode, ThemeRegistry},
    v_flex,
};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::atomic::{AtomicU32, Ordering};

// Catalog keyboard actions: page and jump navigation for the story feed.
actions!(
    catalog,
    [
        PageUp,
        PageDown,
        ScrollHome,
        ScrollEnd,
        CancelAutoscroll,
        ToggleMetrics
    ]
);

/// Distance scrolled by one Page Up/Down, as a fraction of the catalog
/// viewport. Short of a full page so the row that was at the edge stays on
/// screen and the reader keeps their place.
const PAGE_FRACTION: f32 = 0.9;

/// Pacing requested between autoscroll frames, near a 60Hz frame.
///
/// The timer only *asks* for this cadence; each tick scrolls by the time that
/// actually elapsed, so a late frame still travels the right distance.
const AUTOSCROLL_FRAME_INTERVAL: Duration = Duration::from_millis(16);
use std::{collections::HashMap, ops::Range, sync::Arc, time::Duration};

/// One preset in the generated theme table.
///
/// The table is produced by `build.rs` from the `themes/` directory, so adding
/// a JSON file there adds a preset with no Rust change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GalleryThemeEntry {
    slug: &'static str,
    label: &'static str,
    /// The name the theme registers under, or `None` for gpui-component's own
    /// default light and dark themes.
    registry_name: Option<&'static str>,
    group: &'static str,
    dark: bool,
}

include!(concat!(env!("OUT_DIR"), "/themes.rs"));

/// The group name for presets that ship with gpui-ai.
pub const GPUI_AI_THEME_GROUP: &str = "gpui-ai";

fn story_list_frame() -> Stateful<Div> {
    div()
        .id("gallery-story-list")
        .accessibility_id("gallery.story-list")
        .role(Role::List)
        .aria_label("Component stories")
}

/// The tallest each catalog row has been, and the width it earned that at.
///
/// A story is taller when its text wraps more, so a height reached at one
/// width is not a floor at another — without the width, narrowing a window
/// and widening it again would leave every row holding the height it
/// reached at its narrowest.
#[derive(Default)]
struct CatalogFloors {
    width: f32,
    rows: HashMap<StoryId, f32>,
}

impl CatalogFloors {
    /// The floor recorded for `story`, discarding everything first if these
    /// measurements were taken at a different width.
    fn floor_at(&mut self, width: f32, story: StoryId) -> f32 {
        if self.width != width {
            self.width = width;
            self.rows.clear();
        }
        self.rows.get(&story).copied().unwrap_or_default()
    }

    /// Raises `story`'s floor if it has just drawn taller than before.
    fn observe(&mut self, story: StoryId, height: f32) {
        let recorded = self.rows.entry(story).or_default();
        if height > *recorded {
            *recorded = height;
        }
    }
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

/// The height the story last laid out at, in logical pixels.
///
/// A story's height is a function of the width it is given, and not a step
/// function: text rewraps a line at a time, so between 300 and 640 pixels
/// every story that carries prose changes height continuously. Measuring at a
/// few widths and publishing a table would be wrong nearly everywhere in that
/// range, and the range is a phone, a tablet, and a half-width window.
///
/// So the story says what it measured, and the page around it listens. The
/// catalog still carries measured numbers, but only as the height to reserve
/// before a demo starts — they are a promise about the space, not about the
/// story.
///
/// A plain static rather than a GPUI global: the wasm bridge reads it from a
/// free function with no `App` in hand, and there is exactly one story in a
/// window.
static MEASURED_HEIGHT: AtomicU32 = AtomicU32::new(0);

/// What the story in this window last laid out at, if it has laid out at all.
pub fn measured_story_height() -> Option<u32> {
    match MEASURED_HEIGHT.load(Ordering::Relaxed) {
        0 => None,
        height => Some(height),
    }
}

/// An invisible element the size of the story, which records what that is.
///
/// Absolutely positioned over a relative wrapper whose only in-flow child is
/// the frame, so its bounds are the frame's own — a canvas placed inside the
/// frame would measure the padding box and report a height 32 pixels short.
fn height_probe() -> impl IntoElement {
    canvas(
        move |bounds, _, _| {
            let height = f32::from(bounds.size.height).ceil().max(0.) as u32;
            MEASURED_HEIGHT.store(height, Ordering::Relaxed);
        },
        |_, _, _, _| {},
    )
    .absolute()
    .inset_0()
}

/// The state switcher the story currently on screen is drawing, if it has one.
///
/// Five of the thirty-four stories offer states to switch between — the chat
/// story and the four tables — and each draws its own switcher inside the
/// canvas, owned by its own entity. Nothing outside the canvas could reach one:
/// the entities live in the window's keyed state, and their setters have five
/// different signatures.
///
/// So the switcher registers itself as it draws, boxed down to "put this index
/// on screen". That is what lets an address name a state — `variant=welcome`
/// — and what lets Copy link give back the state the reader was actually
/// looking at rather than the one the story opens in.
///
/// One slot, because the embed draws one story. The full gallery draws
/// thirty-four and the last one wins, which is meaningless there and unread:
/// the bridge that uses this exists only on the web, and the web draws the
/// embed.
type VariantSetter = Rc<dyn Fn(usize, &mut App) -> bool>;

thread_local! {
    static ACTIVE_SWITCHER: RefCell<Option<RegisteredSwitcher>> = const { RefCell::new(None) };
}

/// The switcher currently registered, and which story it belongs to.
///
/// The slug is what lets a redraw update the index without building a new
/// setter: a switcher registers every time it draws, and the embed redraws
/// its one story continuously, so allocating a fresh boxed closure each
/// frame bought nothing.
struct RegisteredSwitcher {
    slug: &'static str,
    index: usize,
    apply: VariantSetter,
}

/// Which state the story on screen is showing, if it offers any.
pub fn active_variant_index() -> Option<usize> {
    ACTIVE_SWITCHER.with(|switcher| switcher.borrow().as_ref().map(|current| current.index))
}

/// Puts the story on screen into one of the states it offers.
///
/// Returns whether there was a switcher to tell.
pub fn set_active_variant(index: usize, cx: &mut App) -> bool {
    let Some(apply) = ACTIVE_SWITCHER.with(|switcher| {
        switcher
            .borrow()
            .as_ref()
            .map(|current| Rc::clone(&current.apply))
    }) else {
        return false;
    };
    apply(index, cx)
}

/// Ticks the height measurement runs a streaming story for.
///
/// The answer stream is the longest and finishes well inside this; the code
/// stream starts halfway through it.
#[cfg(test)]
const MEASURE_TICKS: usize = 220;

/// Where `site/test/release/mobile.test.mjs` taps to open the keyboard.
///
/// In CSS pixels inside the embed, at the 390px viewport that suite drives.
/// Kept here because the layout it points into lives here;
/// `the_mobile_suites_tap_lands_on_the_prompt_bar_composer` is what keeps
/// the two in step.
#[cfg(test)]
const MOBILE_COMPOSER_TAP: (f32, f32) = (100., 150.);

/// Where that same suite taps to dismiss the keyboard again.
///
/// Has to miss the composer, or the blur it is checking never happens.
#[cfg(test)]
const MOBILE_COMPOSER_BLUR: (f32, f32) = (8., 200.);

fn story_needs_simulation(story: StoryId) -> bool {
    matches!(
        story,
        StoryId::Loading
            | StoryId::Status
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

impl StoryStates for TableStoryState {
    const ALL: &'static [Self] = &[
        Self::Populated,
        Self::Loading,
        Self::Error,
        Self::Empty,
        Self::Disabled,
        Self::Selected,
        Self::Constrained,
    ];

    const LABELS: &'static [(&'static str, &'static str)] = crate::story::TABLE_STORY_VARIANTS;
}

/// The states one multi-state story switches between.
///
/// Implemented by each story's own state enum rather than by a shared enum:
/// what a table demonstrates and what a composer demonstrates have nothing
/// in common but the shape. The shape is all this names — the states in
/// switcher order, the labels parallel to them, and the position lookup the
/// toolbar needs, which was written out three times before.
trait StoryStates: Copy + PartialEq + Sized + 'static {
    /// Every demonstrated state, in the order the switcher offers them.
    const ALL: &'static [Self];

    /// Switcher labels, parallel to [`Self::ALL`].
    const LABELS: &'static [(&'static str, &'static str)];

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
    let registered = owner.clone();
    ACTIVE_SWITCHER.with(|switcher| {
        let mut switcher = switcher.borrow_mut();
        // The same story redrawing only moved its index; the setter it
        // registered still points at the same entity.
        if let Some(current) = switcher.as_mut()
            && current.slug == slug
        {
            current.index = active_index;
            return;
        }
        *switcher = Some(RegisteredSwitcher {
            slug,
            index: active_index,
            // The entity may be gone — a switcher registers as it draws and
            // nothing unregisters it — so whether the story took the change is
            // the answer, not whether a setter was found.
            apply: Rc::new(move |index: usize, cx: &mut App| {
                registered
                    .update(cx, |story, cx| apply(story, index, cx))
                    .is_ok()
            }) as VariantSetter,
        });
    });

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
                                .text_label(*label)
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
        StoryId::Loading | StoryId::Status | StoryId::Tasks => true,
        StoryId::Thinking | StoryId::Search | StoryId::ToolCalls => delta.answer_phase_changed(),
        StoryId::ImageGeneration | StoryId::StreamingText | StoryId::Chat => {
            delta.answer_content_changed()
        }
        StoryId::CodeBlock => delta.code_content_changed() || delta.code_phase_changed(),
        StoryId::All
        | StoryId::GuidedDemo
        | StoryId::ThemesTrio
        | StoryId::DockComposition
        | StoryId::MotionLab
        | StoryId::Suggestions
        | StoryId::Attachments
        | StoryId::Artifact
        | StoryId::Voice
        | StoryId::Queue
        | StoryId::CodeDiff
        | StoryId::Plan
        | StoryId::ContextMeter
        | StoryId::ThreadList
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
        | StoryId::SelectionActions
        | StoryId::Form
        | StoryId::QuestionFlow
        | StoryId::Decorations => false,
    }
}

struct ChatStory {
    chat: Entity<Chat>,
    answer: Option<StreamedContent>,
    last_event: SharedString,
    show_welcome: bool,
    last_cue: Option<Cue>,
    question_version: usize,
    question_text: Option<SharedString>,
    _subscription: Subscription,
    _cues: CueSubscription,
}

/// The versions a person might branch the latest question into.
const QUESTION_VERSIONS: &[&str] = &[
    "Which supplier is the safest choice this week?",
    "Which supplier is the cheapest this week?",
    "Which supplier can deliver by Friday?",
];

/// Switcher labels for the two demonstrated chat states.
const CHAT_STORY_STATES: &[(&str, &str)] = crate::story::CHAT_STORY_VARIANTS;

impl ChatStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prompt = cx.new(|cx| {
            // snippet:start(prompt-bar)
            let mut prompt = PromptBar::new("gallery-chat-prompt", window, cx);
            prompt.set_models(
                [
                    PromptModel::new("balanced", "Balanced")
                        .provider("Anthropic")
                        .description("Claude Sonnet 5 · everyday work")
                        .context_window(200_000),
                    PromptModel::new("fast", "Fast")
                        .provider("Anthropic")
                        .description("Claude Haiku 4.5 · quick replies")
                        .context_window(200_000),
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
            // snippet:end
        });
        let chat = cx.new(|cx| {
            // snippet:start(chat)
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
            // snippet:end
        });
        // snippet:start(chat)
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
                    ChatEvent::BranchSelected { message_id, index } => {
                        if message_id.as_ref() == "latest-question" {
                            this.question_version = *index;
                            this.question_text = None;
                            this.answer = None;
                        }
                    }
                    ChatEvent::EditSubmitted { message_id, text } => {
                        if message_id.as_ref() == "latest-question" {
                            this.question_text = Some(text.clone());
                            this.answer = None;
                        }
                    }
                    ChatEvent::EditCancelled { .. } | ChatEvent::AttachmentActivated { .. } => {}
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
                    | ChatEvent::SourceActivated { .. }
                    | ChatEvent::MessageCopied { .. }
                    | ChatEvent::RegenerateRequested { .. }
                    | ChatEvent::EditRequested { .. }
                    | ChatEvent::FeedbackSubmitted { .. }
                    | ChatEvent::JumpedToLatest => {}
                }
                cx.notify();
            },
        );
        // snippet:end
        // Cues arrive while the emitting entity is still borrowed, so the
        // readout updates on the next turn of the loop.
        let story = cx.weak_entity();
        let cue_subscription = cues::observe(cx, move |cue, cx| {
            let cue = cue.clone();
            let story = story.clone();
            cx.defer(move |cx| {
                story
                    .update(cx, |this, cx| {
                        this.last_cue = Some(cue);
                        cx.notify();
                    })
                    .ok();
            });
        });
        Self {
            chat,
            answer: None,
            last_event: "Hover a message for actions, or try Retry, a citation, or the composer."
                .into(),
            show_welcome: false,
            last_cue: None,
            question_version: 0,
            question_text: None,
            _subscription: subscription,
            _cues: cue_subscription,
        }
    }

    /// The latest question: an in-place edit wins over the branch version.
    fn question_text(&self) -> String {
        self.question_text
            .as_ref()
            .map(|text| text.to_string())
            .unwrap_or_else(|| QUESTION_VERSIONS[self.question_version].to_owned())
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
            "system-note",
            ChatRole::System,
            StreamedContent::done(
                "This thread virtualizes by stable application IDs, preserves the top item and its pixel offset while you read back through history, exposes unread state, and leaves every async producer to the application.",
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
            // bubble, assistant replies lead unframed. Applications choose
            // the appearance per message.
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
                StreamedContent::done(self.question_text()),
            )
            .branch(BranchPosition::new(
                self.question_version,
                QUESTION_VERSIONS.len(),
            ))
            .attachments([Attachment::new("pricing", "pricing.md")
                .size_bytes(12_300)
                .detail("12 pages")])
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
        let cue_label = self.last_cue.as_ref().map_or_else(
            || "none yet (sound hooks fire here)".to_owned(),
            describe_cue,
        );
        // A chat panel needs vertical room to show a real conversation —
        // transcript history, tool results, the streaming answer, and the
        // composer. 232px crushed all of that into ~1.5 visible messages;
        // 480px shows the whole arc: history, a tool result, and the answer.
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
            .child(
                div()
                    .id("chat-story-cue")
                    .debug_selector(|| "chat-story-cue".into())
                    .role(Role::Status)
                    .aria_label(format!("Last cue: {cue_label}"))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last cue: {cue_label}")),
            )
    }
}

/// Human wording for the cue readout under the chat story.
fn describe_cue(cue: &Cue) -> String {
    match cue {
        Cue::MessageArrived { message_id } => format!("message {message_id} arrived"),
        Cue::ResponseSettled {
            message_id,
            succeeded: true,
        } => format!("response {message_id} completed"),
        Cue::ResponseSettled { message_id, .. } => format!("response {message_id} failed"),
        Cue::Copied => "copied".to_owned(),
        Cue::Submitted => "submitted".to_owned(),
        Cue::Cancelled => "cancelled".to_owned(),
        Cue::SuggestionSelected => "suggestion selected".to_owned(),
        Cue::ThreadSelected => "thread selected".to_owned(),
        Cue::Decided { approved: true } => "approved".to_owned(),
        Cue::Decided { approved: false } => "rejected".to_owned(),
    }
}

/// Files a person might attach to a supplier question; shared by the
/// composer and message strips so identity carries across.
fn demo_attachments(thumbnail: &Arc<Image>) -> Vec<Attachment> {
    vec![
        Attachment::new("meadow", "meadow-sketch.png")
            .size_bytes(248_000)
            .detail("1280×720")
            .thumbnail(thumbnail.clone()),
        Attachment::new("pricing", "pricing.md")
            .size_bytes(12_300)
            .detail("12 pages"),
        Attachment::new("sales", "sales-q2.csv").size_bytes(1_400_000),
        Attachment::new("call", "vendor-call.m4a").detail("4:12"),
    ]
}

/// The upload lifecycle as three tiles.
fn demo_attachment_states() -> Vec<Attachment> {
    vec![
        Attachment::new("queued", "brief.pdf")
            .size_bytes(820_000)
            .state(ProgressState::Pending),
        Attachment::new("uploading", "hero-render.png")
            .size_bytes(3_200_000)
            .state(ProgressState::Running)
            .progress(0.4),
        Attachment::new("too-large", "archive.zip")
            .size_bytes(41_000_000)
            .state(ProgressState::Failed("Larger than 25 MB".into())),
    ]
}

/// A small agent-proposed patch for the code diff story.
const DEMO_PATCH: &str = "--- a/src/pricing.rs\n+++ b/src/pricing.rs\n@@ -12,7 +12,9 @@ impl Quote {\n     pub fn unit_price(&self) -> Money {\n-        self.total / self.units\n+        // Guard against empty orders reported by the catalog sync.\n+        let units = self.units.max(1);\n+        self.total / units\n     }\n \n     pub fn discount(&self) -> f32 {\n@@ -31,4 +33,4 @@ impl Quote {\n     fn volume_tier(&self) -> Tier {\n-        if self.units > 500 { Tier::Wholesale } else { Tier::Retail }\n+        if self.units >= 500 { Tier::Wholesale } else { Tier::Retail }\n     }\n }\n";

/// Three versions of the generated comparison; the last is still streaming.
const ARTIFACT_VERSIONS: &[(&str, &str)] = &[
    (
        "v1",
        "# Supplier comparison\n\n| Supplier | Unit price | Delivery |\n| --- | --- | --- |\n| Alpenrose Dairy | $4.12 | 2 days |\n| Tillamook County Creamery | $4.43 | 3 days |\n| Cascade Cultured Foods | $4.37 | 4 days |\n\nAlpenrose is cheapest at the current volume.",
    ),
    (
        "v2",
        "# Supplier comparison\n\n| Supplier | Unit price | Delivery | Risk |\n| --- | --- | --- | --- |\n| Alpenrose Dairy | $4.12 | 2 days | Low |\n| Tillamook County Creamery | $4.43 | 3 days | Low |\n| Cascade Cultured Foods | $4.37 | 4 days | Medium |\n\n## Recommendation\n\nSwitch bulk orders to **Alpenrose Dairy**: 7% lower unit cost with the shortest delivery window and no change in cold-chain risk.",
    ),
    (
        "v3",
        "# Supplier comparison\n\n| Supplier | Unit price | Delivery | Risk |\n| --- | --- | --- | --- |\n| Alpenrose Dairy | $4.12 | 2 days | Low |\n| Tillamook County Creamery | $4.43 | 3 days | Low |\n| Cascade Cultured Foods | $4.37 | 4 days | Medium |\n\n## Recommendation\n\nSwitch bulk orders to **Alpenrose Dairy**.\n\n## Next steps\n\n1. Confirm the wholesale tier at 500 units.\n2. Draft the revised order",
    ),
];

/// Prompts a person might type while the agent is busy.
fn demo_queue() -> Vec<QueuedMessage> {
    vec![
        QueuedMessage::new(
            "queued-compare",
            "Compare the three suppliers for next month",
        )
        .note("after the current step"),
        QueuedMessage::new("queued-draft", "Draft the order confirmations"),
        QueuedMessage::new(
            "queued-risks",
            "Summarize every risk from the cold-chain review and propose mitigations",
        )
        .note("steering"),
    ]
}

pub(crate) fn demo_thread_sections() -> Vec<ThreadSection> {
    vec![
        ThreadSection::new("today", "Today").items([
            ThreadItem::new("supplier-pricing", "Supplier pricing review")
                .subtitle("2 min ago · Alpenrose is 7% cheaper"),
            ThreadItem::new("cold-chain", "Cold-chain capacity check").subtitle("1 h ago"),
        ]),
        ThreadSection::new("yesterday", "Yesterday").items([
            ThreadItem::new("confirmations", "Draft order confirmations")
                .subtitle("Yesterday · 3 suppliers"),
            ThreadItem::new("delivery-windows", "Delivery window options").subtitle("Yesterday"),
        ]),
        ThreadSection::new("earlier", "Earlier").items([
            ThreadItem::new("margins", "Q2 margin analysis").subtitle("Aug 12"),
            ThreadItem::new("forecast", "Flavor demand forecast").subtitle("Aug 9"),
            ThreadItem::new("packaging", "Packaging vendor shortlist")
                .subtitle("Aug 3")
                .archived(true),
            ThreadItem::new("onboarding", "Onboarding checklist")
                .subtitle("Jul 28")
                .archived(true),
        ]),
    ]
}

struct ThreadListStory {
    threads: Entity<ThreadList>,
    sections: Vec<ThreadSection>,
    created: usize,
    last_event: SharedString,
    _subscription: Subscription,
}

impl ThreadListStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let sections = demo_thread_sections();
        let threads = cx.new(|cx| {
            // snippet:start(thread-list)
            let mut list = ThreadList::new("gallery-threads", window, cx);
            list.set_sections(sections.clone(), cx);
            list.set_active(Some("supplier-pricing"), cx);
            // snippet:end
            list
        });
        let subscription = cx.subscribe_in(
            &threads,
            window,
            |this, threads, event: &ThreadListEvent, _, cx| {
                this.last_event = format!("{event:?}").into();
                match event {
                    ThreadListEvent::Selected { id } => {
                        let id = id.clone();
                        threads.update(cx, |list, cx| list.set_active(Some(id), cx));
                    }
                    ThreadListEvent::NewRequested => {
                        this.created += 1;
                        let id = format!("new-{}", this.created);
                        if let Some(today) = this.sections.first_mut() {
                            let mut items = today.thread_items().to_vec();
                            items.insert(
                                0,
                                ThreadItem::new(id.clone(), "New conversation")
                                    .subtitle("Just now"),
                            );
                            *today = ThreadSection::new(today.id().clone(), today.label().clone())
                                .items(items);
                        }
                        let sections = this.sections.clone();
                        threads.update(cx, |list, cx| {
                            list.set_sections(sections, cx);
                            list.set_active(Some(id), cx);
                        });
                    }
                    ThreadListEvent::ArchiveRequested { id }
                    | ThreadListEvent::UnarchiveRequested { id } => {
                        let archive = matches!(event, ThreadListEvent::ArchiveRequested { .. });
                        this.sections = this
                            .sections
                            .iter()
                            .map(|section| {
                                ThreadSection::new(section.id().clone(), section.label().clone())
                                    .items(section.thread_items().iter().map(|item| {
                                        if item.id() == id {
                                            item.clone().archived(archive)
                                        } else {
                                            item.clone()
                                        }
                                    }))
                            })
                            .collect();
                        let sections = this.sections.clone();
                        threads.update(cx, |list, cx| list.set_sections(sections, cx));
                    }
                    ThreadListEvent::DeleteRequested { id } => {
                        this.sections = this
                            .sections
                            .iter()
                            .map(|section| {
                                ThreadSection::new(section.id().clone(), section.label().clone())
                                    .items(
                                        section
                                            .thread_items()
                                            .iter()
                                            .filter(|item| item.id() != id)
                                            .cloned(),
                                    )
                            })
                            .collect();
                        let sections = this.sections.clone();
                        threads.update(cx, |list, cx| list.set_sections(sections, cx));
                    }
                    ThreadListEvent::RenameRequested { .. }
                    | ThreadListEvent::QueryChanged { .. } => {}
                }
                cx.notify();
            },
        );
        Self {
            threads,
            sections,
            created: 0,
            last_event: "Select a conversation, open its actions, or start a new chat.".into(),
            _subscription: subscription,
        }
    }
}

impl Render for ThreadListStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        v_flex()
            .gap(tokens.spacing.xs)
            .child(
                div()
                    .id("thread-list-story-host")
                    .debug_selector(|| "thread-list-story-host".into())
                    // A sidebar-width, bounded host shows grouping, the
                    // archived toggle, and scrolling within a real pane size.
                    .w(px(300.))
                    .h(px(380.))
                    .max_h(px(380.))
                    .p(tokens.spacing.sm)
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(tokens.radius.lg)
                    .overflow_hidden()
                    .bg(tokens.colors.surface)
                    .child(self.threads.clone()),
            )
            .child(
                div()
                    .id("thread-list-story-event")
                    .role(Role::Status)
                    .aria_label(format!("Last thread event: {}", self.last_event))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
            )
    }
}

/// Which catalog the command-search story is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum CommandSearchStoryState {
    #[default]
    Populated,
    Empty,
    NoResults,
}

impl StoryStates for CommandSearchStoryState {
    const ALL: &'static [Self] = &[Self::Populated, Self::Empty, Self::NoResults];

    const LABELS: &'static [(&'static str, &'static str)] =
        crate::story::COMMAND_SEARCH_STORY_VARIANTS;
}

impl CommandSearchStoryState {
    /// What the heading above the palette says this state is.
    fn heading(self) -> &'static str {
        match self {
            Self::Populated => "Populated — type margin, delivery, or risk",
            Self::Empty => "Empty catalog",
            Self::NoResults => "No results",
        }
    }

    /// The accessible name for that heading.
    fn description(self) -> &'static str {
        match self {
            Self::Populated => "Populated command search",
            Self::Empty => "Empty command catalog",
            Self::NoResults => "No command results",
        }
    }
}

struct CommandSearchStory {
    ready: Entity<CommandSearch>,
    empty: Entity<CommandSearch>,
    no_results: Entity<CommandSearch>,
    active_state: CommandSearchStoryState,
    last_event: SharedString,
    _subscription: Subscription,
}

impl CommandSearchStory {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // snippet:start(command-search)
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
        // snippet:end
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
            active_state: CommandSearchStoryState::default(),
            last_event: "Type, use arrow keys, press Enter, or choose a row.".into(),
            _subscription,
        }
    }

    /// Puts the story into one of the catalogs it demonstrates.
    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(state) = CommandSearchStoryState::ALL.get(index).copied() else {
            return;
        };
        if self.active_state != state {
            self.active_state = state;
            cx.notify();
        }
    }
}

impl Render for CommandSearchStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let state = self.active_state;
        // One palette at a time. A populated catalog, an empty one, and a
        // search that found nothing are three answers to the same question,
        // and stacking them made the story twice as tall as the component.
        let palette = match state {
            CommandSearchStoryState::Populated => self.ready.clone().into_any_element(),
            CommandSearchStoryState::Empty => self.empty.clone().into_any_element(),
            CommandSearchStoryState::NoResults => self.no_results.clone().into_any_element(),
        };
        v_flex()
            .gap(tokens.spacing.md)
            .child(story_state_switcher(
                cx.weak_entity(),
                "command-search",
                CommandSearchStoryState::LABELS,
                state.index(),
                Self::set_active_state,
            ))
            .child(
                div()
                    .id("command-search-heading")
                    .debug_selector(|| "command-search-heading".into())
                    .role(Role::Heading)
                    .aria_label(state.description())
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(state.heading()),
            )
            .child(
                div()
                    .id("command-search-host")
                    .debug_selector(|| "command-search-host".into())
                    .h(px(248.))
                    .max_h(px(248.))
                    .flex_none()
                    .child(palette),
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
    }
}

pub(crate) fn creamery_sidebar_sections() -> [SidebarSection; 3] {
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
        // snippet:start(sidebar-nav)
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
        // snippet:end

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
                    .aria_label(format!(
                        "Last sidebar navigation event: {}",
                        self.last_event
                    ))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
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
            // snippet:start(fine-tune)
            FineTuneCard::new(
                "gallery-fine-tune-populated",
                populated_values.clone(),
                gallery_typefaces(),
                window,
                cx,
            )
            // snippet:end
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
        // the edited values in real time, plus a compact constrained-height
        // variant for the scroll contract.
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
                                        .debug_selector(|| "fine-tune-preview-target".to_owned())
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
    }
}

/// Which composer the prompt-bar story is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum PromptBarStoryState {
    #[default]
    Empty,
    Ready,
    Multiline,
    Running,
    Glyph,
    Gathered,
}

impl StoryStates for PromptBarStoryState {
    const ALL: &'static [Self] = &[
        Self::Empty,
        Self::Ready,
        Self::Multiline,
        Self::Running,
        Self::Glyph,
        Self::Gathered,
    ];

    const LABELS: &'static [(&'static str, &'static str)] = crate::story::PROMPT_BAR_STORY_VARIANTS;
}

impl PromptBarStoryState {
    /// What the heading above the composer says this state is.
    fn heading(self) -> &'static str {
        match self {
            Self::Empty => "Empty without models",
            Self::Ready => "Ready with mention suggestions",
            Self::Multiline => "Multiline draft",
            Self::Running => "Running with cancellation",
            Self::Glyph => "Glyph submit, split row",
            Self::Gathered => "Controls gathered leading",
        }
    }

    /// The accessible name for that heading.
    fn description(self) -> &'static str {
        match self {
            Self::Empty => "Empty prompt without a model catalog",
            Self::Ready => "Ready prompt with mention suggestions",
            Self::Multiline => "Multiline prompt draft",
            Self::Running => "Running prompt with cancellation",
            Self::Glyph => "Compact prompt whose submit control is a glyph",
            Self::Gathered => "Prompt whose controls gather at the leading edge",
        }
    }
}

struct PromptBarStory {
    empty: Entity<PromptBar>,
    ready: Entity<PromptBar>,
    multiline: Entity<PromptBar>,
    running: Entity<PromptBar>,
    glyph: Entity<PromptBar>,
    gathered: Entity<PromptBar>,
    active_state: PromptBarStoryState,
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
        // A compact composer: the arrow is the affordance, and the row
        // still ends where the composer ends.
        let glyph = cx.new(|cx| {
            let mut prompt = Self::configured_prompt(
                "gallery-prompt-glyph",
                "Draft the supplier note",
                ProgressState::Pending,
                false,
                window,
                cx,
            );
            prompt.set_submit(PromptSubmit::Glyph, cx);
            prompt
        });
        // Everything gathered at the leading edge, for a composer docked
        // beside other chrome that owns the row's trailing end.
        let gathered = cx.new(|cx| {
            let mut prompt = Self::configured_prompt(
                "gallery-prompt-gathered",
                "Check the cold-chain window",
                ProgressState::Pending,
                false,
                window,
                cx,
            );
            prompt.set_actions(PromptActions::Leading, cx);
            prompt
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

        let glyph_subscription = cx.subscribe_in(
            &glyph,
            window,
            |this, prompt, event: &PromptBarEvent, _, cx| {
                this.on_event(prompt, event, cx);
            },
        );
        let gathered_subscription = cx.subscribe_in(
            &gathered,
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
            glyph,
            gathered,
            active_state: PromptBarStoryState::default(),
            last_event: "Interact with a composer to inspect its typed event.".into(),
            _subscriptions: vec![
                empty_subscription,
                ready_subscription,
                multiline_subscription,
                running_subscription,
                glyph_subscription,
                gathered_subscription,
            ],
        }
    }

    /// Puts the story into one of the composers it demonstrates.
    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(state) = PromptBarStoryState::ALL.get(index).copied() else {
            return;
        };
        if self.active_state != state {
            self.active_state = state;
            cx.notify();
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
                PromptModel::new("balanced", "Balanced")
                    .provider("Anthropic")
                    .description("Claude Sonnet 5 · everyday work")
                    .context_window(200_000),
                PromptModel::new("deep", "Deep")
                    .provider("Anthropic")
                    .description("Claude Opus 5 · long reasoning")
                    .context_window(200_000),
                PromptModel::new("fast", "Fast")
                    .provider("Anthropic")
                    .description("Claude Haiku 4.5 · quick replies")
                    .context_window(200_000),
                PromptModel::new("local", "Local")
                    .provider("On device")
                    .description("Private, no network")
                    .context_window(32_000),
                PromptModel::new("offline", "Offline")
                    .provider("On device")
                    .description("Unavailable until the model downloads")
                    .disabled(true),
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
        let state = self.active_state;
        // One composer at a time, chosen from the toolbar. Six of them
        // stacked was six times the height for a component that is read one
        // at a time anyway, and the states below the fold were the ones
        // nobody saw.
        let composer = match state {
            PromptBarStoryState::Empty => self.empty.clone().into_any_element(),
            PromptBarStoryState::Ready => self.ready.clone().into_any_element(),
            PromptBarStoryState::Multiline => self.multiline.clone().into_any_element(),
            PromptBarStoryState::Running => self.running.clone().into_any_element(),
            PromptBarStoryState::Glyph => self.glyph.clone().into_any_element(),
            PromptBarStoryState::Gathered => self.gathered.clone().into_any_element(),
        };
        v_flex()
            .gap(tokens.spacing.md)
            .child(story_state_switcher(
                cx.weak_entity(),
                "prompt-bar",
                PromptBarStoryState::LABELS,
                state.index(),
                Self::set_active_state,
            ))
            .child(
                div()
                    .id("prompt-bar-heading")
                    .debug_selector(|| "prompt-bar-heading".into())
                    .role(Role::Heading)
                    .aria_label(state.description())
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(state.heading()),
            )
            .child(
                div()
                    .debug_selector(|| "prompt-bar-composer".into())
                    .child(composer),
            )
            .child(
                TextView::markdown(
                    "prompt-bar-event-log",
                    format!("**Last typed event.** {}", self.last_event),
                )
                .selectable(true),
            )
    }
}

/// Which control family the form story is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum FormStoryState {
    #[default]
    Choices,
    Toggles,
}

impl StoryStates for FormStoryState {
    const ALL: &'static [Self] = &[Self::Choices, Self::Toggles];
    const LABELS: &'static [(&'static str, &'static str)] = crate::story::FORM_STORY_VARIANTS;
}

struct FormStory {
    active_state: FormStoryState,
    flavour: Option<SharedString>,
    stream: bool,
    cite: bool,
    last_event: SharedString,
}

impl FormStory {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            active_state: FormStoryState::default(),
            flavour: Some("five".into()),
            stream: true,
            cite: false,
            last_event: "Choose an option or throw a switch.".into(),
        }
    }

    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(state) = FormStoryState::ALL.get(index).copied() else {
            return;
        };
        if self.active_state != state {
            self.active_state = state;
            cx.notify();
        }
    }
}

impl Render for FormStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let state = self.active_state;
        let body: AnyElement = match state {
            FormStoryState::Choices => {
                // snippet:start(form-controls)
                ChoiceGroup::new("flavours", "How many flavours ship first?")
                    .options([
                        ChoiceOption::new("three", "Three")
                            .description("The core line, and the fastest to stock"),
                        ChoiceOption::new("five", "Five").description("The full case"),
                        ChoiceOption::new("one", "Just one hero"),
                        ChoiceOption::new("none", "Undecided").disabled(true),
                    ])
                    // snippet:end
                    .selection(self.flavour.clone())
                    .on_event(cx.listener(|story, event, _, cx| {
                        let ChoiceEvent::Chosen { option, .. } = event;
                        story.flavour = Some(option.clone());
                        story.last_event = format!("Chose {option}").into();
                        cx.notify();
                    }))
                    .into_any_element()
            }
            FormStoryState::Toggles => v_flex()
                .gap(tokens.spacing.xs)
                .child(
                    Toggle::new("stream", "Stream the answer")
                        .description("Show tokens as they arrive rather than all at once")
                        .shape(ToggleShape::Switch)
                        .on(self.stream)
                        .on_event(cx.listener(|story, event, _, cx| {
                            let ToggleEvent::Toggled { on, .. } = event;
                            story.stream = *on;
                            story.last_event = format!("Streaming {on}").into();
                            cx.notify();
                        })),
                )
                .child(
                    Toggle::new("cite", "Include citations")
                        .shape(ToggleShape::Check)
                        .on(self.cite)
                        .on_event(cx.listener(|story, event, _, cx| {
                            let ToggleEvent::Toggled { on, .. } = event;
                            story.cite = *on;
                            story.last_event = format!("Citations {on}").into();
                            cx.notify();
                        })),
                )
                .child(
                    Toggle::new("locked", "Use the shared workspace")
                        .description("Set by your administrator")
                        .shape(ToggleShape::Check)
                        .on(true)
                        .disabled(true),
                )
                .into_any_element(),
        };

        v_flex()
            .gap(tokens.spacing.md)
            .child(story_state_switcher(
                cx.weak_entity(),
                "form",
                FormStoryState::LABELS,
                state.index(),
                Self::set_active_state,
            ))
            .child(body)
            .child(
                div()
                    .id("form-event")
                    .role(Role::Status)
                    .aria_label(format!("Last form event: {}", self.last_event))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
            )
    }
}

struct QuestionFlowStory {
    step: usize,
    answers: HashMap<SharedString, SharedString>,
    last_event: SharedString,
}

impl QuestionFlowStory {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            step: 0,
            answers: HashMap::new(),
            last_event: "Answer a question to move on.".into(),
        }
    }

    fn questions(&self) -> Vec<Question> {
        [
            (
                "flavours",
                "How many flavours should we launch?",
                vec![
                    ChoiceOption::new("three", "Three").description("The core line"),
                    ChoiceOption::new("five", "Five").description("The full case"),
                    ChoiceOption::new("one", "Just one hero"),
                ],
            ),
            (
                "mixins",
                "Which mix-ins should we stock?",
                vec![
                    ChoiceOption::new("chips", "Chocolate chips"),
                    ChoiceOption::new("waffle", "Waffle bits"),
                    ChoiceOption::new("sprinkles", "Sprinkles"),
                ],
            ),
            (
                "market",
                "Which market do we enter first?",
                vec![
                    ChoiceOption::new("trucks", "Food trucks"),
                    ChoiceOption::new("freezers", "Grocery freezers"),
                    ChoiceOption::new("shops", "Scoop shops"),
                ],
            ),
        ]
        .into_iter()
        .map(|(id, prompt, options)| {
            Question::new(id, prompt)
                .options(options)
                .answered(self.answers.get(id).cloned())
        })
        .collect()
    }
}

impl Render for QuestionFlowStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        v_flex()
            .gap(tokens.spacing.md)
            .child(
                // snippet:start(question-flow)
                QuestionFlow::new("launch", "Before I draft the launch plan")
                    .questions(self.questions())
                    // snippet:end
                    .step(self.step)
                    .on_event(cx.listener(|story, event, _, cx| {
                        match event {
                            QuestionFlowEvent::Answered {
                                question, option, ..
                            } => {
                                story.answers.insert(question.clone(), option.clone());
                                story.last_event = format!("Answered {question}: {option}").into();
                            }
                            QuestionFlowEvent::Skipped { question, .. } => {
                                story.step += 1;
                                story.last_event = format!("Skipped {question}").into();
                            }
                            QuestionFlowEvent::Advanced { step, .. } => {
                                story.step = *step;
                                story.last_event = format!("Moved to question {}", step + 1).into();
                            }
                            QuestionFlowEvent::Completed { .. } => {
                                story.last_event = "Finished — the agent has what it needs".into();
                            }
                        }
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("question-flow-event")
                    .role(Role::Status)
                    .aria_label(format!("Last question-flow event: {}", self.last_event))
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("Last event: {}", self.last_event)),
            )
    }
}

/// Decorations painted into a real component, switchable.
///
/// The component is an approval card because it is one an application would
/// actually decorate: it already carries a semantic border, so a decoration
/// that fought the frame would be obvious immediately.
struct DecorationsStory {
    kind: crate::decorations::DecorationKind,
    /// How far the ripple has travelled, 0 at rest.
    pressed: bool,
}

impl DecorationsStory {
    fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self {
            kind: crate::decorations::DecorationKind::default(),
            pressed: false,
        }
    }

    fn set_active_state(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(kind) = crate::decorations::DecorationKind::ALL.get(index).copied() else {
            return;
        };
        if self.kind != kind {
            self.kind = kind;
            self.pressed = false;
            cx.notify();
        }
    }
}

impl Render for DecorationsStory {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let kind = self.kind;
        // The ripple is the one decoration driven by the application rather
        // than by a clock: the library eases the value, the story owns when
        // it moves.
        let ripple = gpui_ai::prelude::decoration::toward(
            "decorations-ripple",
            if self.pressed { 1.0 } else { 0.0 },
            window,
            cx,
        );

        v_flex()
            .gap(tokens.spacing.md)
            .child(story_state_switcher(
                cx.weak_entity(),
                "decorations",
                crate::decorations::DecorationKind::LABELS,
                kind.index(),
                Self::set_active_state,
            ))
            .child({
                let card = ApprovalCard::new("decorated-gate", "Publish the launch plan?")
                    .description(
                        "The card is unchanged. Everything behind and in front of it \
                         is the application's.",
                    )
                    .decoration(kind.build(ripple, cx))
                    .on_event(cx.listener(|story, _: &ApprovalEvent, _, cx| {
                        story.pressed = !story.pressed;
                        cx.notify();
                    }));
                if crate::decorations::needs_backdrop(kind) {
                    // The stage: what an application paints around its own
                    // component. Three states need it, for two reasons. The
                    // frosted panel needs something behind it to be out of
                    // focus, placed from the same numbers so the blurred copy
                    // lines up. The aurora needs somewhere outside the card to
                    // glow, because the slot clips to the card and light that
                    // leaves its edge can only be drawn from out here.
                    //
                    // Fixed sizes, unusually for this gallery, and that is the
                    // honest shape of it: lining a blurred copy up with its
                    // original means both are placed by the same arithmetic.
                    let aurora = kind == crate::decorations::DecorationKind::Aurora;
                    div()
                        .relative()
                        // The aurora's lights are wider than the stage at the
                        // corners, and drawing a blurred shadow off the edge
                        // of the world leaves shards behind it. The stage is
                        // the page as far as this effect is concerned.
                        .overflow_hidden()
                        .w(px(crate::decorations::BACKDROP.width))
                        .h(px(crate::decorations::BACKDROP.height))
                        .flex()
                        .items_center()
                        .justify_center()
                        // The aurora gets a dark stage rather than the
                        // photograph. Coloured light is read against what is
                        // behind it, and a bright nebula leaves nothing for it
                        // to be brighter than.
                        .when(aurora, |stage| stage.bg(cx.theme().background))
                        .when(!aurora, |stage| {
                            stage.child(crate::decorations::backdrop(
                                crate::decorations::stage_for(kind),
                            ))
                        })
                        .when(aurora, |stage| {
                            stage.child(crate::decorations::aurora_around())
                        })
                        .child(
                            // The card is pinned to the size the decoration
                            // arithmetic assumes. Everywhere else in this
                            // gallery a component sizes itself; here the
                            // blurred copy has to land on the same pixels as
                            // the sharp one, and a content-driven height would
                            // move it.
                            card.w(px(crate::decorations::CARD.width))
                                .h(px(crate::decorations::CARD.height))
                                // The tint is translucency, so the component's
                                // own background has to get out of the way —
                                // the decoration paints over it, not under it.
                                // This is the ordinary style override, doing
                                // exactly what it says.
                                .when(kind == crate::decorations::DecorationKind::Tint, |card| {
                                    card.bg(gpui::transparent_black())
                                }),
                        )
                        .into_any_element()
                } else {
                    card.into_any_element()
                }
            })
            .child(
                div()
                    .id("decorations-note")
                    .role(Role::Status)
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(kind.note()),
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
            // snippet:start(selection-actions)
            SelectionActions::new(
                "gallery-selection-actions",
                "## Weekly flavor review\n\nMint Chip demand softened while Vanilla recovered. Select any phrase in this readable analysis, then choose an action. Native **Ctrl/Cmd+A** and copy remain available.",
                window,
                cx,
            )
            // snippet:end
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
    }
}

/// A theme preset available to native and web gallery hosts.
///
/// The set is whatever `themes/` holds — gpui-component's default Light and
/// Dark, gpui-ai's own JSON themes, and the vendored upstream pack — so adding
/// a file adds a preset without touching this type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GalleryTheme(usize);

impl GalleryTheme {
    /// gpui-component's default light theme.
    pub const LIGHT: Self = Self(0);
    /// gpui-component's default dark theme.
    pub const DARK: Self = Self(1);

    /// Every preset, in table order: Light, Dark, gpui-ai's themes, then the
    /// credited upstream pack.
    pub fn all() -> impl ExactSizeIterator<Item = Self> {
        (0..BUNDLED_PRESETS.len()).map(Self)
    }

    /// Resolves the slug used in `?theme=` URLs and the theme picker.
    pub fn from_slug(slug: &str) -> Option<Self> {
        BUNDLED_PRESETS
            .iter()
            .position(|entry| entry.slug == slug)
            .map(Self)
    }

    fn entry(self) -> &'static GalleryThemeEntry {
        &BUNDLED_PRESETS[self.0]
    }

    /// The stable slug for this preset.
    pub fn slug(self) -> &'static str {
        self.entry().slug
    }

    /// The label shown on the theme control.
    pub fn label(self) -> &'static str {
        self.entry().label
    }

    /// Which group the preset belongs to: gpui-ai's own set, or the vendored
    /// gpui-component pack that the site credits separately.
    pub fn group(self) -> &'static str {
        self.entry().group
    }

    /// Whether the preset asks for dark mode.
    pub fn is_dark(self) -> bool {
        self.entry().dark
    }

    /// The name this preset registers under, or `None` for gpui-component's
    /// own defaults, which come from the registry's default theme per mode.
    fn registry_name(self) -> Option<&'static str> {
        self.entry().registry_name
    }

    /// The next preset in the review cycle.
    ///
    /// The cycle covers gpui-ai's own presets only. The upstream pack stays
    /// reachable by slug and in the site's grouped picker, because stepping a
    /// button through forty-odd themes is not a review workflow.
    fn next(self) -> Self {
        let count = BUNDLED_PRESETS.len();
        let mut index = self.0;
        for _ in 0..count {
            index = (index + 1) % count;
            if BUNDLED_PRESETS[index].group == GPUI_AI_THEME_GROUP {
                return Self(index);
            }
        }
        Self::LIGHT
    }
}

/// Applies a complete gallery preset, including its registered token configuration.
pub fn apply_gallery_theme(preset: GalleryTheme, window: Option<&mut Window>, cx: &mut App) {
    let registry = ThemeRegistry::global(cx);
    let config = match preset.registry_name() {
        None if preset.is_dark() => registry.default_dark_theme().clone(),
        None => registry.default_light_theme().clone(),
        Some(name) => match registry.themes().get(name) {
            Some(config) => config.clone(),
            None => return,
        },
    };

    let mode = if preset.is_dark() {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };

    let theme = Theme::global_mut(cx);
    if mode.is_dark() {
        theme.dark_theme = config.clone();
    } else {
        theme.light_theme = config.clone();
    }
    Theme::change(mode, None, cx);
    restore_unset_metrics(&config, cx);
    if let Some(window) = window {
        window.refresh();
    }
}

/// Puts back the metrics the incoming theme does not mention.
///
/// A theme JSON may leave out `font.size`, `radius`, `shadow` and the rest, and
/// gpui-component only overwrites a field the config declares. What it leaves
/// behind is not the built-in default, though — it is whatever the *previous*
/// theme set. So choosing Graphite, which asks for 14px type and square
/// corners, and then choosing Nord Frost, which says nothing about either, left
/// the whole gallery at 14px with square corners for the rest of the session,
/// and no theme after it could ever look like itself again.
///
/// The website makes this easy to hit: forty-five themes in a picker, three of
/// which set metrics the other forty-two do not.
///
/// Fixed here rather than upstream because the behaviour is defensible in a
/// host that installs one theme and keeps it; it is only wrong when themes are
/// swapped, which is what this gallery exists to do.
fn restore_unset_metrics(config: &ThemeConfig, cx: &mut App) {
    let defaults = Theme::default();
    let theme = Theme::global_mut(cx);
    if config.font_size.is_none() {
        theme.font_size = defaults.font_size;
    }
    if config.mono_font_size.is_none() {
        theme.mono_font_size = defaults.mono_font_size;
    }
    if config.font_family.is_none() {
        theme.font_family = defaults.font_family.clone();
    }
    if config.mono_font_family.is_none() {
        theme.mono_font_family = defaults.mono_font_family.clone();
    }
    if config.radius.is_none() {
        theme.radius = defaults.radius;
    }
    if config.radius_lg.is_none() {
        theme.radius_lg = defaults.radius_lg;
    }
    if config.shadow.is_none() {
        theme.shadow = defaults.shadow;
    }
    // The Base layer keeps its own copy of the radius, so a scrollbar would go
    // on painting with the corners the last theme gave it.
    Theme::sync_base(cx);
}

// snippet:start(records-table)
fn records_story_columns() -> Vec<RecordColumn> {
    vec![
        // Wide enough for the longest supplier in the fixture: a cell's
        // selectable prose cannot ellipsize, so a column narrower than its
        // content clips mid-word rather than trailing off.
        RecordColumn::new("supplier", "Supplier")
            .sortable(true)
            .fixed(true)
            .width(px(280.)),
        RecordColumn::new("region", "Region")
            .sortable(true)
            .width(px(120.)),
        RecordColumn::new("products", "Products").width(px(190.)),
        RecordColumn::new("unit_cost", "Unit cost")
            .sortable(true)
            .alignment(RecordColumnAlignment::Right)
            .width(px(110.)),
        RecordColumn::new("status", "Status")
            .sortable(true)
            .width(px(140.)),
    ]
}
// snippet:end

fn records_story_rows() -> Vec<RecordRow> {
    vec![
        RecordRow::new("alpenrose", "Alpenrose Dairy").cells([
            RecordCell::new("supplier", "Alpenrose Dairy"),
            RecordCell::new("region", "Northwest"),
            RecordCell::tags("products", ["Milk", "Cream"]),
            RecordCell::new("unit_cost", "$4.12"),
            RecordCell::status("status", "Ready", RecordStatusTone::Positive),
        ]),
        RecordRow::new("tillamook", "Tillamook Creamery").cells([
            RecordCell::new("supplier", "Tillamook Creamery"),
            RecordCell::new("region", "Pacific"),
            RecordCell::tags("products", ["Cheese", "Ice cream"]),
            RecordCell::new("unit_cost", "$4.43"),
            RecordCell::status("status", "Review", RecordStatusTone::Caution),
        ]),
        RecordRow::new("cascade", "Cascade Cultured Foods")
            .cells([
                RecordCell::new("supplier", "Cascade Cultured Foods"),
                RecordCell::new("region", "Mountain"),
                RecordCell::tags("products", ["Yogurt", "Kefir"]),
                RecordCell::new("unit_cost", "$4.37"),
                RecordCell::status("status", "Paused", RecordStatusTone::Neutral),
            ])
            .disabled(true),
        RecordRow::new("redwood", "Redwood Organic Dairy").cells([
            RecordCell::new("supplier", "Redwood Organic Dairy"),
            RecordCell::new("region", "West"),
            RecordCell::tags("products", ["Butter", "Cream"]),
            RecordCell::new("unit_cost", "$5.08"),
            RecordCell::status("status", "Blocked", RecordStatusTone::Critical),
        ]),
    ]
}

/// The cents in a "$4.12"-shaped value, when the value is one.
fn records_story_cost_key(value: &str) -> Option<u32> {
    let value = value.strip_prefix('$')?;
    let (dollars, cents) = value.split_once('.')?;
    Some(dollars.parse::<u32>().ok()? * 100 + cents.parse::<u32>().ok()?)
}

fn records_story_many_rows() -> Arc<[RecordRow]> {
    (0..100)
        .map(|index| {
            RecordRow::new(format!("supplier-{index}"), format!("Supplier {index}")).cells([
                RecordCell::new("supplier", format!("Supplier {index}")),
                RecordCell::new("region", format!("Region {}", index % 8)),
                RecordCell::tags("products", ["Milk", "Cream"]),
                RecordCell::new(
                    "unit_cost",
                    format!("${}.{:02}", 3 + index % 4, (index * 17) % 100),
                ),
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
    // snippet:start(records-table)
    let mut table = RecordsTable::new(id, label, window, cx);
    table.set_columns(records_story_columns(), window, cx);
    table.set_records(records, window, cx);
    table
    // snippet:end
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
                                    let left = left.cell(column_id).map(RecordCell::value);
                                    let right = right.cell(column_id).map(RecordCell::value);
                                    let ordering = match (
                                        left.and_then(records_story_cost_key),
                                        right.and_then(records_story_cost_key),
                                    ) {
                                        // Money compares as money, not as text:
                                        // "$12.80" belongs after "$4.12".
                                        (Some(left), Some(right)) => left.cmp(&right),
                                        _ => left.cmp(&right),
                                    };
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

        v_flex().gap_3().child(switcher).child(active).child(
            div()
                .id("records-story-end")
                .debug_selector(|| "records-story-end".into())
                .child(
                    TextView::markdown(
                        "records-story-event-log",
                        format!("**Last typed event.** {}", self.last_event),
                    )
                    .selectable(true),
                ),
        )
    }
}

// snippet:start(diff-table)
fn diff_story_columns() -> Vec<DiffColumn> {
    vec![
        DiffColumn::new("flavor", "Flavor")
            .width(px(220.))
            .fixed(true)
            .sortable(true),
        DiffColumn::new("category", "Category")
            .width(px(380.))
            .sortable(true),
    ]
}
// snippet:end

fn diff_story_rows() -> Vec<DiffRow> {
    vec![
        DiffRow::new("rocky-road", "Rocky Road", DiffChangeKind::Changed).cells([
            DiffCell::unchanged("flavor", "Rocky Road"),
            DiffCell::changed("category", "Classic", "Seasonal"),
        ]),
        DiffRow::new("bubblegum", "Bubblegum", DiffChangeKind::Removed).cells([
            DiffCell::removed("flavor", "Bubblegum"),
            DiffCell::removed("category", "Retro"),
        ]),
        DiffRow::new("mint-chip", "Mint Chip", DiffChangeKind::Changed).cells([
            DiffCell::unchanged("flavor", "Mint Chip"),
            DiffCell::changed("category", "Classic", "Limited"),
        ]),
        DiffRow::new("pistachio", "Pistachio", DiffChangeKind::Added).cells([
            DiffCell::added("flavor", "Pistachio"),
            DiffCell::added("category", "Seasonal"),
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
                DiffCell::changed(
                    "category",
                    format!("Category {}", index % 8),
                    format!("Category {}", (index + 1) % 8),
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
    // snippet:start(diff-table)
    let mut table = DiffTable::new(id, label, window, cx);
    table.set_columns(diff_story_columns(), window, cx);
    table.set_rows(rows, window, cx);
    table
    // snippet:end
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

        v_flex().gap_3().child(switcher).child(active).child(
            div()
                .id("diff-story-end")
                .debug_selector(|| "diff-story-end".into())
                .child(
                    TextView::markdown(
                        "diff-story-event-log",
                        format!("**Last typed event.** {}", self.last_event),
                    )
                    .selectable(true),
                ),
        )
    }
}

// snippet:start(filter-table)
fn filter_story_columns() -> [FilterColumn; 4] {
    [
        FilterColumn::new("task", "Task name")
            .width(px(280.))
            .fixed(true),
        FilterColumn::new("date", "Date")
            .width(px(150.))
            .sortable(true),
        FilterColumn::new("status", "Status")
            .width(px(150.))
            .sortable(true),
        FilterColumn::new("advisor", "Advisor").width(px(240.)),
    ]
}
// snippet:end

fn filter_story_rows() -> Vec<FilterRow> {
    // Chronological by default, with the year on every date so the span
    // across the new year reads as time, not as an alphabet accident.
    [
        (
            "sesame",
            "Churn black sesame",
            "Sep 22, 2025",
            "In Progress",
            "Kumo Creamery",
        ),
        (
            "batch",
            "Taste-test batch 42",
            "Nov 08, 2025",
            "In Progress",
            "Maple Orbit",
        ),
        (
            "mango",
            "Restock mango sorbet",
            "Dec 03, 2025",
            "To do",
            "Mango Moon Gelato",
        ),
        (
            "menu",
            "Print summer menu",
            "Jan 02, 2026",
            "To do",
            "Coral Coast Sorbet",
        ),
        (
            "cones",
            "Order waffle cones",
            "Apr 14, 2026",
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

/// The (year, month, day) in a "Sep 22, 2025"-shaped value, when it is one.
fn filter_story_date_key(value: &str) -> Option<(u16, u8, u8)> {
    let mut parts = value.split([' ', ',']).filter(|part| !part.is_empty());
    let month = match parts.next()? {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    };
    let day = parts.next()?.parse().ok()?;
    let year = parts.next()?.parse().ok()?;
    Some((year, month, day))
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
            // Dates compare as dates — "Jan 02, 2026" after "Dec 03, 2025"
            // — and everything else stays lexicographic.
            let ordering = match (filter_story_date_key(left), filter_story_date_key(right)) {
                (Some(left), Some(right)) => left.cmp(&right),
                _ => left.cmp(right),
            };
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
                FilterCell::new(
                    "date",
                    format!(
                        "{} {:02}, 2025",
                        [
                            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
                            "Nov", "Dec"
                        ][index % 12],
                        index % 28 + 1
                    ),
                ),
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
    // snippet:start(filter-table)
    let mut table = FilterTable::new(id, label, window, cx);
    table.set_columns(filter_story_columns(), window, cx);
    table.set_filters(filters, cx);
    table.set_rows(rows, cx);
    table
    // snippet:end
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
                div()
                    .id("filter-story-end")
                    .debug_selector(|| "filter-story-end".into())
                    .child(
                        TextView::markdown(
                            "filter-story-event-log",
                            format!("**Last typed event.** {}", self.last_event),
                        )
                        .selectable(true),
                    ),
            )
            .into_any_element()
    }
}

// snippet:start(comparison-table)
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
// snippet:end

fn configured_comparison_table(
    id: &str,
    label: &str,
    snapshot: Progressive<ComparisonSnapshot>,
    selected: Option<&str>,
    window: &mut Window,
    cx: &mut Context<ComparisonTable>,
) -> ComparisonTable {
    // snippet:start(comparison-table)
    let mut table = ComparisonTable::new(id, label, window, cx);
    table.set_snapshot(snapshot, window, cx);
    if let Some(selected) = selected {
        table.set_selected_item(selected, window, cx);
    }
    table
    // snippet:end
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

        v_flex().gap_3().child(switcher).child(active).child(
            div()
                .id("comparison-story-end")
                .debug_selector(|| "comparison-story-end".into())
                .child(
                    TextView::markdown(
                        "comparison-story-event-log",
                        format!("**Last typed event.** {}", self.last_event),
                    )
                    .selectable(true),
                ),
        )
    }
}

/// How much of the gallery's own furniture to draw around a story.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GalleryChrome {
    /// The native gallery: a title and the theme control above the story.
    #[default]
    Full,
    /// The website's embed. The page already provides a heading, a theme
    /// picker and a frame, so drawing them again is duplication inside a
    /// duplicate — and the story is centred in whatever room the frame gives.
    Embedded,
}

/// The window-resolved inputs the catalog's measured row heights depend on.
///
/// The library keeps the same key for its own components, private to that
/// crate: a value key is four lines, and duplicating it costs less than a
/// public item that exists only because a consumer needed one. What must not
/// be shared is the policy — each surface re-anchors the way its own content
/// reads.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct ResolvedLayoutKey {
    rem_size: Option<Pixels>,
}

impl ResolvedLayoutKey {
    /// Whether `rem_size` is already the recorded value; mutates nothing, so a
    /// render may ask.
    fn matches(&self, rem_size: Pixels) -> bool {
        self.rem_size == Some(rem_size)
    }

    /// Records `rem_size` and reports whether it replaced a *different* one.
    /// The first observation invalidates nothing: no rows were measured under
    /// an earlier value.
    fn observe(&mut self, rem_size: Pixels) -> bool {
        self.rem_size
            .replace(rem_size)
            .is_some_and(|previous| previous != rem_size)
    }
}

/// Stateful component gallery shared by native and web launchers.
pub struct Gallery {
    selected: StoryId,
    chrome: GalleryChrome,
    /// What the catalog's keybindings are dispatched through.
    ///
    /// GPUI routes a keystroke down the focus path, so a window with nothing
    /// focused hears nothing: the page and jump bindings were unreachable
    /// without this, whatever context they named.
    focus: FocusHandle,
    /// Whether the window has been given its opening focus.
    ///
    /// Once only, and never in the embed: a reader who has clicked into a
    /// composer must keep the caret they put there, and the web host has its
    /// own reasons not to be focused from inside a demo.
    focused_on_open: bool,
    /// Frame timings, collected only while the meter is open.
    metrics: Option<crate::metrics::FrameMeter>,
    /// Bumped by [`Gallery::reset_story`], and part of the key of every story
    /// entity the window holds.
    ///
    /// Those entities are not fields here — they live in the window's keyed
    /// state, which outlives this struct — so replacing this struct left a
    /// reset chat still holding its transcript and a reset table still
    /// sorted. Changing the key is what actually asks for a new one.
    generation: usize,
    catalog_list: ListState,
    /// The tallest each catalog row has been, so a story that animates does
    /// not shove the ones below it.
    ///
    /// Deliberately not the catalog's declared heights: those are measured
    /// for the website, which reserves space before a demo has booted and
    /// sizes its no-WebGPU posters from them. A story here is drawn at
    /// whatever size it actually is — what this stops is the *jolt* of a
    /// story collapsing and re-expanding as its animation restarts, which
    /// moves every row beneath it. Growth still moves things, because
    /// growth is the component doing its job.
    ///
    /// A side channel rather than a field read during render: the probe
    /// writes it while laying out, and writing to state GPUI is observing
    /// mid-layout is how a render ends up notifying itself.
    story_floors: Rc<RefCell<CatalogFloors>>,
    resolved_layout: ResolvedLayoutKey,
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
    removed_attachments: HashSet<SharedString>,
    last_attachment_event: Option<SharedString>,
    demo_thumbnail: Arc<Image>,
    diff_open: bool,
    hunk_reviews: HashMap<usize, HunkReview>,
    last_diff_event: Option<SharedString>,
    approval_decisions: HashMap<SharedString, ApprovalDecision>,
    last_approval_event: Option<SharedString>,
    plan_state: PlanState,
    last_plan_event: Option<SharedString>,
    artifact_view: ArtifactView,
    artifact_version: usize,
    artifact_open: bool,
    last_artifact_event: Option<SharedString>,
    artifact_split: Entity<ResizableState>,
    voice_state: VoiceState,
    voice_transcript: Option<SharedString>,
    last_voice_event: Option<SharedString>,
    queued: Vec<QueuedMessage>,
    last_queue_event: Option<SharedString>,
    theme: GalleryTheme,
    /// Active middle-click autoscroll session (catalog view only).
    autoscroll: Option<Autoscroll>,
    /// Frame driver for the active autoscroll session.
    ///
    /// Owned rather than detached so that exactly one driver can run: starting
    /// replaces it, cancelling drops it, and dropping the gallery ends it.
    autoscroll_task: Option<Task<()>>,
    /// Wheel acceleration state for the catalog feed.
    wheel: WheelAccelerator,
    #[cfg(any(test, feature = "performance"))]
    scan_simulation_suspended: bool,
}

impl Gallery {
    /// Whether the approval story has granted the gate with this stable ID.
    ///
    /// Read-only browser inspection: input tests must observe the real decision,
    /// not infer activation from an incidental change in the story's height.
    pub fn is_approval_granted(&self, id: &str) -> bool {
        self.approval_decisions.get(id) == Some(&ApprovalDecision::Approved)
    }

    /// Creates the gallery for one selected story or the complete catalog.
    pub fn new(selected: StoryId, cx: &mut Context<Self>) -> Self {
        Self::new_with_theme(selected, None, cx)
    }

    fn new_with_theme(
        selected: StoryId,
        theme: Option<GalleryTheme>,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
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
            focus,
            focused_on_open: false,
            metrics: None,
            catalog_list,
            story_floors: Rc::new(RefCell::new(CatalogFloors::default())),
            resolved_layout: ResolvedLayoutKey::default(),
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
            removed_attachments: HashSet::new(),
            last_attachment_event: None,
            demo_thumbnail: Arc::new(Image::from_bytes(
                ImageFormat::Png,
                include_bytes!("../assets/meadow-thumbnail.png").to_vec(),
            )),
            diff_open: true,
            hunk_reviews: HashMap::new(),
            last_diff_event: None,
            approval_decisions: HashMap::new(),
            last_approval_event: None,
            plan_state: PlanState::Proposed,
            last_plan_event: None,
            artifact_view: ArtifactView::Preview,
            artifact_version: 1,
            artifact_open: true,
            last_artifact_event: None,
            artifact_split: cx.new(|_| ResizableState::default()),
            voice_state: VoiceState::Idle,
            voice_transcript: None,
            last_voice_event: None,
            queued: demo_queue(),
            last_queue_event: None,
            theme: theme.unwrap_or_else(|| {
                if cx.theme().is_dark() {
                    GalleryTheme::DARK
                } else {
                    GalleryTheme::LIGHT
                }
            }),
            chrome: GalleryChrome::Full,
            generation: 0,
            autoscroll: None,
            autoscroll_task: None,
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

    /// Re-measures the catalog after the window's rem size changed.
    ///
    /// Story rows cache text laid out at the previous rem, and the simulation
    /// only invalidates rows whose *content* moved, so nothing else notices a
    /// zoom. The catalog re-anchors on the story that was first on screen and
    /// its offset within that row.
    fn resolve_layout(&mut self, rem_size: Pixels, cx: &mut Context<Self>) {
        if !self.resolved_layout.observe(rem_size) {
            return;
        }
        // Not gated on the catalog being the visible surface: the list is
        // retained across story selection, so heights measured while a single
        // story was open would still be stale on the way back.
        let offset = self.catalog_list.logical_scroll_top();
        let anchor = StoryId::ALL
            .get(offset.item_ix)
            .map(|story| (*story, offset.offset_in_item));

        // The floors were observed at the old rem and mean nothing at the
        // new one — a row would otherwise keep a height it earned while the
        // type was larger.
        self.story_floors.borrow_mut().rows.clear();
        self.catalog_list.remeasure();
        if let Some((story, offset_in_item)) = anchor
            && let Some(item_ix) = StoryId::ALL
                .iter()
                .position(|candidate| *candidate == story)
        {
            self.catalog_list.scroll_to(ListOffset {
                item_ix,
                offset_in_item,
            });
        }
        cx.notify();
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
    ///
    /// A page is a fraction of the viewport the list last measured, not of a
    /// fixed row: story rows differ in height and the same row is taller at a
    /// larger type scale, so a constant would page a different amount of what
    /// the reader can actually see at every window size. Before the first
    /// layout there is no measured viewport and paging does nothing.
    pub fn scroll_catalog_page(&mut self, direction: f32, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        let viewport = self.catalog_list.viewport_bounds().size.height;
        self.catalog_list
            .scroll_by(viewport * direction * PAGE_FRACTION);
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
    /// The frame driver is stored, so starting again replaces the running one
    /// rather than adding a second: dropping the old task cancels it, and the
    /// gallery's own drop ends the last one. Motion is not decorative here —
    /// it is the gesture the reader is holding — so reduced motion narrows no
    /// distance; it is the animated stories that answer to it.
    pub fn start_autoscroll(&mut self, anchor: Point<Pixels>, cx: &mut Context<Self>) {
        if self.selected != StoryId::All {
            return;
        }
        self.autoscroll = Some(Autoscroll::start(anchor));
        self.autoscroll_task = Some(cx.spawn(async move |this, cx| {
            // Elapsed time, not the requested interval: a tick delayed behind
            // a long frame must still cover the distance it stands for. The
            // executor's clock is the wasm-safe one and the one tests drive.
            let mut previous = cx.background_executor().now();
            loop {
                cx.background_executor()
                    .timer(AUTOSCROLL_FRAME_INTERVAL)
                    .await;
                let now = cx.background_executor().now();
                let elapsed = now.saturating_duration_since(previous);
                previous = now;
                let alive = this.update(cx, |gallery, cx| {
                    if gallery.autoscroll.is_none() {
                        return false;
                    }
                    gallery.tick_autoscroll(elapsed.as_secs_f32(), cx);
                    gallery.autoscroll.is_some()
                });
                if !alive.unwrap_or(false) {
                    break;
                }
            }
        }));
        cx.notify();
    }

    /// Records the current pointer position for an active autoscroll session.
    pub fn track_autoscroll_pointer(&mut self, position: Point<Pixels>, _cx: &mut Context<Self>) {
        if let Some(session) = self.autoscroll.as_mut() {
            session.track(position);
        }
    }

    /// Ends any active autoscroll session (middle click again, Escape, click).
    ///
    /// Dropping the driver is what stops the frames; the session flag alone
    /// would leave a task waking every 16ms to find nothing to do.
    pub fn cancel_autoscroll(&mut self, cx: &mut Context<Self>) {
        self.autoscroll_task = None;
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
        if story == StoryId::Voice {
            self.voice_state = VoiceState::Listening { level: 0.6 };
        }
        if story == StoryId::ToolCalls {
            self.simulation_task.take();
            self.tool_group_open = Some(true);
        }
        cx.notify();
    }

    /// Flips the Thinking trace's controlled disclosure — exactly the state
    /// a reader's toggle flips — so the gate can reverse it mid-flight.
    #[cfg(feature = "performance")]
    pub fn set_performance_thinking_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.trace_open = open;
        cx.notify();
    }

    /// Reverses the composed tool disclosures while their header motion is active.
    #[cfg(feature = "performance")]
    pub fn set_performance_tools_open(&mut self, open: bool, cx: &mut Context<Self>) {
        for id in [
            "read-pricing",
            "search-suppliers",
            "send-confirmations",
            "query-prices",
        ] {
            self.tool_call_open.insert(id.into(), open);
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

        let frame = story_frame(story, self.selected == StoryId::All)
            .debug_selector(move || format!("story-{}", story.slug()))
            .w_full()
            // A specimen is read at a prose column's width; a surface that
            // carries a layout — a grid, a transcript, a navigation pane —
            // takes the whole demo column, because at a prose width it is
            // squeezed rather than shown. Both sit inside the website's own
            // demo column, so a story is drawn at the width it was measured
            // at. The dock composition is an internal application-layout
            // diagnostic and outgrows even that.
            .max_w(px(story
                .meta()
                .map_or(crate::story::StoryWidth::Column, |meta| meta.width)
                .max_width()))
            .when(story == StoryId::DockComposition, |frame| {
                frame.max_w(px(1200.))
            })
            .px_6()
            .py_4()
            .gap_3()
            // The embed's caption belongs to the page around it. In the
            // catalog this label is how you tell one story from the next; in a
            // frame the site has already titled, it is the same words twice.
            .when(self.chrome == GalleryChrome::Full, |frame| {
                frame.child(
                    div()
                        .text_xs()
                        .font_semibold()
                        .text_color(cx.theme().muted_foreground)
                        .child(title),
                )
            })
            .child(content());

        // Only the embed reports: in the full gallery the stories are stacked
        // in one scrolling column and there is no host to tell.
        if self.chrome != GalleryChrome::Embedded {
            return frame.into_any_element();
        }
        div()
            .relative()
            .w_full()
            .child(frame)
            .child(height_probe())
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
        // A row never gives back height it has already taken, so a story
        // whose animation restarts does not drag every row below it up the
        // screen. The floor is the row's own measurement: the website's
        // declared heights are the website's business, and the gallery
        // draws a story at whatever size it actually is.
        //
        // Only here, never in the single-story view — that is what the web
        // embed renders, and it reports its measured height to the host. A
        // floor there would have it report a high-water mark instead of the
        // truth, which is the one place this has to be exact.
        //
        // The wrapper carries no role, so it is not reported to assistive
        // technology and the list's items stay the list's own children. It
        // also carries no padding, which is why the probe inside it reads
        // the row's full height rather than the frame's content box.
        let width = f32::from(window.viewport_size().width);
        let floor = self.story_floors.borrow_mut().floor_at(width, story);
        let floors = self.story_floors.clone();
        div()
            .relative()
            .w_full()
            .min_h(px(floor))
            .child(self.render_story(story, window, cx))
            .child(
                canvas(
                    move |bounds, _, _| {
                        let height = f32::from(bounds.size.height).max(0.);
                        floors.borrow_mut().observe(story, height);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
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
            StoryId::GuidedDemo => {
                let guided =
                    window.use_keyed_state(("guided-demo-story-state", self.generation), cx, GuidedDemoStory::new);
                self.section(story, "Guided demo", || guided, cx)
            }
            StoryId::DockComposition => {
                let dock = window.use_keyed_state(
                    ("dock-composition-story-state", self.generation),
                    cx,
                    DockCompositionStory::new,
                );
                self.section(story, "Dock composition", || dock, cx)
            }
            StoryId::MotionLab => {
                let lab = window.use_keyed_state(
                    ("motion-lab-story-state", self.generation),
                    cx,
                    MotionLabStory::new,
                );
                self.section(story, "Motion lab", || lab, cx)
            }
            StoryId::ThemesTrio => self.section(
                story,
                "Themes trio",
                || {
                    // The themes page's one-runtime specimen: the same three
                    // components its three separate demos used to boot, in one
                    // view. Content mirrors the standalone stories closely
                    // enough to compare themes by, without their snippet
                    // markers — this composition exports nothing.
                    v_flex()
                        .gap_6()
                        .child(LoadingState::new().label("Reasoning about supplier pricing"))
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_2()
                                .child(
                                    ToolChip::new("trio-chip-read", "read pricing.md")
                                        .status(ToolStatus::Success),
                                )
                                .child(
                                    ToolChip::new("trio-chip-edit", "edit suppliers.rs")
                                        .status(ToolStatus::Running),
                                )
                                .child(
                                    ToolChip::new("trio-chip-fetch", "fetch supplier quotas")
                                        .status(ToolStatus::Failed),
                                ),
                        )
                        .child(
                            ContextCard::new("trio-ctx-pricing", "pricing.md")
                                .snippet(
                                    "Enterprise volume pricing is renegotiated quarterly;                                      the March sheet supersedes all prior quotes.",
                                )
                                .relevance(0.92),
                        )
                },
                cx,
            ),
            StoryId::Loading => self.section(
                story,
                "Loading state",
                || {
                    // snippet:start(loading)
                    LoadingState::new()
                        .label("Reasoning about supplier pricing")
                        .elapsed(elapsed)
                    // snippet:end
                },
                cx,
            ),
            StoryId::Status => self.section(
                story,
                "Status badge",
                || {
                    // The pill's whole reason to exist is the moment a
                    // lifecycle changes, so one badge is driven through it on
                    // the simulation clock — pending, running, completed,
                    // failed, around again — swapping inside its fixed slot,
                    // while the row above holds every tone still for
                    // comparison.
                    let phase = (elapsed.as_secs() / 3) % 4;
                    let driven = match phase {
                        0 => ProgressState::Pending,
                        1 => ProgressState::Running,
                        2 => ProgressState::Complete,
                        _ => ProgressState::Failed("offline".into()),
                    };
                    let tokens = cx.theme().semantic_tokens();
                    // snippet:start(status)
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap(tokens.spacing.sm)
                                .child(StatusBadge::new("status-neutral", "Queued"))
                                .child(
                                    StatusBadge::new("status-info", "Indexing")
                                        .tone(StatusTone::Info)
                                        .active(true),
                                )
                                .child(
                                    StatusBadge::new("status-success", "Deployed")
                                        .tone(StatusTone::Success),
                                )
                                .child(
                                    StatusBadge::new("status-warning", "Needs review")
                                        .tone(StatusTone::Warning),
                                )
                                .child(
                                    StatusBadge::new("status-danger", "Rolled back")
                                        .tone(StatusTone::Danger),
                                ),
                        )
                        .child(StatusBadge::for_progress("status-driven", &driven))
                    // snippet:end
                },
                cx,
            ),
            StoryId::ToolChips => self.section(
                story,
                "Tool chips",
                || {
                    // snippet:start(tool-chips)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Tasks => self.section(
                story,
                "Task rows",
                || {
                    // snippet:start(tasks)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Thinking => self.section(
                story,
                "Thinking",
                || {
                    // snippet:start(thinking)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Orbs => self.section(
                story,
                "Orbs",
                || {
                    // snippet:start(orbs)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Search => self.section(
                story,
                "Web search",
                || {
                    // snippet:start(search)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Todos => self.section(
                story,
                "To-do list",
                || {
                    // snippet:start(todos)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::ImageGeneration => self.section(
                story,
                "Image generation",
                || {
                    // snippet:start(image-generation)
                    // The demo placeholder must not read as a partially
                    // rendered image: a flat muted canvas behind the pulsing
                    // icon keeps the progress state honest until pixels
                    // arrive.
                    ImageGeneration::new("gen")
                        .label("Label sketch: alpine meadow, morning light")
                        .progress(self.sim.progress())
                    // snippet:end
                },
                cx,
            ),
            StoryId::StreamingText => self.section(
                story,
                "Streaming text",
                || {
                    // snippet:start(streaming-text)
                    let cited_answer = Progressive::complete(
                        "Pistachio margins lead vanilla by eight points [[cite:margin-report]], while the supply forecast remains stable [[cite:supply-forecast]]."
                            .into(),
                    );
                    v_flex()
                        .gap_4()
                        .child(
                            StreamingText::new("answer", &self.sim.answer)
                                .source_refs([
                                    SourceRef::new("pricing.md"),
                                    SourceRef::new("suppliers.csv"),
                                    SourceRef::with_id("dairy-index", "2026 Dairy Price Index")
                                        .url("https://dairyreport.org/index/2026"),
                                    SourceRef::with_id("alpenrose", "Alpenrose wholesale programs")
                                        .url("https://www.alpenrose.com/wholesale"),
                                ])
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Chat => {
                let answer = self.sim.answer.clone();
                let chat_story = window.use_keyed_state(
                    ("chat-story-state", self.generation),
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
                    // snippet:start(suggestions)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Attachments => self.section(
                story,
                "Attachment previews",
                || {
                    // snippet:start(attachments)
                    let tokens = cx.theme().semantic_tokens();
                    let composer_items = demo_attachments(&self.demo_thumbnail)
                        .into_iter()
                        .filter(|item| !self.removed_attachments.contains(item.id()))
                        .collect::<Vec<_>>();
                    let status: SharedString = self
                        .last_attachment_event
                        .clone()
                        .unwrap_or_else(|| "Remove a composer file or open a message file".into());
                    let caption = |text: &'static str| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(text)
                    };
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(caption("Composer: compact and removable"))
                        .child(
                            AttachmentStrip::new("composer-attachments")
                                .label("Prompt attachments")
                                .items(composer_items)
                                .removable(true)
                                .compact(true)
                                .on_event(cx.listener(|this, event: &AttachmentEvent, _, cx| {
                                    if let AttachmentEvent::Removed { id } = event {
                                        this.removed_attachments.insert(id.clone());
                                        this.last_attachment_event =
                                            Some(format!("Removed {id}").into());
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(caption("Message: read-only, openable tiles"))
                        .child(
                            AttachmentStrip::new("message-attachments")
                                .label("Message attachments")
                                .items(demo_attachments(&self.demo_thumbnail))
                                .on_event(cx.listener(|this, event: &AttachmentEvent, _, cx| {
                                    if let AttachmentEvent::Opened { id } = event {
                                        this.last_attachment_event =
                                            Some(format!("Opened {id}").into());
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(caption("Upload lifecycle: queued, uploading, failed"))
                        .child(
                            AttachmentStrip::new("attachment-states")
                                .label("Upload states")
                                .items(demo_attachment_states()),
                        )
                        .child(
                            div()
                                .id("attachments-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::Artifact => self.section(
                story,
                "Artifact panel",
                || {
                    // snippet:start(artifact)
                    let tokens = cx.theme().semantic_tokens();
                    let status: SharedString = self
                        .last_artifact_event
                        .clone()
                        .unwrap_or_else(|| "Switch views or versions, run an action, or close the panel".into());
                    let version = self.artifact_version.min(ARTIFACT_VERSIONS.len() - 1);
                    let (version_id, source) = ARTIFACT_VERSIONS[version];
                    let artifact = Artifact::new("comparison", "Supplier comparison", if version + 1 == ARTIFACT_VERSIONS.len() {
                        StreamedContent::running(source.to_owned())
                    } else {
                        StreamedContent::done(source)
                    })
                    .kind(ArtifactKind::Markdown)
                    .versions(ARTIFACT_VERSIONS.iter().enumerate().map(|(index, (id, _))| {
                        ArtifactVersion::new(*id, format!("v{}", index + 1))
                    }))
                    .active_version(version_id);
                    let conversation = v_flex()
                        .size_full()
                        .gap(tokens.spacing.sm)
                        .p(tokens.spacing.md)
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("Conversation"),
                        )
                        .child(
                            TextView::markdown(
                                "artifact-conversation",
                                "**User:** Compare the three suppliers for next month.\n\n**Agent:** I drafted a comparison as a document on the right; v3 is still being written.",
                            )
                            .selectable(true),
                        )
                        .when(!self.artifact_open, |this| {
                            this.child(
                                Button::new("artifact-reopen")
                                    .outline()
                                    .small()
                                    .text_label("Reopen artifact")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.artifact_open = true;
                                        this.last_artifact_event = Some("Reopened".into());
                                        cx.notify();
                                    })),
                            )
                        });
                    let panel = ArtifactPanel::new("artifact-panel", &artifact)
                        .view(self.artifact_view)
                        .actions([
                            ArtifactAction::new("open", "Open in editor"),
                            ArtifactAction::new("export", "Export"),
                        ])
                        .on_event(cx.listener(|this, event: &ArtifactPanelEvent, _, cx| {
                            match event {
                                ArtifactPanelEvent::Closed { .. } => this.artifact_open = false,
                                ArtifactPanelEvent::ViewSelected { view, .. } => {
                                    this.artifact_view = *view;
                                }
                                ArtifactPanelEvent::VersionSelected { version_id, .. } => {
                                    if let Some(index) = ARTIFACT_VERSIONS
                                        .iter()
                                        .position(|(id, _)| *id == version_id.as_ref())
                                    {
                                        this.artifact_version = index;
                                    }
                                }
                                ArtifactPanelEvent::ActionActivated { .. } => {}
                            }
                            this.last_artifact_event = Some(format!("{event:?}").into());
                            cx.notify();
                        }));
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            div()
                                .id("artifact-story-host")
                                .h(px(440.))
                                .w_full()
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded(tokens.radius.lg)
                                .overflow_hidden()
                                .child(
                                    h_resizable("artifact-split")
                                        .with_state(&self.artifact_split)
                                        .child(
                                            resizable_panel()
                                                .size(px(220.))
                                                .size_range(px(160.)..px(360.))
                                                .child(conversation),
                                        )
                                        .child(
                                            resizable_panel()
                                                .visible(self.artifact_open)
                                                .child(div().size_full().p(tokens.spacing.sm).child(panel)),
                                        ),
                                ),
                        )
                        .child(
                            div()
                                .id("artifact-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::Voice => self.section(
                story,
                "Voice controls",
                || {
                    // snippet:start(voice)
                    let tokens = cx.theme().semantic_tokens();
                    let status: SharedString = self
                        .last_voice_event
                        .clone()
                        .unwrap_or_else(|| "Start dictation, then stop or cancel it; Speak reads the latest reply".into());
                    let mut controls = VoiceControls::new("voice", self.voice_state)
                        .speakable(true)
                        .on_event(cx.listener(|this, event: &VoiceEvent, _, cx| {
                            match event {
                                VoiceEvent::DictationStarted => {
                                    this.voice_state = VoiceState::Listening { level: 0.55 };
                                    this.voice_transcript =
                                        Some("compare the three suppliers for next".into());
                                }
                                VoiceEvent::DictationStopped => {
                                    this.voice_state = VoiceState::Transcribing;
                                }
                                VoiceEvent::DictationCancelled => {
                                    this.voice_state = VoiceState::Idle;
                                    this.voice_transcript = None;
                                }
                                VoiceEvent::SpeakRequested => this.voice_state = VoiceState::Speaking,
                                VoiceEvent::SpeakStopped => this.voice_state = VoiceState::Idle,
                            }
                            this.last_voice_event = Some(format!("{event:?}").into());
                            cx.notify();
                        }));
                    if let Some(transcript) = self.voice_transcript.clone() {
                        controls = controls.transcript(transcript);
                    }
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(controls)
                        .when(self.voice_state == VoiceState::Transcribing, |this| {
                            this.child(
                                Button::new("voice-finish")
                                    .outline()
                                    .small()
                                    .text_label("Simulate: transcription finished")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.voice_state = VoiceState::Idle;
                                        this.voice_transcript = None;
                                        this.last_voice_event = Some(
                                            "Transcribed: \"compare the three suppliers for next month\"".into(),
                                        );
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            div()
                                .id("voice-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::Queue => self.section(
                story,
                "Message queue",
                || {
                    // snippet:start(queue)
                    let tokens = cx.theme().semantic_tokens();
                    let status: SharedString = self
                        .last_queue_event
                        .clone()
                        .unwrap_or_else(|| "Reorder, edit, send now, or remove a waiting prompt".into());
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            MessageQueue::new("queue")
                                .items(self.queued.iter().cloned())
                                .editable(true)
                                .on_event(cx.listener(|this, event: &QueueEvent, _, cx| {
                                    match event {
                                        QueueEvent::Removed { id } | QueueEvent::SentNow { id } => {
                                            this.queued.retain(|item| item.id() != id);
                                        }
                                        QueueEvent::MovedUp { id } => {
                                            if let Some(index) =
                                                this.queued.iter().position(|item| item.id() == id)
                                                && index > 0
                                            {
                                                this.queued.swap(index, index - 1);
                                            }
                                        }
                                        QueueEvent::MovedDown { id } => {
                                            if let Some(index) =
                                                this.queued.iter().position(|item| item.id() == id)
                                                && index + 1 < this.queued.len()
                                            {
                                                this.queued.swap(index, index + 1);
                                            }
                                        }
                                        QueueEvent::EditRequested { .. } => {}
                                        QueueEvent::Cleared => this.queued.clear(),
                                    }
                                    this.last_queue_event = Some(format!("{event:?}").into());
                                    cx.notify();
                                })),
                        )
                        .when(self.queued.is_empty(), |this| {
                            this.child(
                                Button::new("queue-refill")
                                    .outline()
                                    .small()
                                    .text_label("Queue the demo prompts again")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.queued = demo_queue();
                                        this.last_queue_event = Some("Refilled".into());
                                        cx.notify();
                                    })),
                            )
                        })
                        .child(
                            div()
                                .id("queue-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::ContextMeter => self.section(
                story,
                "Context meter",
                || {
                    // snippet:start(context-meter)
                    let tokens = cx.theme().semantic_tokens();
                    let comfortable = ContextUsage::new(84_300, 200_000)
                        .input(61_000)
                        .output(19_800)
                        .cached(3_500)
                        .cost("$0.42");
                    let elevated = ContextUsage::new(148_000, 200_000)
                        .input(120_000)
                        .output(22_000)
                        .reasoning(6_000)
                        .cost("$0.91");
                    let critical = ContextUsage::new(186_500, 200_000)
                        .input(150_000)
                        .output(30_000)
                        .reasoning(6_500)
                        .cost("$1.28");
                    let row = |label: &'static str, variant: ContextMeterVariant| {
                        h_flex()
                            .items_center()
                            .flex_wrap()
                            .gap(tokens.spacing.lg)
                            .child(
                                div()
                                    .w(tokens.spacing.xxl * 2.0)
                                    .text_xs()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(label),
                            )
                            .child(
                                ContextMeter::new(format!("ctx-{label}-comfortable"), &comfortable)
                                    .variant(variant),
                            )
                            .child(
                                ContextMeter::new(format!("ctx-{label}-elevated"), &elevated)
                                    .variant(variant),
                            )
                            .child(
                                ContextMeter::new(format!("ctx-{label}-critical"), &critical)
                                    .variant(variant),
                            )
                    };
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(row("Ring", ContextMeterVariant::Ring))
                        .child(row("Bar", ContextMeterVariant::Bar))
                        .child(row("Text", ContextMeterVariant::Text))
                    // snippet:end
                },
                cx,
            ),
            StoryId::CommandSearch => {
                let command_story = window.use_keyed_state(
                    ("command-search-story-state", self.generation),
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
                    ("sidebar-nav-story-state", self.generation),
                    cx,
                    SidebarNavStory::new,
                );
                self.section(story, "Sidebar navigation", || sidebar_story, cx)
            }
            StoryId::ThreadList => {
                let thread_story =
                    window.use_keyed_state(("thread-list-story-state", self.generation), cx, ThreadListStory::new);
                self.section(story, "Thread list", || thread_story, cx)
            }
            StoryId::FineTune => {
                let fine_tune_story = window.use_keyed_state(
                    ("fine-tune-story-state", self.generation),
                    cx,
                    FineTuneStory::new,
                );
                self.section(story, "Fine-tune card", || fine_tune_story, cx)
            }
            StoryId::RecordsTable => {
                let records_story = window.use_keyed_state(
                    ("records-table-story-state", self.generation),
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
                    ("diff-table-story-state", self.generation),
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
                    ("filter-table-story-state", self.generation),
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
                    ("comparison-table-story-state", self.generation),
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
            StoryId::CodeDiff => self.section(
                story,
                "Code diff",
                || {
                    // snippet:start(code-diff)
                    let tokens = cx.theme().semantic_tokens();
                    let mut file = DiffFile::from_unified(DEMO_PATCH).remove(0);
                    let hunks: Vec<DiffHunk> = file
                        .hunk_refs()
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(index, hunk)| {
                            hunk.review(
                                self.hunk_reviews
                                    .get(&index)
                                    .copied()
                                    .unwrap_or_default(),
                            )
                        })
                        .collect();
                    file = file.hunks(hunks);
                    let status: SharedString = self
                        .last_diff_event
                        .clone()
                        .unwrap_or_else(|| "Accept or reject a hunk, or collapse the file".into());
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            CodeDiff::new("proposed-patch", &file)
                                .open(self.diff_open)
                                .reviewable(true)
                                .on_event(cx.listener(|this, event: &CodeDiffEvent, _, cx| {
                                    match event {
                                        CodeDiffEvent::Toggled { .. } => {
                                            this.diff_open = !this.diff_open;
                                        }
                                        CodeDiffEvent::HunkAccepted { hunk, .. } => {
                                            this.hunk_reviews.insert(*hunk, HunkReview::Accepted);
                                        }
                                        CodeDiffEvent::HunkRejected { hunk, .. } => {
                                            this.hunk_reviews.insert(*hunk, HunkReview::Rejected);
                                        }
                                    }
                                    this.last_diff_event = Some(format!("{event:?}").into());
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("code-diff-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::CodeBlock => self.section(
                story,
                "Code block",
                || {
                    // snippet:start(code-block)
                    CodeBlock::streamed("code", &self.sim.code).language("rust")
                    // snippet:end
                },
                cx,
            ),
            StoryId::Approval => self.section(
                story,
                "Approval card",
                || {
                    // snippet:start(approval)
                    let tokens = cx.theme().semantic_tokens();
                    let decision_of = |id: &str| {
                        self.approval_decisions
                            .get(id)
                            .copied()
                            .unwrap_or_default()
                    };
                    let make_handler = || {
                        cx.listener(|this, event: &ApprovalEvent, _, cx| {
                        let (id, decision) = match event {
                            ApprovalEvent::Approved { id } | ApprovalEvent::ApprovedAlways { id } => {
                                (id.clone(), ApprovalDecision::Approved)
                            }
                            ApprovalEvent::Rejected { id } => (id.clone(), ApprovalDecision::Rejected),
                        };
                        this.approval_decisions.insert(id, decision);
                        this.last_approval_event = Some(format!("{event:?}").into());
                        cx.notify();
                        })
                    };
                    let status: SharedString = self
                        .last_approval_event
                        .clone()
                        .unwrap_or_else(|| "Decide a gate to see its resolved state".into());
                    let caption = |text: &'static str| {
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(text)
                    };
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(caption("Default tone with a payload"))
                        .child(
                            ApprovalCard::new("gate", "Send order confirmation to 3 suppliers?")
                                .description(
                                    "Emails will go out immediately and cannot be recalled.",
                                )
                                .decision(decision_of("gate"))
                                .note("Decided in this session")
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
                                .on_event(make_handler()),
                        )
                        .child(caption("Destructive tone with \"Always allow\""))
                        .child(
                            ApprovalCard::new("purge", "Delete 12 stale supplier records?")
                                .description("Records older than 18 months are removed permanently.")
                                .tone(ApprovalTone::Destructive)
                                .approve_label("Delete records")
                                .allow_always(true)
                                .decision(decision_of("purge"))
                                .note("Decided in this session")
                                .on_event(make_handler()),
                        )
                        .child(
                            div()
                                .id("approval-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::Plan => self.section(
                story,
                "Plan card",
                || {
                    // snippet:start(plan)
                    let tokens = cx.theme().semantic_tokens();
                    let status: SharedString = self
                        .last_plan_event
                        .clone()
                        .unwrap_or_else(|| "Approve, reject, or open a step".into());
                    let steps = [
                        PlanStep::new("compare", "Compare unit prices across suppliers")
                            .detail("pricing.md · 3 suppliers")
                            .status(PlanStepStatus::Done),
                        PlanStep::new("risk", "Check delivery-window risk")
                            .detail("Cold-chain capacity for the next two weeks")
                            .status(PlanStepStatus::Done),
                        PlanStep::new("draft", "Draft the revised bulk order")
                            .status(match self.plan_state {
                                PlanState::Approved | PlanState::Running => PlanStepStatus::Running,
                                _ => PlanStepStatus::Pending,
                            }),
                        PlanStep::new("send", "Send confirmations")
                            .detail("Emails 3 suppliers; needs its own approval"),
                    ];
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            PlanCard::new("rollout", "Switch bulk orders to Alpenrose Dairy")
                                .description(
                                    "Four steps; the last one sends email and will ask again.",
                                )
                                .steps(steps)
                                .state(self.plan_state)
                                .note("Decided in this session")
                                .editable(true)
                                .on_event(cx.listener(|this, event: &PlanEvent, _, cx| {
                                    match event {
                                        PlanEvent::Approved { .. } => {
                                            this.plan_state = PlanState::Approved;
                                        }
                                        PlanEvent::Rejected { .. } => {
                                            this.plan_state = PlanState::Rejected;
                                        }
                                        PlanEvent::EditRequested { .. }
                                        | PlanEvent::StepActivated { .. } => {}
                                    }
                                    this.last_plan_event = Some(format!("{event:?}").into());
                                    cx.notify();
                                })),
                        )
                        .child(
                            div()
                                .id("plan-story-status")
                                .role(Role::Status)
                                .aria_label(status.clone())
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(status),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::Recommendation => self.section(
                story,
                "Recommendation card",
                || {
                    // snippet:start(recommendation)
                    RecommendationCard::new("rec", "Switch supplier to Alpenrose Dairy")
                        .description("Lower unit cost at equal volume; delivery risk unchanged.")
                        .confidence(0.87)
                        .alternatives(["Keep current supplier", "Split volume 60/40"])
                        .on_event(cx.listener(|_, event: &RecommendationEvent, _, _| {
                            println!("recommendation event: {event:?}");
                        }))
                    // snippet:end
                },
                cx,
            ),
            StoryId::Context => self.section(
                story,
                "Context cards",
                || {
                    // snippet:start(context)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Insights => self.section(
                story,
                "Insight card",
                || {
                    // snippet:start(insights)
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
                            div()
                                .debug_selector(|| "insight-story-end".into())
                                .h(px(1.)),
                        )
                    // snippet:end
                },
                cx,
            ),
            StoryId::PromptBar => {
                let prompt_story = window.use_keyed_state(
                    ("prompt-bar-story-state", self.generation),
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
                            // One composer at a time now, so the box is sized
                            // for one: 256px clipped suggestion rows mid-card,
                            // and the grouped model menu still needs room to
                            // open below the composer.
                            .h(px(400.))
                            .max_h(px(400.))
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
                    // snippet:start(tool-calls)
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
                    // snippet:end
                },
                cx,
            ),
            StoryId::Form => {
                let form = window.use_keyed_state(
                    ("form-story-state", self.generation),
                    cx,
                    FormStory::new,
                );
                self.section(story, "Form controls", || form, cx)
            }
            StoryId::QuestionFlow => {
                let flow = window.use_keyed_state(
                    ("question-flow-story-state", self.generation),
                    cx,
                    QuestionFlowStory::new,
                );
                self.section(story, "Question flow", || flow, cx)
            }
            StoryId::Decorations => {
                let decorations = window.use_keyed_state(
                    ("decorations-story-state", self.generation),
                    cx,
                    DecorationsStory::new,
                );
                self.section(story, "Decorations", || decorations, cx)
            }
            StoryId::SelectionActions => {
                let selection_story = window.use_keyed_state(
                    ("selection-actions-story-state", self.generation),
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
        // The rem is a resolved-layout input, so a change to it invalidates
        // every measured story height. Reading it here mutates nothing; the
        // reaction is deferred so that render never notifies.
        let rem_size = window.rem_size();
        if !self.resolved_layout.matches(rem_size) {
            cx.defer_in(window, move |gallery, _, cx| {
                gallery.resolve_layout(rem_size, cx);
            });
        }

        // The catalog's keys are dispatched through this, and the embed is
        // deliberately left alone: `GalleryChrome::Embedded` is set before the
        // first frame, so a demo on the website never takes focus from it.
        if self.chrome == GalleryChrome::Full && !self.focused_on_open {
            self.focused_on_open = true;
            cx.defer_in(window, |gallery, window, cx| {
                window.focus(&gallery.focus, cx);
            });
        }

        if let Some(meter) = self.metrics.as_mut() {
            meter.record(std::time::Instant::now());
            // Nothing else may be moving, and a meter that reads zero for a
            // still picture answers the wrong question. See metrics.rs.
            window.request_animation_frame();
        }

        let content = if self.selected == StoryId::All {
            div()
                .id("gallery-scroll")
                .track_focus(&self.focus)
                // The keybindings below are scoped to this name. Without the
                // context to match, every one of them was unreachable: the
                // predicate can only be satisfied by a `key_context`, and an
                // element id is not one.
                .key_context("gallery-scroll")
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
                .on_action(cx.listener(|this, _: &ToggleMetrics, _, cx| {
                    this.toggle_metrics(cx);
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
            let story = self.render_story(self.selected, window, cx);
            div()
                .id("gallery-scroll")
                .track_focus(&self.focus)
                .key_context("gallery-scroll")
                .on_action(cx.listener(|this, _: &ToggleMetrics, _, cx| {
                    this.toggle_metrics(cx);
                }))
                .debug_selector(|| "gallery-scroll".into())
                .flex_1()
                .min_h_0()
                .overflow_y_scrollbar()
                .child(
                    // Horizontally centred, never vertically. A host sizes its
                    // frame to the height the story measures, so for every
                    // story but one the two are equal and centring does
                    // nothing. The exception is the guided demo, which starts
                    // as a bare composer and grows to five times that: centred,
                    // it opened floating in the middle of an empty box, which
                    // is the dead space the measured heights exist to remove.
                    v_flex().w_full().min_h_full().items_center().child(story),
                )
                .into_any_element()
        };

        v_flex()
            .size_full()
            .overflow_hidden()
            .bg(cx.theme().background)
            .when(self.chrome == GalleryChrome::Full, |this| {
                this.child(
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
                                .text_label(format!("Theme: {}", self.theme.label()))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.theme = this.theme.next();
                                    apply_gallery_theme(this.theme, Some(window), cx);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(content)
            .when_some(self.metrics.as_ref(), |frame, meter| {
                // Last child, so it paints over the story: GPUI has no
                // z-index, and paint order is child order.
                frame
                    .relative()
                    .child(crate::metrics::overlay(meter.reading(), cx))
            })
    }
}

impl Gallery {
    /// Draws the story without the gallery's own title and theme control.
    ///
    /// The website frames each demo itself, so the embed must not repeat that
    /// furniture inside the frame.
    pub fn set_chrome(&mut self, chrome: GalleryChrome, cx: &mut Context<Self>) {
        if self.chrome == chrome {
            return;
        }
        self.chrome = chrome;
        cx.notify();
    }

    /// Shows or hides the frame meter (F3).
    ///
    /// Dropped rather than hidden when off, so a closed meter costs nothing
    /// and an old reading can never be shown for new content.
    fn toggle_metrics(&mut self, cx: &mut Context<Self>) {
        self.metrics = match self.metrics {
            Some(_) => None,
            None => Some(crate::metrics::FrameMeter::default()),
        };
        cx.notify();
    }

    /// Puts every story back to the state it opened in.
    ///
    /// Rebuilt rather than reset field by field: this struct carries the state
    /// of thirty-four stories — which tool calls are expanded, which gates were
    /// decided, which artifact version is showing, how far each simulation has
    /// run — and a reset that walked them one at a time would be a list nobody
    /// remembers to add to.
    ///
    /// The selection, the theme, and the chrome survive, because those are the
    /// page's decisions rather than the story's.
    pub fn reset_story(&mut self, cx: &mut Context<Self>) {
        let chrome = self.chrome;
        // The stories the window holds are keyed by this, so a new number is
        // how they are asked for again. Without it a reset chat kept its
        // transcript and a reset table kept its sort: replacing this struct
        // only reaches the state this struct owns.
        let generation = self.generation.wrapping_add(1);
        *self = Self::new_with_theme(self.selected, Some(self.theme), cx);
        self.chrome = chrome;
        self.generation = generation;
        cx.notify();
    }
}

/// Initializes the component and theme globals used by every gallery host.
pub fn init(cx: &mut App) {
    gpui_ai::init(cx);
    // Every pack under themes/ — gpui-ai's own presets and the vendored
    // upstream set — is embedded by build.rs and registered here, so the
    // picker and the website's downloadable themes cannot drift apart.
    for pack in BUNDLED_THEME_FILES {
        ThemeRegistry::global_mut(cx)
            .load_themes_from_str(pack)
            .expect("bundled theme packs must be valid JSON");
    }
    cx.bind_keys([
        KeyBinding::new("pageup", PageUp, Some("gallery-scroll")),
        KeyBinding::new("pagedown", PageDown, Some("gallery-scroll")),
        KeyBinding::new("home", ScrollHome, Some("gallery-scroll")),
        KeyBinding::new("end", ScrollEnd, Some("gallery-scroll")),
        KeyBinding::new("escape", CancelAutoscroll, Some("gallery-scroll")),
        KeyBinding::new("f3", ToggleMetrics, Some("gallery-scroll")),
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
        AUTOSCROLL_FRAME_INTERVAL, ChatStory, Gallery, GalleryTheme, GuidedDemoStory, GuidedStage,
        Theme, filter_story_project_rows, filter_story_rows, reduce_filter_story_projection,
    };
    use crate::StoryId;
    use gpui::{
        AppContext as _, Context, Element as _, Entity, IntoElement as _, Modifiers, MouseButton,
        Render, Role, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, Window,
        accesskit, point, px, size,
    };
    use gpui_ai::{
        prelude::{
            AUTOSCROLL_FULL_SPEED_DISTANCE_PX, FilterRow, FilterSortDirection, FilterTableEvent,
            MAX_AUTOSCROLL_SPEED_PX_PER_SEC,
        },
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
    fn the_theme_table_starts_with_the_built_in_light_and_dark_presets() {
        assert_eq!(GalleryTheme::LIGHT.slug(), "light");
        assert!(!GalleryTheme::LIGHT.is_dark());
        assert_eq!(GalleryTheme::DARK.slug(), "dark");
        assert!(GalleryTheme::DARK.is_dark());
    }

    #[test]
    fn every_bundled_theme_file_contributes_presets_with_unique_slugs() {
        let mut slugs: Vec<&str> = GalleryTheme::all().map(GalleryTheme::slug).collect();
        let total = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), total, "theme slugs must be unique");

        // The directory drives the table: gpui-ai's own presets and the
        // vendored upstream pack both have to be present.
        assert!(GalleryTheme::from_slug("graphite").is_some());
        assert!(GalleryTheme::from_slug("solstice").is_some());
        assert!(
            GalleryTheme::all().any(|theme| theme.group() == "gpui-component"),
            "the vendored upstream pack must reach the picker"
        );
        assert!(!super::BUNDLED_THEME_FILES.is_empty());
    }

    #[test]
    fn the_review_cycle_visits_every_gpui_ai_preset_and_closes() {
        let expected: Vec<GalleryTheme> = GalleryTheme::all()
            .filter(|theme| theme.group() == super::GPUI_AI_THEME_GROUP)
            .collect();
        assert!(expected.len() >= 3, "Light, Dark, and Contrast at minimum");

        let mut visited = Vec::new();
        let mut theme = GalleryTheme::LIGHT;
        for _ in 0..expected.len() {
            theme = theme.next();
            visited.push(theme);
        }
        visited.sort_unstable_by_key(|theme| theme.slug());

        let mut wanted = expected.clone();
        wanted.sort_unstable_by_key(|theme| theme.slug());
        assert_eq!(visited, wanted, "the cycle must visit each preset once");
        assert_eq!(theme, GalleryTheme::LIGHT, "the cycle must close");
    }

    #[test]
    fn the_review_cycle_skips_the_vendored_upstream_pack() {
        let upstream = GalleryTheme::all()
            .find(|theme| theme.group() == "gpui-component")
            .expect("the upstream pack must be vendored");
        assert_eq!(
            upstream.next().group(),
            super::GPUI_AI_THEME_GROUP,
            "stepping out of the upstream pack returns to the review cycle"
        );
    }

    #[gpui::test]
    fn contrast_theme_installs_the_contrast_registry_config(cx: &mut TestAppContext) {
        cx.update(super::init);
        cx.update(|cx| {
            let contrast = GalleryTheme::from_slug("contrast").expect("contrast is bundled");
            super::apply_gallery_theme(contrast, None, cx);
            assert_eq!(cx.theme().dark_theme.name.as_ref(), "gpui-ai Contrast");
        });
    }

    /// Reset puts the story back without touching the page's own decisions.
    ///
    /// The website's Reset button used to replace the whole frame, which tears
    /// down a seventeen-megabyte WebAssembly instance to reach a state the
    /// story gets back to in a frame. It now calls this instead, so this is
    /// where "back to the start" has to be true: the browser can see that the
    /// document survived, and nothing else.
    #[gpui::test]
    fn resetting_a_story_restores_it_and_keeps_the_page_decisions(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::ToolCalls, cx));
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let ember = GalleryTheme::from_slug("ember-dusk").expect("ember-dusk is bundled");
        cx.update(|_, cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.set_chrome(super::GalleryChrome::Embedded, cx);
                gallery.set_theme_preset(ember, cx);
                // Story state a reader could have produced: a collapsed trace,
                // an expanded group, a decided gate.
                gallery.trace_open = false;
                gallery.tool_group_open = Some(true);
                gallery
                    .approval_decisions
                    .insert("deploy".into(), super::ApprovalDecision::Approved);
                assert!(gallery.is_approval_granted("deploy"));
                assert!(!gallery.is_approval_granted("unknown"));
                gallery.last_approval_event = Some("decided".into());
            })
        });

        cx.update(|_, cx| gallery.update(cx, |gallery, cx| gallery.reset_story(cx)));

        gallery.read_with(cx, |gallery, _| {
            assert!(
                gallery.trace_open,
                "reset must put the trace back as it opened"
            );
            assert_eq!(gallery.tool_group_open, None);
            assert!(gallery.approval_decisions.is_empty());
            assert!(!gallery.is_approval_granted("deploy"));
            assert_eq!(gallery.last_approval_event, None);

            // The page chose these, not the story. Losing them would repaint a
            // demo the reader had themed and re-draw furniture the page
            // already provides.
            assert_eq!(gallery.selected, StoryId::ToolCalls);
            assert_eq!(gallery.theme, ember);
            assert_eq!(gallery.chrome, super::GalleryChrome::Embedded);
        });
    }

    /// Reset reaches the stories this struct does not own.
    ///
    /// Twelve stories are entities held in the window's keyed state rather
    /// than fields here, and that state outlives this struct — so replacing
    /// the struct left a reset chat holding its transcript and a reset table
    /// still sorted. They are keyed by a generation, and reset is a new one.
    #[gpui::test]
    fn resetting_a_story_asks_for_new_entities_too(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::Chat, cx));
        let cx: &mut VisualTestContext = cx;
        let ember = GalleryTheme::from_slug("ember-dusk").expect("ember-dusk is bundled");
        cx.update(|_, cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.set_chrome(super::GalleryChrome::Embedded, cx);
                gallery.set_theme_preset(ember, cx);
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let edit = |cx: &mut VisualTestContext, replacement: Option<&str>| {
            let editor = cx
                .debug_bounds("prompt-bar-editor")
                .expect("retained prompt editor");
            cx.simulate_click(editor.center(), Modifiers::default());
            cx.simulate_keystrokes("ctrl-a");
            if let Some(text) = replacement {
                cx.simulate_input(text);
                cx.simulate_keystrokes("ctrl-a");
            }
            cx.simulate_keystrokes("ctrl-c");
            cx.update(|_, cx| cx.read_from_clipboard().and_then(|item| item.text()))
        };
        assert_eq!(
            edit(cx, Some("A reader's unsent draft")).as_deref(),
            Some("A reader's unsent draft")
        );
        cx.update(|_, cx| gallery.update(cx, |gallery, cx| gallery.reset_story(cx)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            edit(cx, None).as_deref(),
            Some("Ask a follow-up about suppliers"),
            "reset must replace the retained editor, not just reset the gallery's own fields"
        );
        gallery.read_with(cx, |gallery, _| {
            assert_eq!(gallery.selected, StoryId::Chat);
            assert_eq!(gallery.theme, ember);
            assert_eq!(gallery.chrome, super::GalleryChrome::Embedded);
        });
    }

    /// A theme that says nothing about type size must not inherit the last one's.
    ///
    /// gpui-component only overwrites a metric the incoming config declares,
    /// and what it leaves behind is the previous theme's value rather than the
    /// default. Three of the forty-five bundled themes set metrics the other
    /// forty-two leave out, so on the website choosing Graphite and then
    /// anything else left every demo at 14px with square corners until the
    /// page was reloaded.
    #[gpui::test]
    fn a_theme_that_sets_no_metrics_gets_the_defaults_back(cx: &mut TestAppContext) {
        cx.update(super::init);
        cx.update(|cx| {
            let default_size = cx.theme().font_size;
            let default_radius = cx.theme().radius;

            let graphite = GalleryTheme::from_slug("graphite").expect("graphite is bundled");
            super::apply_gallery_theme(graphite, None, cx);
            assert_eq!(
                cx.theme().font_size,
                px(14.),
                "graphite asks for 14px type and must get it"
            );
            assert_ne!(
                cx.theme().radius,
                default_radius,
                "graphite asks for square corners, so this test needs it to differ"
            );

            // Nord Frost declares colours and nothing else.
            let nord = GalleryTheme::from_slug("nord-frost").expect("nord-frost is bundled");
            super::apply_gallery_theme(nord, None, cx);
            assert_eq!(
                cx.theme().font_size,
                default_size,
                "a theme with no font size must read at the default, not at the last theme's"
            );
            assert_eq!(
                cx.theme().radius,
                default_radius,
                "a theme with no radius must use the default corners"
            );

            // And the other direction, so the restore cannot simply be
            // clobbering every metric with a default.
            let solstice = GalleryTheme::from_slug("solstice").expect("solstice is bundled");
            super::apply_gallery_theme(solstice, None, cx);
            assert_eq!(
                cx.theme().font_size,
                px(17.),
                "a theme that does ask for a size must still get it"
            );
        });
    }

    #[gpui::test]
    fn a_vendored_upstream_theme_applies_from_the_registry(cx: &mut TestAppContext) {
        cx.update(super::init);
        cx.update(|cx| {
            let tokyo = GalleryTheme::from_slug("tokyo-night").expect("upstream pack is vendored");
            super::apply_gallery_theme(tokyo, None, cx);
            assert_eq!(cx.theme().dark_theme.name.as_ref(), "Tokyo Night");
        });
    }

    /// Every one of the catalog's keybindings named a context nothing set, and
    /// the window focused nothing to dispatch through, so none of them ever
    /// fired. Both halves are needed: this fails without either.
    #[gpui::test]
    fn page_down_scrolls_the_catalog(cx: &mut TestAppContext) {
        let (gallery, cx) = all_stories(cx);
        let before = gallery.read_with(cx, |gallery, _| {
            gallery.catalog_list.logical_scroll_top().item_ix
        });
        cx.simulate_keystrokes("pagedown");
        cx.run_until_parked();
        let after = gallery.read_with(cx, |gallery, _| {
            gallery.catalog_list.logical_scroll_top().item_ix
        });
        assert_ne!(before, after, "Page Down must move the catalog");
    }

    #[gpui::test]
    fn f3_opens_and_closes_the_frame_meter(cx: &mut TestAppContext) {
        let (gallery, cx) = all_stories(cx);
        assert!(
            gallery.read_with(cx, |gallery, _| gallery.metrics.is_none()),
            "the meter costs a redraw a frame, so it starts closed"
        );

        cx.simulate_keystrokes("f3");
        cx.run_until_parked();
        assert!(gallery.read_with(cx, |gallery, _| gallery.metrics.is_some()));

        cx.simulate_keystrokes("f3");
        cx.run_until_parked();
        assert!(
            gallery.read_with(cx, |gallery, _| gallery.metrics.is_none()),
            "closing it must stop the continuous redraw, not just hide the box"
        );
    }

    /// A single story is where a decoration is actually judged, and it is a
    /// different element from the catalog with its own dispatch setup.
    #[gpui::test]
    fn f3_opens_the_frame_meter_on_a_single_story(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::Decorations, cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.simulate_keystrokes("f3");
        cx.run_until_parked();
        assert!(gallery.read_with(cx, |gallery, _| gallery.metrics.is_some()));
    }

    #[gpui::test]
    fn all_stories_exposes_a_vertical_scrollbar(cx: &mut TestAppContext) {
        let (_, cx) = all_stories(cx);

        assert!(cx.debug_bounds("scrollbar-overlay").is_some());
    }

    #[gpui::test]
    fn dock_composition_uses_the_desktop_width_its_panels_need(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::DockComposition, cx));
        let cx: &mut VisualTestContext = cx;

        cx.simulate_resize(size(px(1200.), px(800.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let story = cx
            .debug_bounds("story-dock-composition")
            .expect("the dock composition story should draw");
        let host = cx
            .debug_bounds("dock-composition-host")
            .expect("the dock composition host should draw");

        assert!(
            story.size.width > px(1000.),
            "the diagnostic should use a desktop frame, got {:?}",
            story.size.width
        );
        assert!(
            host.size.width > px(900.),
            "the four docked panels should receive the wide frame, got {:?}",
            host.size.width
        );
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

    /// Zooms the way the shell does: the theme carries the base type size and
    /// `Root` hands it to the window every frame.
    ///
    /// Two draws, because the surface notices the new rem while rendering and
    /// reacts afterwards — the first draw is where it sees the change, the
    /// second lays out what it re-measured. Nothing here asks a list to
    /// remeasure; that the surface does it unprompted is the property under
    /// test.
    fn zoom_to(cx: &mut VisualTestContext, font_size: f32) {
        cx.update(|window, cx| {
            Theme::global_mut(cx).font_size = px(font_size);
            window.set_rem_size(cx.theme().font_size);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn story_selector(story: StoryId) -> &'static str {
        Box::leak(format!("story-{}", story.slug()).into_boxed_str())
    }

    fn top_story(gallery: &Entity<Gallery>, cx: &mut VisualTestContext) -> Option<StoryId> {
        gallery.read_with(cx, |gallery: &Gallery, _| {
            StoryId::ALL
                .get(gallery.catalog_list.logical_scroll_top().item_ix)
                .copied()
        })
    }

    /// Absolute scroll offset in pixels, positive downward.
    fn catalog_offset(gallery: &Entity<Gallery>, cx: &mut VisualTestContext) -> f32 {
        gallery.read_with(cx, |gallery: &Gallery, _| {
            -gallery
                .catalog_list
                .scroll_px_offset_for_scrollbar()
                .y
                .as_f32()
        })
    }

    #[gpui::test]
    fn catalog_row_estimates_invalidate_when_rem_size_changes(cx: &mut TestAppContext) {
        // The design guides require anything cached from resolved layout to
        // key on rem size: the catalog's measured story heights must be
        // re-derived, not stale, after a base-font (zoom) change. The test
        // moves the rem and draws — it never calls `remeasure`, because
        // invalidating at runtime without being told is the whole finding.
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);
        zoom_to(cx, 16.);

        // Scroll into the feed so measured rows exist to go stale.
        gallery.update(cx, |gallery, cx| gallery.scroll_catalog_page(2., cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let anchor = top_story(&gallery, cx).expect("the feed should rest on a story");

        zoom_to(cx, 32.);

        gallery.read_with(cx, |gallery: &Gallery, _| {
            assert!(
                gallery.resolved_layout.matches(px(32.)),
                "the catalog must notice the new rem from its own render"
            );
        });
        assert_eq!(
            top_story(&gallery, cx),
            Some(anchor),
            "the story that was first on screen stays first across a zoom"
        );
        assert!(
            cx.debug_bounds(story_selector(anchor)).is_some(),
            "the anchored story is still drawn at 200% type"
        );

        zoom_to(cx, 16.);

        gallery.read_with(cx, |gallery: &Gallery, _| {
            assert!(
                gallery.resolved_layout.matches(px(16.)),
                "restoring the base font is another change to react to"
            );
        });
        assert_eq!(
            top_story(&gallery, cx),
            Some(anchor),
            "restoring the base font restores the reader's place"
        );
    }

    #[gpui::test]
    fn added_themes_render_the_composition_matrix_at_double_rem(cx: &mut TestAppContext) {
        let (gallery, cx) = all_stories(cx);
        let themes = [
            "blood-moon-cathedral",
            "forest-spirit",
            "mako-reactor",
            "moon-prism",
            "neon-pilgrim",
            "pocket-voltage",
            "silver-key-sky",
            "spice-horizon",
            "sunday-panel",
            "vector-grid",
        ];
        for slug in themes {
            let theme = GalleryTheme::from_slug(slug).expect("release theme must be registered");
            cx.update(|window, cx| super::apply_gallery_theme(theme, Some(window), cx));
            for (width, rem, reduced) in [(900., 16., false), (390., 32., true)] {
                cx.simulate_resize(size(px(width), px(844.)));
                cx.update(|_, cx| cx.set_reduce_motion(reduced));
                zoom_to(cx, rem);
                for story in [
                    StoryId::Chat,
                    StoryId::PromptBar,
                    StoryId::RecordsTable,
                    StoryId::ToolCalls,
                ] {
                    gallery.update(cx, |gallery, cx| {
                        gallery.prepare_performance_viewport(story, cx)
                    });
                    cx.run_until_parked();
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    let bounds = cx
                        .debug_bounds(story_selector(story))
                        .expect("themed story must render");
                    assert!(
                        bounds.size.width > px(0.) && bounds.size.width <= px(width),
                        "{slug} / {} at {rem}px must fit its host width",
                        story.slug()
                    );
                }
                gallery.update(cx, |gallery, cx| {
                    gallery.prepare_performance_viewport(StoryId::All, cx)
                });
                cx.run_until_parked();
                gallery.update(cx, |gallery, cx| gallery.scroll_catalog_edge(true, cx));
                cx.run_until_parked();
                cx.update(|window, cx| window.draw(cx).clear(cx));
                let last = *StoryId::ALL.last().expect("catalog");
                assert!(
                    cx.debug_bounds(story_selector(last)).is_some(),
                    "{slug} catalog tail at {rem}px"
                );
            }
        }
    }

    #[gpui::test]
    fn catalog_stories_stay_reachable_at_every_zoom(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);
        let first = *StoryId::ALL.first().expect("the catalog has stories");
        let last = *StoryId::ALL.last().expect("the catalog has stories");

        // 100%, 150%, 200% of the 16px base.
        for font_size in [16., 24., 32.] {
            zoom_to(cx, font_size);

            gallery.update(cx, |gallery, cx| gallery.scroll_catalog_edge(true, cx));
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(
                cx.debug_bounds(story_selector(last)).is_some(),
                "End must reach {} at {font_size}px type",
                last.slug()
            );

            gallery.update(cx, |gallery, cx| gallery.scroll_catalog_edge(false, cx));
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(
                cx.debug_bounds(story_selector(first)).is_some(),
                "Home must reach {} at {font_size}px type",
                first.slug()
            );
        }
    }

    #[gpui::test]
    fn catalog_paging_tracks_the_measured_viewport(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);

        let page_distance = |cx: &mut VisualTestContext, height: f32| -> f32 {
            cx.simulate_resize(size(px(900.), px(height)));
            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            gallery.update(cx, |gallery, cx| gallery.scroll_catalog_edge(false, cx));
            cx.update(|window, cx| window.draw(cx).clear(cx));

            let before = catalog_offset(&gallery, cx);
            gallery.update(cx, |gallery, cx| gallery.scroll_catalog_page(1., cx));
            catalog_offset(&gallery, cx) - before
        };

        let short = page_distance(cx, 420.);
        let tall = page_distance(cx, 900.);

        assert!(short > 0., "a page must move the feed, got {short}");
        assert!(
            tall > short + 100.,
            "a page is a fraction of what the reader can see, so a taller \
             viewport must page farther: {short} vs {tall}"
        );
    }

    /// Runs `frames` autoscroll ticks and reports how far the feed travelled.
    fn autoscroll_frames(
        gallery: &Entity<Gallery>,
        cx: &mut VisualTestContext,
        frames: usize,
    ) -> f32 {
        let before = catalog_offset(gallery, cx);
        for _ in 0..frames {
            cx.executor().advance_clock(AUTOSCROLL_FRAME_INTERVAL);
            cx.run_until_parked();
        }
        catalog_offset(gallery, cx) - before
    }

    #[gpui::test]
    fn starting_autoscroll_again_replaces_the_driver_instead_of_adding_one(
        cx: &mut TestAppContext,
    ) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);
        let anchor = point(px(100.), px(100.));
        let pointer = point(px(100.), px(260.));

        let travel = |cx: &mut VisualTestContext, starts: usize| -> f32 {
            gallery.update(cx, |gallery, cx| {
                gallery.scroll_catalog_edge(false, cx);
                for _ in 0..starts {
                    gallery.start_autoscroll(anchor, cx);
                }
                gallery.track_autoscroll_pointer(pointer, cx);
            });
            let travelled = autoscroll_frames(&gallery, cx, 10);
            gallery.update(cx, |gallery, cx| gallery.cancel_autoscroll(cx));
            travelled
        };

        let one_start = travel(cx, 1);
        assert!(
            one_start > 0.,
            "the stored driver should be scrolling the feed, moved {one_start}"
        );

        let two_starts = travel(cx, 2);
        assert!(
            (two_starts - one_start).abs() < 1.,
            "a second start must replace the driver, not add one that doubles \
             every frame: {one_start} then {two_starts}"
        );
    }

    #[gpui::test]
    fn cancelling_autoscroll_drops_the_driver_and_stops_the_frames(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);

        gallery.update(cx, |gallery, cx| {
            gallery.start_autoscroll(point(px(100.), px(100.)), cx);
            gallery.track_autoscroll_pointer(point(px(100.), px(260.)), cx);
        });
        assert!(autoscroll_frames(&gallery, cx, 5) > 0.);

        gallery.update(cx, |gallery, cx| gallery.cancel_autoscroll(cx));
        gallery.read_with(cx, |gallery: &Gallery, _| {
            assert!(!gallery.autoscroll_active());
            assert!(
                gallery.autoscroll_task.is_none(),
                "cancelling must drop the driver, not leave it waking every frame"
            );
        });

        assert_eq!(
            autoscroll_frames(&gallery, cx, 30),
            0.,
            "no frame may scroll the feed after the session is cancelled"
        );
    }

    #[gpui::test]
    fn autoscroll_at_the_anchor_stays_idle_until_the_pointer_moves(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);
        let anchor = point(px(100.), px(200.));

        gallery.update(cx, |gallery, cx| {
            gallery.start_autoscroll(anchor, cx);
            gallery.track_autoscroll_pointer(anchor, cx);
        });

        assert_eq!(
            autoscroll_frames(&gallery, cx, 20),
            0.,
            "a pointer resting on the anchor asks for no distance, so its \
             frames must move nothing"
        );
        gallery.read_with(cx, |gallery: &Gallery, _| {
            assert!(
                gallery.autoscroll_active() && gallery.autoscroll_task.is_some(),
                "idle is not over: the session is still held and still driven"
            );
        });

        gallery.update(cx, |gallery, cx| {
            gallery.track_autoscroll_pointer(point(px(100.), px(360.)), cx);
        });
        assert!(
            autoscroll_frames(&gallery, cx, 5) > 0.,
            "the same driver must move the feed once the pointer leaves the anchor"
        );
    }

    /// The feed travels at the library's speed *per second*, not per frame.
    ///
    /// A driver that assumes its requested interval is the elapsed time reads
    /// correctly only while the two agree — it drifts under any real frame
    /// jitter, and it silently halves the speed the day the interval changes.
    /// Here distance is checked against the clock, so the two cannot come
    /// apart.
    #[gpui::test]
    fn autoscroll_travels_at_the_library_speed_for_the_time_that_elapsed(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = all_stories(cx);
        let anchor = point(px(100.), px(100.));
        // Beyond the library's full-speed distance, so the expected rate is
        // exactly its maximum and needs no curve arithmetic here.
        let pointer = point(px(100.), px(100. + AUTOSCROLL_FULL_SPEED_DISTANCE_PX + 20.));

        gallery.update(cx, |gallery, cx| {
            gallery.start_autoscroll(anchor, cx);
            gallery.track_autoscroll_pointer(pointer, cx);
        });
        // One frame first: the driver takes its clock baseline on its first
        // tick, so measuring from there compares like with like.
        autoscroll_frames(&gallery, cx, 1);

        let started = cx.executor().now();
        let travelled = autoscroll_frames(&gallery, cx, 12);
        let elapsed = cx
            .executor()
            .now()
            .saturating_duration_since(started)
            .as_secs_f32();
        gallery.update(cx, |gallery, cx| gallery.cancel_autoscroll(cx));

        let expected = MAX_AUTOSCROLL_SPEED_PX_PER_SEC * elapsed;
        assert!(elapsed > 0., "the frames should have consumed clock time");
        assert!(
            (travelled - expected).abs() < 1.,
            "{elapsed}s at {MAX_AUTOSCROLL_SPEED_PX_PER_SEC}px/s is {expected}px, \
             but the feed moved {travelled}px"
        );
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
            cx.debug_bounds("records-story-end").is_some(),
            "records story should exercise records-story-end"
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
    fn constrained_records_table_story_keeps_its_end_reachable(cx: &mut TestAppContext) {
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
            .debug_bounds("records-story-end")
            .expect("the story end should remain rendered");
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
            .debug_bounds("records-story-end")
            .expect("the story end should remain rendered after scrolling");
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
            cx.debug_bounds("diff-story-end").is_some(),
            "diff story should exercise diff-story-end"
        );
    }

    #[gpui::test]
    fn constrained_diff_table_story_keeps_its_end_reachable(cx: &mut TestAppContext) {
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
            .debug_bounds("diff-story-end")
            .expect("the diff story end should remain rendered");
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
            .debug_bounds("diff-story-end")
            .expect("the diff story end should remain rendered after scrolling");
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
            cx.debug_bounds("filter-story-end").is_some(),
            "filter story should exercise filter-story-end"
        );
    }

    #[gpui::test]
    fn constrained_filter_table_story_keeps_its_end_reachable(cx: &mut TestAppContext) {
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
            .debug_bounds("filter-story-end")
            .expect("the filter story end should remain rendered");
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
            .debug_bounds("filter-story-end")
            .expect("the filter story end should remain rendered after scrolling");
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
            cx.debug_bounds("comparison-story-end").is_some(),
            "comparison story should exercise comparison-story-end"
        );
    }

    #[gpui::test]
    fn constrained_comparison_table_story_keeps_its_end_reachable(cx: &mut TestAppContext) {
        cx.update(super::init);
        let gallery_slot = Rc::new(RefCell::new(None));
        let result = gallery_slot.clone();
        let (_, cx) = cx.add_window_view(move |_, cx| {
            let gallery = cx.new(|cx| Gallery::new(StoryId::ComparisonTable, cx));
            *gallery_slot.borrow_mut() = Some(gallery.clone());
            GalleryTestRoot { gallery }
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(700.), px(360.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let viewport = cx
            .debug_bounds("comparison-table-story-scroll")
            .expect("the direct comparison story should expose its overflow region");
        let initial_end = cx
            .debug_bounds("comparison-story-end")
            .expect("the comparison story end should remain rendered");
        // Unlike the other three tables, this story's content no longer
        // fills the 400px surface, so there is nothing here to scroll to and
        // the post-scroll assertion below cannot fail. Saying so out loud is
        // what keeps that from going unnoticed: if the story grows again this
        // fails, and the "starts below the viewport" assertion the records,
        // diff, and filter tests carry should come back with it.
        assert!(
            initial_end.bottom() <= viewport.bottom(),
            "the comparison story overflows its surface again; restore the scroll assertion: end={initial_end:?}, viewport={viewport:?}"
        );

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
            .debug_bounds("comparison-story-end")
            .expect("the comparison story end should remain rendered after scrolling");
        assert!(
            end.bottom() <= viewport.bottom() && end.bottom() > viewport.top(),
            "{end:?} must fit in {viewport:?}"
        );
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

    /// The prompt-bar story fits the box it is given.
    ///
    /// It used to stack six composers and scroll, and this checked the end
    /// stayed reachable. One composer at a time fits without scrolling, so
    /// what is left to check is that it still does — the box carries
    /// headroom for the model menu to open into, and a story that outgrew
    /// it would be clipped rather than scrolled.
    #[gpui::test]
    fn the_constrained_prompt_bar_story_fits_its_frame(cx: &mut TestAppContext) {
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

        // The story shows one composer, named by its heading, and the
        // toolbar is how the reader reaches the rest. It used to stack all
        // six, which is what made the end of this story hard to reach in the
        // first place.
        assert!(
            cx.debug_bounds("prompt-bar-state-switcher").is_some(),
            "the story should offer its states rather than stack them"
        );
        assert!(
            cx.debug_bounds("prompt-bar-heading").is_some(),
            "the shown composer should say which state it is"
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
            "{end:?} must fit in {story:?} — the box is sized for one composer              plus the room its model menu needs to open into"
        );
    }

    /// The mobile suite's tap lands on the composer.
    ///
    /// GPUI's web backend exposes no accessibility tree, so `mobile.test.mjs`
    /// opens the keyboard by tapping a canvas coordinate. That coordinate is
    /// a measured fact about this story's layout, and a layout change moves
    /// it silently — the suite then taps blank canvas and fails twenty
    /// seconds later with a timeout that says nothing about the cause. This
    /// asserts the two agree, in the crate that owns the layout.
    #[gpui::test]
    fn the_mobile_suites_tap_lands_on_the_prompt_bar_composer(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::PromptBar, cx));
        let cx: &mut VisualTestContext = cx;
        cx.update(|_, cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.set_chrome(super::GalleryChrome::Embedded, cx)
            })
        });
        // The viewport the mobile suite drives the embed at.
        cx.simulate_resize(size(px(390.), px(300.)));
        cx.update(|window, cx| window.set_rem_size(cx.theme().font_size));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let composer = cx
            .debug_bounds("prompt-bar-composer")
            .expect("the story renders a composer");
        let (tap_x, tap_y) = super::MOBILE_COMPOSER_TAP;
        assert!(
            composer.left() <= px(tap_x)
                && px(tap_x) <= composer.right()
                && composer.top() <= px(tap_y)
                && px(tap_y) <= composer.bottom(),
            "mobile.test.mjs taps ({tap_x}, {tap_y}), which is outside the composer at {composer:?} \
             — move the tap in mobile.test.mjs and this constant together"
        );

        let (blur_x, blur_y) = super::MOBILE_COMPOSER_BLUR;
        assert!(
            !(composer.left() <= px(blur_x)
                && px(blur_x) <= composer.right()
                && composer.top() <= px(blur_y)
                && px(blur_y) <= composer.bottom()),
            "the blur tap ({blur_x}, {blur_y}) is inside the composer at {composer:?}, so it \
             would never close the keyboard it is checking closes"
        );
    }

    /// The switcher puts a different composer on screen.
    ///
    /// The states a component supports are worth nothing if the gallery can
    /// only reach the first, which is what stacking them below the fold
    /// amounted to.
    #[gpui::test]
    fn the_prompt_bar_story_switches_between_its_composers(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (_, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::PromptBar, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(700.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let offered = crate::story::PROMPT_BAR_STORY_VARIANTS;
        assert!(offered.len() > 1, "the story offers more than one state");
        assert_eq!(
            super::active_variant_index(),
            Some(0),
            "it starts on the first"
        );

        let switched = cx.update(|_, cx| super::set_active_variant(offered.len() - 1, cx));
        assert!(switched, "the switcher should take the change");
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            super::active_variant_index(),
            Some(offered.len() - 1),
            "the story should report the state it was put into"
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
        // Whichever story ends the catalog, not a named one: this is about
        // the last row being reached and static, and naming a story here
        // breaks the test every time the catalog gains one.
        let last = StoryId::ALL.last().expect("the catalog has stories");
        let last_selector: &'static str =
            Box::leak(format!("story-{}", last.slug()).into_boxed_str());
        assert!(
            cx.debug_bounds(last_selector).is_some(),
            "scrolling to the end should reach {last_selector}"
        );

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

    /// Runs the guided demo forward until it settles, or gives up.
    fn drive_guided(story: &Entity<GuidedDemoStory>, cx: &mut VisualTestContext) -> usize {
        for tick in 1..=600 {
            cx.executor().advance_clock(super::sim::TICK_INTERVAL);
            cx.run_until_parked();
            if story.read_with(cx, |story, _| story.stage) == GuidedStage::Settled {
                return tick;
            }
        }
        panic!("the guided demo never settled");
    }

    #[gpui::test]
    fn the_guided_demo_scripts_tools_then_reasoning_then_a_settled_reply(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (story, cx) = cx.add_window_view(GuidedDemoStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| window.draw(cx).clear(cx));

        story.read_with(cx, |story, _| {
            assert_eq!(story.stage, GuidedStage::Idle);
            assert!(story.reply.text().is_empty(), "nothing streams before Send");
        });

        cx.update(|window, cx| story.update(cx, |story, cx| story.start(window, cx)));
        story.read_with(cx, |story, _| {
            assert_eq!(
                story.stage,
                GuidedStage::Tools,
                "Send starts the tool group"
            );
        });

        // The stages must arrive in order, not merely end in the right place.
        let mut seen = vec![GuidedStage::Tools];
        for _ in 0..600 {
            cx.executor().advance_clock(super::sim::TICK_INTERVAL);
            cx.run_until_parked();
            let stage = story.read_with(cx, |story, _| story.stage);
            if seen.last() != Some(&stage) {
                seen.push(stage);
            }
            if stage == GuidedStage::Settled {
                break;
            }
        }
        assert_eq!(
            seen,
            vec![
                GuidedStage::Tools,
                GuidedStage::Reasoning,
                GuidedStage::Replying,
                GuidedStage::Settled,
            ],
            "the script must pass through every stage in order"
        );

        story.read_with(cx, |story, _| {
            assert!(!story.reasoning.is_streaming(), "reasoning must finish");
            assert!(!story.reply.is_streaming(), "the reply must finish");
            assert!(
                story.reply.text().contains("34 composed"),
                "the settled reply must be the full copy"
            );
        });
    }

    #[gpui::test]
    fn the_guided_demo_is_deterministic(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (first, cx) = cx.add_window_view(GuidedDemoStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| first.update(cx, |story, cx| story.start(window, cx)));
        let first_ticks = drive_guided(&first, cx);

        // Reset and run it again: a scripted demo must not drift.
        cx.update(|window, cx| first.update(cx, |story, cx| story.reset(window, cx)));
        cx.update(|window, cx| first.update(cx, |story, cx| story.start(window, cx)));
        let second_ticks = drive_guided(&first, cx);

        assert_eq!(
            first_ticks, second_ticks,
            "the same script must take the same number of ticks every run"
        );
    }

    #[gpui::test]
    fn resetting_the_guided_demo_returns_it_to_the_opening_state(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (story, cx) = cx.add_window_view(GuidedDemoStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| story.update(cx, |story, cx| story.start(window, cx)));
        drive_guided(&story, cx);

        cx.update(|window, cx| story.update(cx, |story, cx| story.reset(window, cx)));

        story.read_with(cx, |story, _| {
            assert_eq!(story.stage, GuidedStage::Idle);
            assert!(story.reply.text().is_empty());
            assert!(story.reasoning.text().is_empty());
            assert!(story.driver.is_none(), "reset must stop the driver");
        });

        // And it can run again from there.
        cx.update(|window, cx| story.update(cx, |story, cx| story.start(window, cx)));
        drive_guided(&story, cx);
        story.read_with(cx, |story, _| assert_eq!(story.stage, GuidedStage::Settled));
    }

    #[gpui::test]
    fn reduced_motion_lands_on_the_finished_answer_without_ticking(cx: &mut TestAppContext) {
        cx.update(super::init);
        cx.update(|cx| cx.set_reduce_motion(true));
        let (story, cx) = cx.add_window_view(GuidedDemoStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| window.draw(cx).clear(cx));

        cx.update(|window, cx| story.update(cx, |story, cx| story.start(window, cx)));

        // No clock advance at all: the answer is already whole.
        story.read_with(cx, |story, _| {
            assert_eq!(story.stage, GuidedStage::Settled);
            assert!(!story.reply.is_streaming());
            assert!(story.reply.text().contains("34 composed"));
            assert!(
                story.driver.is_none(),
                "reduced motion must not schedule a repeating task"
            );
        });
    }

    #[gpui::test]
    fn the_guided_demo_names_its_stage_for_assistive_technology(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (story, cx) = cx.add_window_view(GuidedDemoStory::new);
        let cx: &mut VisualTestContext = cx;
        for (stage, expected) in [
            (
                GuidedStage::Idle,
                "Ready — send the question to run the demo",
            ),
            (GuidedStage::Tools, "Running tools"),
            (GuidedStage::Reasoning, "Reasoning"),
            (GuidedStage::Replying, "Writing the reply"),
            (GuidedStage::Settled, "Answer complete"),
        ] {
            let (role, node) = cx.update(|_, cx| {
                story.update(cx, |story, cx| {
                    story.stage = stage;
                    cx.notify();
                    let element = story.status_element(cx).into_element();
                    let mut node = accesskit::Node::new(Role::Unknown);
                    element.write_a11y_info(&mut node);
                    (element.a11y_role(), node)
                })
            });
            cx.update(|window, cx| window.draw(cx).clear(cx));
            assert!(cx.debug_bounds("guided-demo-status").is_some());
            assert_eq!(role, Some(Role::Status));
            assert_eq!(node.label(), Some(expected));
        }
    }

    /// A theme that changes the base type size changes what a story measures.
    ///
    /// The measurement above takes its rem from the theme, which matters only
    /// if the theme can move it — and with the default theme it cannot, because
    /// 16px is also GPUI's own default. So the two calls that read the theme
    /// could be deleted and every height test would still pass. This is the one
    /// that would not: Graphite asks for 14px and Solstice for 17px, and a
    /// story laid out at those sizes is a different height.
    #[gpui::test]
    fn a_story_measures_differently_under_a_theme_that_resizes_the_type(cx: &mut TestAppContext) {
        cx.update(super::init);

        // Chat, because it is mostly text: a story of chips and icons would
        // barely move and would make a weak claim.
        let measure = |cx: &mut TestAppContext, slug: &str| {
            let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::Chat, cx));
            let cx: &mut VisualTestContext = cx;
            cx.update(|window, cx| {
                let preset = GalleryTheme::from_slug(slug).expect("bundled theme");
                super::apply_gallery_theme(preset, Some(window), cx);
            });
            cx.update(|_, cx| {
                gallery.update(cx, |gallery, cx| {
                    gallery.set_chrome(super::GalleryChrome::Embedded, cx)
                })
            });
            cx.simulate_resize(size(px(900.), px(2400.)));
            cx.update(|window, cx| {
                window.set_rem_size(cx.theme().font_size);
                window.draw(cx).clear(cx)
            });
            cx.debug_bounds("story-chat")
                .map(|bounds| f32::from(bounds.size.height).ceil() as u32)
                .expect("the chat story renders a measurable frame")
        };

        let small = measure(cx, "graphite");
        let base = measure(cx, "dark");
        let large = measure(cx, "solstice");

        assert!(
            small < base,
            "graphite asks for 14px type, so its chat must be shorter than {base}, not {small}"
        );
        assert!(
            large > base,
            "solstice asks for 17px type, so its chat must be taller than {base}, not {large}"
        );
    }

    /// Every still story is finished on the frame it first draws.
    ///
    /// The catalog's reserved heights are measured from a single draw, so a
    /// story that finishes assembling itself on some later frame publishes
    /// the height of its unfinished self and the website reserves too little.
    /// More to the point, a reader looking at a first paint is looking at
    /// the component — whatever is missing then is missing for them.
    ///
    /// Only the stories that do not simulate: an animating story is a
    /// different height on every frame by design, and says nothing about
    /// whether its first one was complete.
    #[gpui::test]
    fn a_still_story_is_finished_on_the_frame_it_first_draws(cx: &mut TestAppContext) {
        cx.update(super::init);
        let mut late = Vec::new();

        for story in StoryId::ALL {
            if super::story_needs_simulation(*story) {
                continue;
            }
            let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(*story, cx));
            let cx: &mut VisualTestContext = cx;
            cx.update(|_, cx| {
                gallery.update(cx, |gallery, cx| {
                    gallery.set_chrome(super::GalleryChrome::Embedded, cx)
                })
            });
            cx.simulate_resize(size(px(900.), px(2400.)));
            cx.update(|window, cx| window.set_rem_size(cx.theme().font_size));
            cx.update(|window, cx| window.draw(cx).clear(cx));

            let selector: &'static str =
                Box::leak(format!("story-{}", story.slug()).into_boxed_str());
            let first = cx
                .debug_bounds(selector)
                .expect("a measurable story frame")
                .size
                .height;

            cx.run_until_parked();
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let settled = cx
                .debug_bounds(selector)
                .expect("a measurable story frame")
                .size
                .height;

            let retired = gallery.downgrade();
            drop(gallery);
            cx.update(|window, _| window.remove_window());
            cx.run_until_parked();
            assert!(retired.upgrade().is_none(), "the story must be disposed");

            let first = f32::from(first).ceil() as u32;
            let settled = f32::from(settled).ceil() as u32;
            if first.abs_diff(settled) > 2 {
                late.push(format!(
                    "  {} => first draw {first}px, settles at {settled}px",
                    story.slug()
                ));
            }
        }

        assert!(
            late.is_empty(),
            "these stories are not finished on their first frame:
{}",
            late.join(
                "
"
            )
        );
    }

    /// The exhaustive sampler shared by catalog measurement and the transient-height
    /// regression. Never replace its maximum with the settled/final frame.
    fn measure_height_frames(
        cx: &mut VisualTestContext,
        selector: &'static str,
        ticks: usize,
    ) -> u32 {
        let mut maximum = 0;
        for tick in 0..=ticks {
            if tick > 0 {
                cx.executor().advance_clock(super::sim::TICK_INTERVAL);
                cx.run_until_parked();
                cx.update(|window, cx| window.draw(cx).clear(cx));
            }
            let height = cx
                .debug_bounds(selector)
                .expect("a measurable story frame")
                .size
                .height;
            maximum = maximum.max(f32::from(height).ceil() as u32);
        }
        maximum
    }

    #[gpui::test]
    fn height_sampler_keeps_a_transient_spike_not_just_the_settled_frame(cx: &mut TestAppContext) {
        use gpui::{InteractiveElement as _, ParentElement as _, Styled as _};
        struct Transient {
            height: gpui::Pixels,
            _task: gpui::Task<()>,
        }
        impl Render for Transient {
            fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
                gpui::div()
                    .debug_selector(|| "transient-height".into())
                    .w(px(100.))
                    .h(self.height)
                    .child("A transient expanded state")
            }
        }
        let (probe, cx) = cx.add_window_view(|_, cx: &mut Context<Transient>| {
            let task = cx.spawn(async move |this, cx| {
                for height in [240., 20.] {
                    cx.background_executor()
                        .timer(super::sim::TICK_INTERVAL)
                        .await;
                    this.update(cx, |this, cx| {
                        this.height = px(height);
                        cx.notify();
                    })
                    .expect("live probe");
                }
            });
            Transient {
                height: px(20.),
                _task: task,
            }
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(measure_height_frames(cx, "transient-height", 4), 240);
        assert_eq!(probe.read_with(cx, |probe, _| probe.height), px(20.));
    }

    /// The declared heights must be what the stories actually measure.
    ///
    /// The website centres each story in a frame sized from this number, so a
    /// stale value shows as dead space or a clipped demo. When a story changes
    /// shape this test fails and prints the number to put in `story.rs`.
    #[gpui::test]
    fn story_heights_match_what_the_stories_measure(cx: &mut TestAppContext) {
        cx.update(super::init);
        let mut wrong = Vec::new();

        for story in StoryId::ALL {
            let started = std::time::Instant::now();
            let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(*story, cx));
            let cx: &mut VisualTestContext = cx;
            cx.update(|_, cx| {
                gallery.update(cx, |gallery, cx| {
                    gallery.set_chrome(super::GalleryChrome::Embedded, cx)
                })
            });
            // Tall enough that nothing is clipped by the viewport itself.
            cx.simulate_resize(size(px(900.), px(2400.)));
            // The real window wraps everything in gpui-component's Root, whose
            // render sets the rem size from the theme. These tests build a
            // window without it, so without this every story is measured at
            // GPUI's default rem rather than the one it will be drawn at, and
            // a theme that changes the base size could not move anything.
            cx.update(|window, cx| window.set_rem_size(cx.theme().font_size));
            cx.update(|window, cx| window.draw(cx).clear(cx));

            let selector: &'static str =
                Box::leak(format!("story-{}", story.slug()).into_boxed_str());
            // Keep every intermediate frame, including rest/restart and wrap thresholds.
            let ticks = if super::story_needs_simulation(*story) {
                super::MEASURE_TICKS
            } else {
                0
            };
            let measured = measure_height_frames(cx, selector, ticks);
            let declared = story
                .meta()
                .expect("component stories carry metadata")
                .height;

            // Window-owned roots otherwise keep every earlier simulation alive while
            // the shared executor advances for later stories.
            let retired = gallery.downgrade();
            drop(gallery);
            cx.update(|window, _| window.remove_window());
            cx.run_until_parked();
            assert!(
                retired.upgrade().is_none(),
                "the previous story and its owned task must be disposed"
            );
            println!(
                "height-sample {}: max={measured}px draws={} elapsed_ms={}",
                story.slug(),
                ticks + 1,
                started.elapsed().as_millis()
            );

            // A pixel of rounding drift is not worth a failing build.
            if measured.abs_diff(declared) > 2 {
                wrong.push(format!(
                    "  {} => declared {declared}, measures {measured}",
                    story.slug()
                ));
            }
        }

        assert!(
            wrong.is_empty(),
            "these story heights are stale; update StoryMeta::height in story.rs:\n{}",
            wrong.join("\n")
        );
    }

    /// A catalog row never gives back height it has already taken.
    ///
    /// Stories that animate change size as they run, and in the stacked
    /// catalog a story that collapses drags every row below it up the
    /// screen — the jumping the 0.5.0 review reported. A row keeps the
    /// tallest it has been, so the reader's place stops moving under them.
    /// Deliberately the row's own measurement rather than the catalog's
    /// declared heights: those exist for the website, and a component
    /// library's own gallery should not be laid out by the website's needs.
    #[gpui::test]
    fn a_catalog_row_keeps_the_tallest_it_has_been(cx: &mut TestAppContext) {
        cx.update(super::init);
        let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::All, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(1200.)));
        cx.update(|window, cx| {
            window.set_rem_size(cx.theme().font_size);
            window.draw(cx).clear(cx)
        });

        let floors = gallery.read_with(cx, |gallery, _| gallery.story_floors.clone());
        let observed = |story: StoryId| {
            floors
                .borrow()
                .rows
                .get(&story)
                .copied()
                .unwrap_or_default()
        };

        // Whatever the first visible row measured, it is now that row's
        // floor — and the floor only ever rises.
        let first = StoryId::ALL[0];
        let recorded = observed(first);
        assert!(
            recorded > 0.,
            "a drawn row must record what it measured, got {recorded}"
        );

        floors.borrow_mut().rows.insert(first, recorded + 400.);
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(
            observed(first),
            recorded + 400.,
            "a row that has been taller must not shrink back to its content"
        );

        // A floor earned while the window was narrow is not a floor once it
        // is wide: the story only needed that height because its text
        // wrapped more.
        cx.simulate_resize(size(px(700.), px(1200.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(
            observed(first) < recorded + 400.,
            "a change of width must discard heights measured at the old one"
        );
    }

    /// The hero's declared height must be what the settled demo measures.
    ///
    /// The loop above never sees it: the hero is outside `StoryId::ALL` and
    /// carries no `StoryMeta`. It also needs a state the other stories do not
    /// have — idle is a composer alone, and the site sizes its frame for the
    /// finished answer. Reduced motion lands there in one frame.
    #[gpui::test]
    fn the_hero_height_matches_what_the_settled_demo_measures(cx: &mut TestAppContext) {
        cx.update(super::init);
        cx.update(|cx| cx.set_reduce_motion(true));

        let (gallery, cx) = cx.add_window_view(|_, cx| Gallery::new(StoryId::GuidedDemo, cx));
        let cx: &mut VisualTestContext = cx;
        cx.update(|_, cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.set_chrome(super::GalleryChrome::Embedded, cx)
            })
        });
        // Tall enough that nothing is clipped by the viewport itself.
        cx.simulate_resize(size(px(900.), px(2400.)));
        cx.update(|window, cx| window.set_rem_size(cx.theme().font_size));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let send = cx
            .debug_bounds("prompt-bar-send-control")
            .expect("the hero opens on a prefilled composer")
            .center();
        cx.simulate_mouse_move(send, None, Modifiers::default());
        cx.simulate_mouse_down(send, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(send, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert!(
            cx.debug_bounds("streaming-follow-up-components").is_some(),
            "the follow-ups appear only once the answer has settled"
        );
        let measured = cx
            .debug_bounds("story-guided-demo")
            .map(|bounds| f32::from(bounds.size.height).ceil() as u32)
            .expect("the hero did not render a measurable frame");

        // A pixel of rounding drift is not worth a failing build.
        assert!(
            measured.abs_diff(crate::story::HERO_HEIGHT) <= 2,
            "the hero height is stale; set HERO_HEIGHT in story.rs to {measured}"
        );
    }
}

// ---------------------------------------------------------------------------
// Guided demo — the website's hero.
//
// A scripted answer to one question, composed by hand from shipped components
// because a `ChatMessage` cannot yet carry reasoning or tool-call parts (that
// is item 23 in the tier-3 backlog). Everything here is gallery-side: the
// library gains nothing for the hero's sake.
// ---------------------------------------------------------------------------

/// The question the composer opens with.
const GUIDED_QUESTION: &str = "What is gpui-ai?";

/// Reasoning shown while the answer is composed.
///
/// H-04 copy — reviewed with Oscar. Keep it specific and true: every claim
/// here is checkable against the repository.
const GUIDED_REASONING: &str = "The question is about the library itself, not a \
supplier. I should describe what it gives a Rust developer rather than list \
types.\n\nWorth naming what makes it different: these are composed components \
above gpui-component, not a fork of it, and every value resolves through the \
active theme.\n\nI should be honest that it is pre-1.0 and installed from git.";

/// The assistant's reply. H-04 copy — reviewed with Oscar.
const GUIDED_REPLY: &str = "**gpui-ai** is the interface layer AI applications \
keep rebuilding, for [GPUI](https://gpui.rs) — streamed answers, reasoning \
traces, tool calls, approval gates, and chat, as **34 composed \
components**.\n\nThree things shape it:\n\n\
- Components sit *above* [gpui-component](https://github.com/longbridge/gpui-component) rather than forking it\n\
- Every colour, radius, and type style resolves through the active theme, so a \
theme is a JSON file rather than a patch\n\
- Your application owns the data and the async work; components report typed \
intent by stable ID\n\n\
It is pre-1.0 and installs from git — this page is running the same Rust \
compiled to WebAssembly.";

/// Ticks the tool group runs before the first call completes.
const GUIDED_FIRST_TOOL_TICKS: usize = 6;
/// Ticks before the second call completes and reasoning begins.
const GUIDED_TOOLS_TICKS: usize = 14;
/// Characters revealed per tick while reasoning and replying stream.
const GUIDED_CHARS_PER_TICK: usize = 6;

/// Where the scripted demo has got to.
///
/// The stages are ordered, so a render can ask "have we reached Reasoning yet"
/// with a comparison rather than a match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum GuidedStage {
    /// Nothing sent; the composer is waiting.
    Idle,
    /// The tool group is running.
    Tools,
    /// Reasoning is streaming.
    Reasoning,
    /// The reply is streaming.
    Replying,
    /// The reply finished and follow-ups are offered.
    Settled,
}

impl GuidedStage {
    /// A name for the status region, so the stage is readable without sight.
    fn label(self) -> &'static str {
        match self {
            Self::Idle => "Ready — send the question to run the demo",
            Self::Tools => "Running tools",
            Self::Reasoning => "Reasoning",
            Self::Replying => "Writing the reply",
            Self::Settled => "Answer complete",
        }
    }
}

/// The site hero: a prompt bar whose Send runs one deterministic script.
struct GuidedDemoStory {
    prompt: Entity<PromptBar>,
    stage: GuidedStage,
    ticks: usize,
    reasoning: StreamedContent,
    reply: StreamedContent,
    reasoning_pos: usize,
    reply_pos: usize,
    driver: Option<Task<()>>,
    _subscription: Subscription,
}

impl GuidedDemoStory {
    fn status_element(&self, cx: &App) -> gpui::Stateful<gpui::Div> {
        div()
            .id("guided-demo-status")
            .debug_selector(|| "guided-demo-status".into())
            .role(Role::Status)
            .aria_label(self.stage.label())
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child(self.stage.label())
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prompt = cx.new(|cx| PromptBar::new("guided-demo-prompt", window, cx));
        prompt.update(cx, |prompt, cx| {
            prompt.set_draft(GUIDED_QUESTION, window, cx);
        });
        let subscription = cx.subscribe_in(
            &prompt,
            window,
            |this, _, event: &PromptBarEvent, window, cx| {
                if matches!(event, PromptBarEvent::Submit { .. }) {
                    this.start(window, cx);
                }
            },
        );

        Self {
            prompt,
            stage: GuidedStage::Idle,
            ticks: 0,
            reasoning: StreamedContent::new(),
            reply: StreamedContent::new(),
            reasoning_pos: 0,
            reply_pos: 0,
            driver: None,
            _subscription: subscription,
        }
    }

    /// Runs the script. Under reduced motion it lands on the finished answer
    /// immediately: the demo is the content, not the animation.
    fn start(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.stage != GuidedStage::Idle {
            return;
        }

        if cx.reduce_motion() {
            self.reasoning = StreamedContent::done(GUIDED_REASONING);
            self.reply = StreamedContent::done(GUIDED_REPLY);
            self.reasoning_pos = GUIDED_REASONING.len();
            self.reply_pos = GUIDED_REPLY.len();
            self.stage = GuidedStage::Settled;
            self.emit_settled(cx);
            cx.notify();
            return;
        }

        self.stage = GuidedStage::Tools;
        self.ticks = 0;
        self.driver = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(sim::TICK_INTERVAL).await;
                let running = this.update(cx, |this, cx| this.advance(cx));
                match running {
                    Ok(true) => {}
                    // Finished, or the story went away.
                    _ => break,
                }
            }
        }));
        cx.notify();
    }

    /// Advances one tick. Returns whether the script is still running, so the
    /// driver stops itself rather than ticking a settled demo forever.
    fn advance(&mut self, cx: &mut Context<Self>) -> bool {
        match self.stage {
            GuidedStage::Idle | GuidedStage::Settled => return false,
            GuidedStage::Tools => {
                self.ticks += 1;
                if self.ticks >= GUIDED_TOOLS_TICKS {
                    self.stage = GuidedStage::Reasoning;
                }
            }
            GuidedStage::Reasoning => {
                if advance_stream(
                    GUIDED_REASONING,
                    &mut self.reasoning_pos,
                    &mut self.reasoning,
                ) {
                    self.stage = GuidedStage::Replying;
                }
            }
            GuidedStage::Replying => {
                if advance_stream(GUIDED_REPLY, &mut self.reply_pos, &mut self.reply) {
                    self.stage = GuidedStage::Settled;
                    self.emit_settled(cx);
                }
            }
        }
        cx.notify();
        self.stage != GuidedStage::Settled
    }

    fn emit_settled(&self, cx: &mut Context<Self>) {
        cues::emit(
            cx,
            Cue::ResponseSettled {
                message_id: "guided-demo-reply".into(),
                succeeded: true,
            },
        );
    }

    /// Returns the demo to its opening state so a visitor can run it again.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.driver = None;
        self.stage = GuidedStage::Idle;
        self.ticks = 0;
        self.reasoning = StreamedContent::new();
        self.reply = StreamedContent::new();
        self.reasoning_pos = 0;
        self.reply_pos = 0;
        self.prompt.update(cx, |prompt, cx| {
            prompt.set_draft(GUIDED_QUESTION, window, cx);
        });
        cx.notify();
    }

    /// The two calls the script runs, at the completeness this tick implies.
    fn tool_calls(&self) -> [Progressive<ToolInvocation>; 2] {
        let catalog = ToolInvocation::new("read-catalog", "read_file")
            .summary("site/generated/catalog.json")
            .input("{\n  \"path\": \"site/generated/catalog.json\"\n}");
        let count = ToolInvocation::new("count-stories", "count_stories")
            .summary("crates/gallery/src/story.rs")
            .input("{\n  \"registry\": \"StoryId::ALL\"\n}");

        let past_first = self.stage > GuidedStage::Tools || self.ticks >= GUIDED_FIRST_TOOL_TICKS;
        let past_second = self.stage > GuidedStage::Tools;

        [
            if past_first {
                Progressive::complete(
                    catalog
                        .output("Read **34 components** across 8 categories.")
                        .elapsed(Duration::from_millis(180)),
                )
            } else {
                Progressive::running(catalog)
            },
            if past_second {
                Progressive::complete(
                    count
                        .output("`StoryId::ALL` lists **34** stable stories.")
                        .elapsed(Duration::from_millis(240)),
                )
            } else if past_first {
                Progressive::running(count)
            } else {
                Progressive::pending(count)
            },
        ]
    }
}

/// Reveals the next few characters of `target`; returns whether it finished.
fn advance_stream(target: &str, pos: &mut usize, content: &mut StreamedContent) -> bool {
    if *pos >= target.len() {
        return true;
    }
    let mut end = *pos;
    for _ in 0..GUIDED_CHARS_PER_TICK {
        match target[end..].chars().next() {
            Some(character) => end += character.len_utf8(),
            None => break,
        }
    }
    content.append(&target[*pos..end]);
    *pos = end;
    if *pos >= target.len() {
        content.finish();
        true
    } else {
        false
    }
}

impl Render for GuidedDemoStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let started = self.stage > GuidedStage::Idle;
        let [first_call, second_call] = self.tool_calls();

        let reasoning_trace = ThinkingTrace::new().prose(self.reasoning.text().to_owned());
        let reasoning = if self.stage > GuidedStage::Reasoning {
            Progressive::complete(reasoning_trace.thought_for(Duration::from_secs(2)))
        } else {
            Progressive::running(reasoning_trace)
        };

        v_flex()
            .id("guided-demo")
            .debug_selector(|| "guided-demo".into())
            .w_full()
            .gap(tokens.spacing.lg)
            .when(started, |this| {
                this.child(
                    v_flex()
                        .gap(tokens.spacing.md)
                        .child(
                            div()
                                .id("guided-demo-question")
                                .role(Role::Paragraph)
                                .aria_label(GUIDED_QUESTION)
                                .self_end()
                                .max_w(px(420.))
                                .px(tokens.spacing.md)
                                .py(tokens.spacing.sm)
                                .rounded(cx.theme().radius)
                                .bg(cx.theme().accent)
                                .text_color(cx.theme().accent_foreground)
                                .child(GUIDED_QUESTION),
                        )
                        .child(
                            ToolGroup::new("guided-demo-tools")
                                .title("Looking at the repository")
                                .count(2)
                                .active(self.stage == GuidedStage::Tools)
                                .open(true)
                                .child(ToolCall::new(&first_call).into_any_element())
                                .child(ToolCall::new(&second_call).into_any_element()),
                        )
                        .when(self.stage >= GuidedStage::Reasoning, |this| {
                            this.child(
                                Thinking::new("guided-demo-reasoning", &reasoning)
                                    .open(self.stage == GuidedStage::Reasoning),
                            )
                        })
                        .when(self.stage >= GuidedStage::Replying, |this| {
                            this.child(
                                StreamingText::new("guided-demo-reply", &self.reply).follow_ups(
                                    if self.stage == GuidedStage::Settled {
                                        vec![
                                            FollowUp::new("components", "Browse all 34 components"),
                                            FollowUp::new("theming", "How does theming work?"),
                                            FollowUp::new("install", "Add it to my app"),
                                        ]
                                    } else {
                                        Vec::new()
                                    },
                                ),
                            )
                        }),
                )
            })
            .child(self.prompt.clone())
            .child(
                h_flex()
                    .gap(tokens.spacing.sm)
                    .child(
                        Button::new("guided-demo-reset")
                            .outline()
                            .text_label("Reset")
                            .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
                    )
                    .child(self.status_element(cx)),
            )
    }
}
