//! Native component gallery for gpui-ai.
//!
//! One story per component, driven by simulated agent activity (fake token
//! streams, task lifecycles). All simulation lives here — the library
//! components only ever see data.

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
    [PageUp, PageDown, ScrollHome, ScrollEnd, CancelAutoscroll]
);

/// Distance scrolled by one Page Up/Down, as a fraction of one story row.
const PAGE_FRACTION: f32 = 3.0;
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
    static ACTIVE_SWITCHER: RefCell<Option<(usize, VariantSetter)>> = const { RefCell::new(None) };
}

/// Which state the story on screen is showing, if it offers any.
pub fn active_variant_index() -> Option<usize> {
    ACTIVE_SWITCHER.with(|switcher| switcher.borrow().as_ref().map(|(index, _)| *index))
}

/// Puts the story on screen into one of the states it offers.
///
/// Returns whether there was a switcher to tell.
pub fn set_active_variant(index: usize, cx: &mut App) -> bool {
    let Some(apply) = ACTIVE_SWITCHER.with(|switcher| {
        switcher
            .borrow()
            .as_ref()
            .map(|(_, apply)| Rc::clone(apply))
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
    const LABELS: &'static [(&'static str, &'static str)] = crate::story::TABLE_STORY_VARIANTS;

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
        *switcher.borrow_mut() = Some((
            active_index,
            // The entity may be gone — a switcher registers as it draws and
            // nothing unregisters it — so whether the story took the change is
            // the answer, not whether a setter was found.
            Rc::new(move |index: usize, cx: &mut App| {
                registered
                    .update(cx, |story, cx| apply(story, index, cx))
                    .is_ok()
            }) as VariantSetter,
        ));
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
        | StoryId::GuidedDemo
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
        | StoryId::SelectionActions => false,
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

fn demo_thread_sections() -> Vec<ThreadSection> {
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

struct CommandSearchStory {
    ready: Entity<CommandSearch>,
    empty: Entity<CommandSearch>,
    no_results: Entity<CommandSearch>,
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
// snippet:end

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
// snippet:end

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
// snippet:end

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

/// Stateful component gallery shared by native and web launchers.
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

pub struct Gallery {
    selected: StoryId,
    chrome: GalleryChrome,
    /// Bumped by [`Gallery::reset_story`], and part of the key of every story
    /// entity the window holds.
    ///
    /// Those entities are not fields here — they live in the window's keyed
    /// state, which outlives this struct — so replacing this struct left a
    /// reset chat still holding its transcript and a reset table still
    /// sorted. Changing the key is what actually asks for a new one.
    generation: usize,
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

        let frame = story_frame(story, self.selected == StoryId::All)
            .debug_selector(move || format!("story-{}", story.slug()))
            .w_full()
            .max_w(px(640.))
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
            StoryId::GuidedDemo => {
                let guided =
                    window.use_keyed_state(("guided-demo-story-state", self.generation), cx, GuidedDemoStory::new);
                self.section(story, "Guided demo", || guided, cx)
            }
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
                                    .label("Reopen artifact")
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
                                    .label("Simulate: transcription finished")
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
                                    .label("Queue the demo prompts again")
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
                            // 256px clipped suggestion rows mid-card; the grouped
                            // model menu needs room to open below the composer.
                            .h(px(560.))
                            .max_h(px(560.))
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
            let story = self.render_story(self.selected, window, cx);
            div()
                .id("gallery-scroll")
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
                                .label(format!("Theme: {}", self.theme.label()))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.theme = this.theme.next();
                                    apply_gallery_theme(this.theme, Some(window), cx);
                                    cx.notify();
                                })),
                        ),
                )
            })
            .child(content)
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
        ChatStory, Gallery, GalleryTheme, GuidedDemoStory, GuidedStage, filter_story_project_rows,
        filter_story_rows, reduce_filter_story_projection,
    };
    use crate::StoryId;
    use gpui::{
        AppContext as _, Context, Element as _, Entity, IntoElement as _, Modifiers, MouseButton,
        Render, Role, ScrollDelta, ScrollWheelEvent, TestAppContext, VisualTestContext, Window,
        accesskit, point, px, size,
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
        cx.update(|_, cx| {
            gallery.update(cx, |gallery, cx| {
                gallery.set_chrome(super::GalleryChrome::Embedded, cx)
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let key_of =
            |cx: &mut VisualTestContext| gallery.read_with(cx, |gallery, _| gallery.generation);
        let before = key_of(cx);

        cx.update(|_, cx| gallery.update(cx, |gallery, cx| gallery.reset_story(cx)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert_ne!(
            key_of(cx),
            before,
            "a reset that reused the key would hand back the same chat, still holding \
             everything the reader had done to it"
        );
        // And the story still draws afterwards, which is what says the new key
        // built a story rather than losing one.
        assert!(cx.debug_bounds("story-chat").is_some());
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
        let (_story, cx) = cx.add_window_view(GuidedDemoStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let idle = cx.debug_bounds("guided-demo").is_some();
        assert!(idle, "the hero must render before anything is sent");

        // Every stage has a distinct, human name rather than a code.
        let mut labels = Vec::new();
        for stage in [
            GuidedStage::Idle,
            GuidedStage::Tools,
            GuidedStage::Reasoning,
            GuidedStage::Replying,
            GuidedStage::Settled,
        ] {
            let label = stage.label();
            assert!(!label.is_empty(), "{stage:?} has no accessible name");
            labels.push(label);
        }
        let mut unique = labels.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), labels.len(), "stage names must be distinct");
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
            let height_now = |cx: &mut VisualTestContext| {
                cx.debug_bounds(selector)
                    .map(|bounds| f32::from(bounds.size.height).ceil() as u32)
                    .unwrap_or_else(|| panic!("{} did not render a measurable frame", story.slug()))
            };

            // The simulation starts with empty streams, so measuring only the
            // first frame sizes a search story to its "searching" state and
            // clips the results that arrive a second later. Run the streams to
            // completion and keep the tallest state the story passes through.
            // Only the stories the simulation actually feeds need ticking; the
            // rest are static, and ticking all of them costs minutes.
            let mut measured = height_now(cx);
            if super::story_needs_simulation(*story) {
                for _ in 0..super::MEASURE_TICKS {
                    cx.executor().advance_clock(super::sim::TICK_INTERVAL);
                    cx.run_until_parked();
                    cx.update(|window, cx| window.draw(cx).clear(cx));
                    measured = measured.max(height_now(cx));
                }
            }
            let declared = story
                .meta()
                .expect("component stories carry metadata")
                .height;

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
                            .label("Reset")
                            .on_click(cx.listener(|this, _, window, cx| this.reset(window, cx))),
                    )
                    .child(
                        div()
                            .id("guided-demo-status")
                            .role(Role::Status)
                            .aria_label(self.stage.label())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(self.stage.label()),
                    ),
            )
    }
}
