//! Controlled, virtualized chat transcript composed from gpui-ai primitives.

use crate::{
    attachment::{Attachment, AttachmentEvent, AttachmentStrip},
    control::{outlined_control, outlined_control_with_label},
    cues::{self, Cue},
    motion::reveal,
    orbs::Orbs,
    prompt_bar::{PromptBar, PromptBarEvent},
    resolved_layout::ResolvedLayoutKey,
    scrolling::list_scroll_mask,
    stream::{ProgressState, StreamedContent},
    streaming_text::{CitationRef, FollowUp, StreamingText, StreamingTextEvent},
    suggestions::{Suggestion, Suggestions, SuggestionsEvent},
    surface::{icon_button, meta},
    theme::SemanticStyledExt as _,
};
use gpui::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, ElementId, Entity, EventEmitter,
    FocusHandle, Focusable as _, FollowMode, FontWeight, InteractiveElement as _, IntoElement as _,
    ListAlignment, ListOffset, ListState, ParentElement as _, Pixels, Render, Role, SharedString,
    Stateful, StatefulInteractiveElement as _, Styled as _, Subscription, Task, Window, div, list,
    prelude::FluentBuilder as _, px, relative,
};
use gpui_base::Button;
use gpui_component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button as LabeledButton, ButtonVariants as _},
    h_flex,
    input::{Escape, InputEvent, Textarea, TextareaState},
    scroll::ScrollableElement as _,
    text::TextView,
    v_flex,
};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
    time::Duration,
};

/// Where a message sits among its sibling versions (regenerations or edits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BranchPosition {
    index: usize,
    count: usize,
}

impl BranchPosition {
    /// Creates a position; `index` is zero-based and clamped below `count`
    /// (which is at least one).
    pub fn new(index: usize, count: usize) -> Self {
        let count = count.max(1);
        Self {
            index: index.min(count - 1),
            count,
        }
    }

    /// Zero-based index of the shown version.
    pub fn index(self) -> usize {
        self.index
    }

    /// Number of versions.
    pub fn count(self) -> usize {
        self.count
    }

    /// The accessible name of the version switcher.
    pub fn label(self) -> String {
        format!("Version {} of {}", self.index + 1, self.count)
    }
}

/// The in-place editor for one message, alive only while editing.
struct EditSession {
    message_id: SharedString,
    editor: Entity<TextareaState>,
    _subscription: Subscription,
}

/// How long the copy action shows its "copied" confirmation.
const COPIED_FEEDBACK: Duration = Duration::from_secs(2);

/// The semantic speaker or producer of a [`ChatMessage`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChatRole {
    /// A message authored by the person using the application.
    User,
    /// A message authored by the assistant.
    Assistant,
    /// Application or conversation-level guidance.
    System,
    /// Output produced by an application-owned tool invocation.
    Tool,
}

impl ChatRole {
    fn label(self) -> &'static str {
        match self {
            Self::User => "User",
            Self::Assistant => "Assistant",
            Self::System => "System",
            Self::Tool => "Tool",
        }
    }
}

/// One application-owned message snapshot rendered by [`Chat`].
///
/// IDs must be unique within a controlled snapshot. The message keeps all
/// progressive content and interaction metadata immutable; applications
/// replace the enclosing `Arc<[ChatMessage]>` as work advances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    id: SharedString,
    role: ChatRole,
    author: Option<SharedString>,
    content: StreamedContent,
    citations: Vec<CitationRef>,
    sources: Vec<SharedString>,
    follow_ups: Vec<FollowUp>,
    attachments: Vec<Attachment>,
    branch: Option<BranchPosition>,
    retryable: bool,
    appearance: ChatMessageAppearance,
    actions: Option<MessageActions>,
}

/// Application-controlled presentation for one message.
///
/// Chat deliberately does not prescribe how a message looks: consumers decide
/// alignment and bubble treatment per message (typically by role), so a
/// right-aligned user / left-aligned agent layout is one configuration away.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChatMessageAppearance {
    alignment: MessageAlignment,
    bubble: MessageBubble,
}

impl Default for ChatMessageAppearance {
    fn default() -> Self {
        Self {
            alignment: MessageAlignment::Leading,
            bubble: MessageBubble::Bordered,
        }
    }
}

/// Horizontal placement of a message within the transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageAlignment {
    /// Hug the transcript's leading edge (default).
    Leading,
    /// Push toward the transcript's trailing edge.
    Trailing,
}

/// Surface treatment of the message frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageBubble {
    /// Card with border and surface background (default).
    Bordered,
    /// Filled tint with no border (chat-bubble look).
    Filled,
    /// No frame at all; content sits directly on the transcript.
    Plain,
}

impl ChatMessageAppearance {
    /// Creates an appearance with explicit alignment and bubble treatment.
    pub fn new(alignment: MessageAlignment, bubble: MessageBubble) -> Self {
        Self { alignment, bubble }
    }

    /// Horizontal placement of this message.
    pub fn alignment(&self) -> MessageAlignment {
        self.alignment
    }

    /// Surface treatment of this message.
    pub fn bubble(&self) -> MessageBubble {
        self.bubble
    }
}

/// The per-message actions a transcript row offers.
///
/// Actions are quiet at rest and appear on hover or keyboard focus; the last
/// settled message keeps them visible so the most likely next action is one
/// click away. Every action is reported as a typed [`ChatEvent`] — the
/// application performs the work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MessageActions {
    copy: bool,
    regenerate: bool,
    edit: bool,
    feedback: bool,
}

impl MessageActions {
    /// No actions.
    pub fn none() -> Self {
        Self::default()
    }

    /// The conventional set for a role: assistant replies can be copied,
    /// regenerated, and rated; user prompts can be copied and edited; tool
    /// output can be copied; system notes offer nothing.
    pub fn for_role(role: ChatRole) -> Self {
        match role {
            ChatRole::Assistant => Self::none().copy(true).regenerate(true).feedback(true),
            ChatRole::User => Self::none().copy(true).edit(true),
            ChatRole::Tool => Self::none().copy(true),
            ChatRole::System => Self::none(),
        }
    }

    /// Offers copying the message text to the clipboard.
    pub fn copy(mut self, enabled: bool) -> Self {
        self.copy = enabled;
        self
    }

    /// Offers requesting a new response.
    pub fn regenerate(mut self, enabled: bool) -> Self {
        self.regenerate = enabled;
        self
    }

    /// Offers editing (and resending) the message.
    pub fn edit(mut self, enabled: bool) -> Self {
        self.edit = enabled;
        self
    }

    /// Offers helpful / not-helpful feedback.
    pub fn feedback(mut self, enabled: bool) -> Self {
        self.feedback = enabled;
        self
    }

    /// Whether any action is offered.
    pub fn is_empty(&self) -> bool {
        !(self.copy || self.regenerate || self.edit || self.feedback)
    }
}

/// What an empty conversation shows: a welcome, optional guidance, and
/// starter suggestions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatWelcome {
    title: SharedString,
    description: Option<SharedString>,
    suggestions: Vec<Suggestion>,
}

impl ChatWelcome {
    /// Creates a welcome with a headline.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
            description: None,
            suggestions: Vec::new(),
        }
    }

    /// Adds supporting guidance under the headline.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds starter prompts, reported through [`ChatEvent::SuggestionSelected`].
    pub fn suggestions(mut self, suggestions: impl IntoIterator<Item = Suggestion>) -> Self {
        self.suggestions = suggestions.into_iter().collect();
        self
    }
}

impl ChatMessage {
    /// Creates a message with stable application identity and progressive text.
    pub fn new(id: impl Into<SharedString>, role: ChatRole, content: StreamedContent) -> Self {
        Self {
            id: id.into(),
            role,
            author: None,
            content,
            citations: Vec::new(),
            sources: Vec::new(),
            follow_ups: Vec::new(),
            attachments: Vec::new(),
            branch: None,
            retryable: false,
            appearance: ChatMessageAppearance::default(),
            actions: None,
        }
    }

    /// Overrides the actions this message offers (default: [`MessageActions::for_role`]).
    pub fn actions(mut self, actions: MessageActions) -> Self {
        self.actions = Some(actions);
        self
    }

    /// The actions this message offers.
    pub fn message_actions(&self) -> MessageActions {
        self.actions
            .unwrap_or_else(|| MessageActions::for_role(self.role))
    }

    /// Sets this message's presentation (alignment and bubble treatment).
    pub fn with_appearance(mut self, appearance: ChatMessageAppearance) -> Self {
        self.appearance = appearance;
        self
    }

    /// This message's presentation.
    pub fn appearance(&self) -> &ChatMessageAppearance {
        &self.appearance
    }

    /// Adds an optional visible author name.
    pub fn author(mut self, author: impl Into<SharedString>) -> Self {
        self.author = Some(author.into());
        self
    }

    /// Adds application-owned inline citation metadata.
    pub fn citations(mut self, citations: impl IntoIterator<Item = CitationRef>) -> Self {
        self.citations = citations.into_iter().collect();
        self
    }

    /// Adds source labels shown after progressive content settles.
    pub fn sources(mut self, sources: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.sources = sources.into_iter().map(Into::into).collect();
        self
    }

    /// Adds stable follow-up suggestions.
    pub fn follow_ups(mut self, follow_ups: impl IntoIterator<Item = FollowUp>) -> Self {
        self.follow_ups = follow_ups.into_iter().collect();
        self
    }

    /// Attaches files shown read-only above the content; activating one
    /// reports [`ChatEvent::AttachmentActivated`].
    pub fn attachments(mut self, attachments: impl IntoIterator<Item = Attachment>) -> Self {
        self.attachments = attachments.into_iter().collect();
        self
    }

    /// Marks this message as one of several versions; the header shows a
    /// switcher that reports [`ChatEvent::BranchSelected`].
    pub fn branch(mut self, position: BranchPosition) -> Self {
        self.branch = Some(position);
        self
    }

    /// Controls whether a failed message exposes a retry action.
    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// Returns the stable application-level message identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the message role.
    pub fn role(&self) -> ChatRole {
        self.role
    }

    /// Returns the optional visible author.
    pub fn author_name(&self) -> Option<&SharedString> {
        self.author.as_ref()
    }

    /// Returns the progressive content snapshot.
    pub fn content(&self) -> &StreamedContent {
        &self.content
    }

    /// Returns inline citation metadata.
    pub fn citation_refs(&self) -> &[CitationRef] {
        &self.citations
    }

    /// Returns source labels.
    pub fn source_labels(&self) -> &[SharedString] {
        &self.sources
    }

    /// Returns follow-up suggestions.
    pub fn follow_up_suggestions(&self) -> &[FollowUp] {
        &self.follow_ups
    }

    /// Returns the attachments carried by this message.
    pub fn attachment_refs(&self) -> &[Attachment] {
        &self.attachments
    }

    /// Returns where this message sits among its versions, if branched.
    pub fn branch_position(&self) -> Option<BranchPosition> {
        self.branch
    }

    /// Returns whether a failed message may be retried.
    pub fn is_retryable(&self) -> bool {
        self.retryable
    }

    fn accessibility_label(&self) -> SharedString {
        match &self.author {
            Some(author) => format!("{author}, {} message", self.role.label()).into(),
            None => format!("{} message", self.role.label()).into(),
        }
    }

    fn state_description(&self) -> SharedString {
        match self.content.state() {
            ProgressState::Pending => "Pending".into(),
            ProgressState::Running => "Streaming".into(),
            ProgressState::Complete => "Complete".into(),
            ProgressState::Failed(reason) => format!("Failed: {reason}").into(),
        }
    }
}

/// A typed application intent emitted by [`Chat`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatEvent {
    /// A cloned event from the composed [`PromptBar`].
    Prompt(PromptBarEvent),
    /// The user requested another attempt for a failed message.
    RetryRequested {
        /// Stable message identifier.
        message_id: SharedString,
    },
    /// The user selected a follow-up attached to a message.
    FollowUpSelected {
        /// Stable message identifier.
        message_id: SharedString,
        /// Stable follow-up identifier.
        follow_up_id: SharedString,
    },
    /// The user activated a source chip attached to a message.
    SourceActivated {
        /// Stable message identifier.
        message_id: SharedString,
        /// Stable source identifier.
        source_id: SharedString,
        /// The source location.
        url: SharedString,
    },
    /// The user activated an attachment carried by a message.
    AttachmentActivated {
        /// Stable message identifier.
        message_id: SharedString,
        /// Stable attachment identifier.
        attachment_id: SharedString,
    },
    /// The user activated an inline citation attached to a message.
    CitationActivated {
        /// Stable message identifier.
        message_id: SharedString,
        /// Stable citation identifier.
        citation_id: SharedString,
        /// Opaque application-owned destination.
        destination: SharedString,
    },
    /// The user copied a message; the text is already on the clipboard.
    MessageCopied {
        /// Stable message identifier.
        message_id: SharedString,
    },
    /// The user asked for a new response in place of this one.
    RegenerateRequested {
        /// Stable message identifier.
        message_id: SharedString,
    },
    /// The user opened the in-place editor for this message.
    EditRequested {
        /// Stable message identifier.
        message_id: SharedString,
    },
    /// The user committed an in-place edit; the application decides
    /// whether that resends, branches, or simply rewrites the message.
    EditSubmitted {
        /// Stable message identifier.
        message_id: SharedString,
        /// The edited text.
        text: SharedString,
    },
    /// The user abandoned an in-place edit.
    EditCancelled {
        /// Stable message identifier.
        message_id: SharedString,
    },
    /// The user moved to another version of a branched message.
    BranchSelected {
        /// Stable message identifier.
        message_id: SharedString,
        /// Zero-based index of the chosen version.
        index: usize,
    },
    /// The user rated a response.
    FeedbackSubmitted {
        /// Stable message identifier.
        message_id: SharedString,
        /// `true` for helpful, `false` for not helpful.
        positive: bool,
    },
    /// The user chose a welcome suggestion.
    SuggestionSelected {
        /// Stable suggestion identifier.
        suggestion_id: SharedString,
    },
    /// The user returned the transcript to active tail-following.
    JumpedToLatest,
}

fn message_ids_are_unique(messages: &[ChatMessage]) -> bool {
    let mut ids = HashSet::with_capacity(messages.len());
    messages.iter().all(|message| ids.insert(&message.id))
}

fn structural_splice(old: &[ChatMessage], new: &[ChatMessage]) -> (Range<usize>, usize) {
    let prefix = old
        .iter()
        .zip(new)
        .take_while(|(old, new)| old.id == new.id)
        .count();
    let max_suffix = old.len().min(new.len()).saturating_sub(prefix);
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suffix)
        .take_while(|(old, new)| old.id == new.id)
        .count();
    (
        prefix..old.len().saturating_sub(suffix),
        new.len().saturating_sub(prefix + suffix),
    )
}

fn chat_frame(id: &SharedString) -> Stateful<gpui::Div> {
    v_flex()
        .id(id.clone())
        .accessibility_id(format!("chat.{id}"))
        .role(Role::Log)
        .aria_label("Conversation")
        .tab_group()
        .size_full()
        .min_h_0()
        .min_w_0()
}

fn transcript_frame(id: ElementId) -> Stateful<gpui::Div> {
    div()
        .id(id)
        .debug_selector(|| "chat-transcript".into())
        .role(Role::List)
        .aria_label("Messages")
        .min_h_0()
        .min_w_0()
}

fn message_frame(chat_id: &SharedString, message: &ChatMessage) -> Stateful<gpui::Div> {
    let id = message.id.clone();
    let debug_id = id.to_string();
    v_flex()
        .id((ElementId::from(chat_id.clone()), id.clone()))
        .debug_selector(move || format!("chat-message-{debug_id}"))
        .accessibility_id(format!("chat.message.{id}"))
        .role(Role::ListItem)
        .aria_label(message.accessibility_label())
        .aria_description(message.state_description())
        .w_full()
        .min_w_0()
}

fn retry_button(chat_id: &SharedString, message_id: &SharedString, cx: &mut App) -> Button {
    let debug_id = message_id.to_string();
    outlined_control(
        (
            ElementId::from((ElementId::from(chat_id.clone()), message_id.clone())),
            "retry",
        ),
        "Retry message",
        cx,
    )
    .debug_selector(move || format!("chat-retry-{debug_id}"))
}

fn jump_to_latest_button(chat_id: &SharedString, label: SharedString, cx: &mut App) -> Button {
    outlined_control((ElementId::from(chat_id.clone()), "jump-latest"), label, cx)
        .debug_selector(|| "chat-jump-latest".into())
}

/// A controlled virtualized conversation composed with [`PromptBar`] and
/// [`StreamingText`].
///
/// The application owns the `Arc<[ChatMessage]>` snapshot and every async
/// producer. Chat retains only variable-height list, tail-follow, unread, and
/// prompt-subscription state. The pinned `gpui::ListState` is used because it
/// measures wrapped visible rows on demand and preserves logical anchors;
/// `gpui_base::v_virtual_list` requires exact heights for every row up front.
///
/// # Example
///
/// ```ignore
/// let prompt = cx.new(|cx| PromptBar::new("conversation-prompt", window, cx));
/// let chat = cx.new(|cx| Chat::new("conversation", prompt, window, cx));
/// chat.update(cx, |chat, cx| {
///     chat.set_messages(
///         Arc::from([ChatMessage::new(
///             "welcome",
///             ChatRole::Assistant,
///             StreamedContent::done("How can I help?"),
///         )]),
///         window,
///         cx,
///     );
/// });
/// ```
pub struct Chat {
    id: SharedString,
    prompt_bar: Entity<PromptBar>,
    messages: Arc<[ChatMessage]>,
    message_focus_handles: HashMap<SharedString, FocusHandle>,
    list_state: ListState,
    /// Rem size the retained row heights were measured against.
    resolved_layout: ResolvedLayoutKey,
    visible_range: Range<usize>,
    pinned_to_bottom: bool,
    unread_message_ids: HashSet<SharedString>,
    /// Messages appended to a live transcript; they settle in with one
    /// reveal, while a freshly loaded history appears at rest.
    arrivals: HashSet<SharedString>,
    welcome: Option<ChatWelcome>,
    copied_message: Option<SharedString>,
    copied_reset: Option<Task<()>>,
    feedback: HashMap<SharedString, bool>,
    editing: Option<EditSession>,
    _prompt_subscription: Subscription,
}

impl Chat {
    /// Creates an empty chat around an application-supplied prompt entity.
    pub fn new(
        id: impl Into<SharedString>,
        prompt_bar: Entity<PromptBar>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let list_state = ListState::new(0, ListAlignment::Bottom, px(256.));
        list_state.set_follow_mode(FollowMode::Tail);
        let weak_chat = cx.weak_entity();
        list_state.set_scroll_handler(move |event, _, cx| {
            weak_chat
                .update(cx, |chat, cx| {
                    chat.visible_range = event.visible_range.clone();
                    chat.pinned_to_bottom = event.is_following_tail;
                    if event.is_following_tail {
                        chat.unread_message_ids.clear();
                    }
                    cx.notify();
                })
                .ok();
        });
        let prompt_subscription = cx.subscribe_in(
            &prompt_bar,
            window,
            |_, _, event: &PromptBarEvent, _, cx| {
                cx.emit(ChatEvent::Prompt(event.clone()));
            },
        );

        Self {
            id: id.into(),
            prompt_bar,
            messages: Arc::from([]),
            message_focus_handles: HashMap::new(),
            list_state,
            resolved_layout: ResolvedLayoutKey::default(),
            visible_range: 0..0,
            pinned_to_bottom: true,
            unread_message_ids: HashSet::new(),
            arrivals: HashSet::new(),
            welcome: None,
            copied_message: None,
            copied_reset: None,
            feedback: HashMap::new(),
            editing: None,
            _prompt_subscription: prompt_subscription,
        }
    }

    /// Sets what an empty conversation shows.
    pub fn set_welcome(&mut self, welcome: Option<ChatWelcome>, cx: &mut Context<Self>) {
        if self.welcome != welcome {
            self.welcome = welcome;
            cx.notify();
        }
    }

    /// Replaces the controlled message snapshot.
    ///
    /// A malformed snapshot containing duplicate IDs is ignored so recycled
    /// rows can never alias one another. Structural edits are applied through
    /// one list splice, while content-only changes invalidate only their
    /// retained rows. When not following the tail, the prior top message ID
    /// and pixel offset are restored in the replacement snapshot.
    pub fn set_messages(
        &mut self,
        messages: Arc<[ChatMessage]>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Arc::ptr_eq(&self.messages, &messages) || self.messages.as_ref() == messages.as_ref() {
            return;
        }
        if !message_ids_are_unique(&messages) {
            return;
        }

        let was_following = self.list_state.is_following_tail() && self.pinned_to_bottom;
        let was_empty = self.messages.is_empty();
        let old_offset = self.list_state.logical_scroll_top();
        let old_anchor = self
            .messages
            .get(old_offset.item_ix)
            .map(|message| (message.id.clone(), old_offset.offset_in_item));
        let old_by_id: HashMap<_, _> = self
            .messages
            .iter()
            .map(|message| (message.id.clone(), message))
            .collect();
        let last_retained_index = messages
            .iter()
            .rposition(|message| old_by_id.contains_key(&message.id));
        // A snapshot that shares no identity with the previous one is a
        // different conversation, not a hundred new messages. Without this the
        // switch would cue every message as an arrival, animate them all in,
        // and mark the whole history unread.
        let is_replacement = !was_empty && !messages.is_empty() && last_retained_index.is_none();
        let appended_ids = if is_replacement {
            Vec::new()
        } else {
            messages
                .iter()
                .skip(last_retained_index.map_or(0, |index| index + 1))
                .filter(|message| !old_by_id.contains_key(&message.id))
                .map(|message| message.id.clone())
                .collect::<Vec<_>>()
        };
        // Only messages appended to a live transcript animate in and cue;
        // a freshly loaded history settles silently.
        let arrived = if was_empty {
            Vec::new()
        } else {
            appended_ids.clone()
        };
        let settled = messages
            .iter()
            .filter(|message| {
                old_by_id
                    .get(&message.id)
                    .is_some_and(|old| old.content.is_streaming())
                    && matches!(
                        message.content.state(),
                        ProgressState::Complete | ProgressState::Failed(_)
                    )
            })
            .map(|message| {
                (
                    message.id.clone(),
                    *message.content.state() == ProgressState::Complete,
                )
            })
            .collect::<Vec<_>>();
        let (old_range, new_count) = structural_splice(&self.messages, &messages);
        let structural_new_range = old_range.start..old_range.start + new_count;
        let mut next_focus_handles = HashMap::with_capacity(messages.len());
        for message in messages.iter() {
            let focus_handle = self
                .message_focus_handles
                .remove(&message.id)
                .unwrap_or_else(|| cx.focus_handle());
            next_focus_handles.insert(message.id.clone(), focus_handle);
        }

        if !old_range.is_empty() || new_count > 0 {
            let inserted_focus_handles = messages[structural_new_range.clone()]
                .iter()
                .map(|message| next_focus_handles.get(&message.id).cloned());
            self.list_state
                .splice_focusable(old_range, inserted_focus_handles);
        }
        self.message_focus_handles = next_focus_handles;

        let changed = messages
            .iter()
            .enumerate()
            .filter(|(ix, message)| {
                !structural_new_range.contains(ix)
                    && old_by_id
                        .get(&message.id)
                        .is_some_and(|old| *old != *message)
            })
            .map(|(ix, _)| ix)
            .collect::<Vec<_>>();
        for ix in changed {
            self.list_state.remeasure_items(ix..ix + 1);
        }

        let new_ids = messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<HashSet<_>>();
        self.unread_message_ids
            .retain(|message_id| new_ids.contains(message_id));
        // Transient state belongs to a message, so it cannot outlive one.
        // Otherwise feedback grows without bound across conversation switches
        // and an open editor keeps pointing at a message that is gone.
        self.feedback
            .retain(|message_id, _| new_ids.contains(message_id));
        if self
            .copied_message
            .as_ref()
            .is_some_and(|message_id| !new_ids.contains(message_id))
        {
            self.copied_message = None;
            self.copied_reset = None;
        }
        let stale_editor = self
            .editing
            .as_ref()
            .filter(|session| !new_ids.contains(&session.message_id))
            .map(|session| session.editor.clone());
        if let Some(editor) = stale_editor {
            // Dropping the session is not enough on its own. If the editor
            // being removed holds focus, the window keeps pointing at a
            // handle whose entity is gone: the caret disappears and typing
            // reaches nothing. Hand focus to the composer first.
            let editor_had_focus = editor.read(cx).focus_handle(cx).is_focused(window);
            self.editing = None;
            if editor_had_focus {
                let prompt_focus = self.prompt_bar.read(cx).focus_handle(cx);
                window.focus(&prompt_focus, cx);
            }
        }
        self.messages = messages;
        if self.messages.is_empty() {
            self.list_state.set_follow_mode(FollowMode::Tail);
            self.list_state.scroll_to_end();
            self.unread_message_ids.clear();
            self.pinned_to_bottom = true;
        } else if was_following || is_replacement {
            // A different conversation opens at its end, like opening it fresh,
            // rather than inheriting the previous transcript's scroll offset.
            if is_replacement {
                self.list_state.set_follow_mode(FollowMode::Tail);
            }
            self.list_state.scroll_to_end();
            self.unread_message_ids.clear();
            self.pinned_to_bottom = true;
        } else {
            if let Some((anchor_id, offset_in_item)) = old_anchor
                && let Some(item_ix) = self
                    .messages
                    .iter()
                    .position(|message| message.id == anchor_id)
            {
                self.list_state.scroll_to(ListOffset {
                    item_ix,
                    offset_in_item,
                });
            }
            self.unread_message_ids.extend(appended_ids);
        }
        self.arrivals
            .retain(|message_id| new_ids.contains(message_id));
        self.arrivals.extend(arrived.iter().cloned());
        for message_id in arrived {
            cues::emit(cx, Cue::MessageArrived { message_id });
        }
        for (message_id, succeeded) in settled {
            cues::emit(
                cx,
                Cue::ResponseSettled {
                    message_id,
                    succeeded,
                },
            );
        }
        cx.notify();
    }

    /// Returns the current application-owned message snapshot.
    pub fn messages(&self) -> &Arc<[ChatMessage]> {
        &self.messages
    }

    /// Returns the composed prompt entity.
    pub fn prompt_bar(&self) -> &Entity<PromptBar> {
        &self.prompt_bar
    }

    /// Returns the number of messages received while the transcript was offscreen.
    pub fn unread_count(&self) -> usize {
        self.unread_message_ids.len()
    }

    /// Returns whether the list is actively following the latest content.
    pub fn is_pinned_to_bottom(&self) -> bool {
        self.list_state.is_following_tail() && self.pinned_to_bottom
    }

    /// Returns the currently known visible message range.
    pub fn visible_range(&self) -> Range<usize> {
        self.visible_range.clone()
    }

    /// Resumes tail-following, clears unread state, and emits a typed event.
    pub fn scroll_to_latest(&mut self, cx: &mut Context<Self>) {
        self.list_state.set_follow_mode(FollowMode::Tail);
        self.list_state.scroll_to_end();
        self.pinned_to_bottom = true;
        self.unread_message_ids.clear();
        cx.emit(ChatEvent::JumpedToLatest);
        cx.notify();
    }

    /// Re-measures the transcript after the window's rem size changed.
    ///
    /// Row heights cache wrapped text laid out at the previous rem, and no
    /// message snapshot reports a zoom change, so nothing else invalidates
    /// them. The anchor policy is Chat's own: a transcript already following
    /// the tail keeps following it, and otherwise the message that was first
    /// on screen stays first.
    fn resolve_layout(&mut self, rem_size: Pixels, cx: &mut Context<Self>) {
        if !self.resolved_layout.observe(rem_size) {
            return;
        }
        let was_following = self.is_pinned_to_bottom();
        let offset = self.list_state.logical_scroll_top();
        let anchor = self
            .messages
            .get(offset.item_ix)
            .map(|message| (message.id.clone(), offset.offset_in_item));

        self.list_state.remeasure();
        if was_following {
            self.list_state.set_follow_mode(FollowMode::Tail);
            self.list_state.scroll_to_end();
            self.pinned_to_bottom = true;
        } else if let Some((anchor_id, offset_in_item)) = anchor
            && let Some(item_ix) = self
                .messages
                .iter()
                .position(|message| message.id == anchor_id)
        {
            self.list_state.scroll_to(ListOffset {
                item_ix,
                offset_in_item,
            });
        }
        cx.notify();
    }

    fn forward_streaming_event(
        &mut self,
        message_id: SharedString,
        event: &StreamingTextEvent,
        cx: &mut Context<Self>,
    ) {
        match event {
            StreamingTextEvent::FollowUpSelected { id } => {
                cx.emit(ChatEvent::FollowUpSelected {
                    message_id,
                    follow_up_id: id.clone(),
                });
            }
            StreamingTextEvent::CitationActivated { id, destination } => {
                cx.emit(ChatEvent::CitationActivated {
                    message_id,
                    citation_id: id.clone(),
                    destination: destination.clone(),
                });
            }
            StreamingTextEvent::SourceActivated { id, url } => {
                cx.emit(ChatEvent::SourceActivated {
                    message_id,
                    source_id: id.clone(),
                    url: url.clone(),
                });
            }
        }
    }

    fn retry(&mut self, message_id: SharedString, cx: &mut Context<Self>) {
        cx.emit(ChatEvent::RetryRequested { message_id });
    }

    /// Opens the in-place editor for a message, prefilled with its text, and
    /// reports [`ChatEvent::EditRequested`]. Enter or Save reports
    /// [`ChatEvent::EditSubmitted`]; Escape or Cancel reports
    /// [`ChatEvent::EditCancelled`]. The message snapshot is untouched until
    /// the application applies the edit.
    pub fn begin_edit(
        &mut self,
        message_id: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message_id = message_id.into();
        let Some(message) = self
            .messages
            .iter()
            .find(|message| message.id == message_id)
        else {
            return;
        };
        if self
            .editing
            .as_ref()
            .is_some_and(|session| session.message_id == message_id)
        {
            return;
        }
        let text = message.content.text().to_owned();
        let editor = cx.new(|cx| {
            let mut state = TextareaState::new(window, cx)
                .auto_grow(1, 8)
                .submit_on_enter(true);
            state.set_value(text, window, cx);
            state
        });
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |this, _, event: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { shift: false, .. } = event {
                    this.submit_edit(window, cx);
                    cx.stop_propagation();
                }
            },
        );
        let focus_handle = editor.read(cx).focus_handle(cx);
        window.focus(&focus_handle, cx);
        self.editing = Some(EditSession {
            message_id: message_id.clone(),
            editor,
            _subscription: subscription,
        });
        cx.emit(ChatEvent::EditRequested { message_id });
        cx.notify();
    }

    /// Replaces the in-place editor's draft.
    pub fn set_edit_draft(
        &mut self,
        draft: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = &self.editing else {
            return;
        };
        let draft = draft.into();
        session.editor.update(cx, |editor, cx| {
            editor.set_value(draft, window, cx);
        });
    }

    /// Returns the message being edited in place, if any.
    pub fn editing_message(&self) -> Option<&SharedString> {
        self.editing.as_ref().map(|session| &session.message_id)
    }

    /// Commits the in-place edit. Empty drafts are ignored so a stray Enter
    /// cannot erase a message.
    pub fn submit_edit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = &self.editing else {
            return;
        };
        let draft = session.editor.read(cx).value().to_string();
        let text = draft.trim();
        if text.is_empty() {
            return;
        }
        let message_id = session.message_id.clone();
        let text: SharedString = text.to_owned().into();
        self.editing = None;
        cx.emit(ChatEvent::EditSubmitted { message_id, text });
        cx.notify();
    }

    /// Abandons the in-place edit.
    pub fn cancel_edit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(session) = self.editing.take() else {
            return;
        };
        cx.emit(ChatEvent::EditCancelled {
            message_id: session.message_id,
        });
        cx.notify();
    }

    fn copy_message(&mut self, message_id: SharedString, cx: &mut Context<Self>) {
        let Some(message) = self
            .messages
            .iter()
            .find(|message| message.id == message_id)
        else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(message.content.text().to_owned()));
        self.copied_message = Some(message_id.clone());
        // One-shot confirmation owned by the entity: dropping the chat drops
        // the timer, and a second copy restarts it.
        self.copied_reset = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_FEEDBACK).await;
            this.update(cx, |chat, cx| {
                chat.copied_message = None;
                chat.copied_reset = None;
                cx.notify();
            })
            .ok();
        }));
        cx.emit(ChatEvent::MessageCopied { message_id });
        cues::emit(cx, Cue::Copied);
        cx.notify();
    }

    fn submit_feedback(
        &mut self,
        message_id: SharedString,
        positive: bool,
        cx: &mut Context<Self>,
    ) {
        self.feedback.insert(message_id.clone(), positive);
        cx.emit(ChatEvent::FeedbackSubmitted {
            message_id,
            positive,
        });
        cx.notify();
    }

    fn render_actions(
        &self,
        message: &ChatMessage,
        is_last: bool,
        message_focus_handle: &FocusHandle,
        group_name: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let actions = message.message_actions();
        if self
            .editing
            .as_ref()
            .is_some_and(|session| session.message_id == message.id)
        {
            return None;
        }
        let settled = matches!(
            message.content.state(),
            ProgressState::Complete | ProgressState::Failed(_)
        );
        if actions.is_empty() || !settled {
            return None;
        }
        let tokens = cx.theme().semantic_tokens();
        let message_id = message.id.clone();
        let base_id = ElementId::from((ElementId::from(self.id.clone()), message_id.clone()));
        let copied = self.copied_message.as_ref() == Some(&message_id);
        let rating = self.feedback.get(&message_id).copied();
        // Quiet at rest; revealed by pointer hover on the row, by keyboard
        // focus inside it, and permanently on the last settled message.
        let always_visible = is_last || message_focus_handle.contains_focused(window, cx);
        let debug_id = message_id.to_string();
        let bar = h_flex()
            .id((base_id.clone(), "actions"))
            .debug_selector(move || format!("chat-actions-{debug_id}"))
            .role(Role::Toolbar)
            .aria_label("Message actions")
            .tab_group()
            .items_center()
            .gap(tokens.spacing.xxs)
            .opacity(if always_visible { 1.0 } else { 0.0 })
            .group_hover(group_name, |style| style.opacity(1.0))
            .when(actions.copy, |bar| {
                let id = message_id.clone();
                let debug_id = message_id.to_string();
                let copy_button = icon_button(
                    (base_id.clone(), "copy"),
                    if copied {
                        IconName::Check
                    } else {
                        IconName::Copy
                    },
                    if copied { "Copied" } else { "Copy message" },
                    cx,
                )
                .debug_selector(move || format!("chat-action-copy-{debug_id}"))
                .when(copied, |button| button.text_color(cx.theme().success))
                .on_click(cx.listener(move |chat, _, _, cx| {
                    chat.copy_message(id.clone(), cx);
                }));
                // The check pops in once per copy: its keyed reveal state is
                // dropped with the icon, so the next copy replays it.
                bar.child(if copied {
                    reveal(copy_button, (base_id.clone(), "copied"), window, cx)
                } else {
                    copy_button
                })
            })
            .when(actions.regenerate, |bar| {
                let id = message_id.clone();
                let debug_id = message_id.to_string();
                bar.child(
                    icon_button(
                        (base_id.clone(), "regenerate"),
                        IconName::Redo,
                        "Regenerate response",
                        cx,
                    )
                    .debug_selector(move || format!("chat-action-regenerate-{debug_id}"))
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(ChatEvent::RegenerateRequested {
                            message_id: id.clone(),
                        });
                    })),
                )
            })
            .when(actions.edit, |bar| {
                let id = message_id.clone();
                let debug_id = message_id.to_string();
                bar.child(
                    icon_button(
                        (base_id.clone(), "edit"),
                        IconName::Replace,
                        "Edit message",
                        cx,
                    )
                    .debug_selector(move || format!("chat-action-edit-{debug_id}"))
                    .on_click(cx.listener(move |chat, _, window, cx| {
                        chat.begin_edit(id.clone(), window, cx);
                    })),
                )
            })
            .when(actions.feedback, |bar| {
                let up_id = message_id.clone();
                let down_id = message_id.clone();
                let up_debug_id = message_id.to_string();
                let down_debug_id = message_id.to_string();
                bar.child(
                    icon_button(
                        (base_id.clone(), "helpful"),
                        IconName::ThumbsUp,
                        if rating == Some(true) {
                            "Marked helpful"
                        } else {
                            "Mark helpful"
                        },
                        cx,
                    )
                    .debug_selector(move || format!("chat-action-helpful-{up_debug_id}"))
                    .selected(rating == Some(true))
                    .when(rating == Some(true), |button| {
                        button.text_color(cx.theme().primary)
                    })
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        chat.submit_feedback(up_id.clone(), true, cx);
                    })),
                )
                .child(
                    icon_button(
                        (base_id.clone(), "unhelpful"),
                        IconName::ThumbsDown,
                        if rating == Some(false) {
                            "Marked not helpful"
                        } else {
                            "Mark not helpful"
                        },
                        cx,
                    )
                    .debug_selector(move || format!("chat-action-unhelpful-{down_debug_id}"))
                    .selected(rating == Some(false))
                    .when(rating == Some(false), |button| {
                        button.text_color(cx.theme().primary)
                    })
                    .on_click(cx.listener(move |chat, _, _, cx| {
                        chat.submit_feedback(down_id.clone(), false, cx);
                    })),
                )
            });
        Some(bar.into_any_element())
    }

    fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let tokens = cx.theme().semantic_tokens();
        let Some(welcome) = &self.welcome else {
            return div()
                .id((ElementId::from(self.id.clone()), "empty"))
                .role(Role::Status)
                .aria_label("No messages yet")
                .p(tokens.spacing.md)
                .text_token(tokens.typography.sm)
                .text_color(cx.theme().muted_foreground)
                .child("No messages yet")
                .into_any_element();
        };
        v_flex()
            .id((ElementId::from(self.id.clone()), "welcome"))
            .debug_selector(|| "chat-welcome".into())
            .role(Role::Group)
            .aria_label(welcome.title.clone())
            .size_full()
            .items_center()
            .justify_center()
            .gap(tokens.spacing.md)
            .p(tokens.spacing.xl)
            .child(Orbs::new())
            .child(
                div()
                    .text_token(tokens.typography.lg)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(cx.theme().foreground)
                    .text_center()
                    .child(welcome.title.clone()),
            )
            .when_some(welcome.description.clone(), |this, description| {
                this.child(
                    div()
                        .max_w(relative(0.8))
                        .text_token(tokens.typography.sm)
                        .text_color(cx.theme().muted_foreground)
                        .text_center()
                        .child(description),
                )
            })
            .when(!welcome.suggestions.is_empty(), |this| {
                this.child(
                    Suggestions::new((ElementId::from(self.id.clone()), "suggestions"))
                        .items(welcome.suggestions.iter().cloned())
                        .justify_center()
                        .on_event(cx.listener(|_, event: &SuggestionsEvent, _, cx| {
                            let SuggestionsEvent::Selected { id } = event;
                            cx.emit(ChatEvent::SuggestionSelected {
                                suggestion_id: id.clone(),
                            });
                        })),
                )
            })
            .into_any_element()
    }

    fn render_message(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(message) = self.messages.get(index).cloned() else {
            return div().hidden().into_any_element();
        };
        let tokens = cx.theme().semantic_tokens();
        let message_id = message.id.clone();
        let Some(message_focus_handle) = self.message_focus_handles.get(&message_id).cloned()
        else {
            return div().hidden().into_any_element();
        };
        let content_id = ElementId::from((
            ElementId::from((ElementId::from(self.id.clone()), message_id.clone())),
            "content",
        ));
        let content = match message.role {
            ChatRole::Assistant | ChatRole::Tool => {
                StreamingText::new(content_id, &message.content)
                    .citations(message.citations.clone())
                    .sources(message.sources.clone())
                    .follow_ups(message.follow_ups.clone())
                    .on_event(cx.listener({
                        let message_id = message_id.clone();
                        move |chat, event, _, cx| {
                            chat.forward_streaming_event(message_id.clone(), event, cx);
                        }
                    }))
                    .into_any_element()
            }
            ChatRole::User | ChatRole::System => {
                TextView::markdown(content_id, message.content.text())
                    .selectable(true)
                    .into_any_element()
            }
        };
        let content = match self
            .editing
            .as_ref()
            .filter(|session| session.message_id == message_id)
        {
            Some(session) => self.render_editor(&message_id, session.editor.clone(), cx),
            None => content,
        };
        let attachments = (!message.attachments.is_empty()).then(|| {
            let strip_id = ElementId::from((
                ElementId::from((ElementId::from(self.id.clone()), message_id.clone())),
                "attachments",
            ));
            AttachmentStrip::new(strip_id)
                .label("Message attachments")
                .items(message.attachments.iter().cloned())
                .on_event(cx.listener({
                    let message_id = message_id.clone();
                    move |_, event: &AttachmentEvent, _, cx| {
                        if let AttachmentEvent::Opened { id } = event {
                            cx.emit(ChatEvent::AttachmentActivated {
                                message_id: message_id.clone(),
                                attachment_id: id.clone(),
                            });
                        }
                    }
                }))
        });
        let retryable_failure =
            message.retryable && matches!(message.content.state(), ProgressState::Failed(_));
        let author = message
            .author
            .clone()
            .unwrap_or_else(|| message.role.label().into());
        let heading_id = ElementId::from((
            ElementId::from((ElementId::from(self.id.clone()), message_id.clone())),
            "heading",
        ));

        // The semantic list item spans the transcript, while the visual
        // bubble is a constrained child. That separation lets alignment move
        // the actual painted surface instead of merely right-aligning content
        // inside a full-width background.
        let appearance = message.appearance();
        let bubble_debug_id = message_id.clone();
        let group_name: SharedString = format!("{}-message-group-{message_id}", self.id).into();
        let is_last = index + 1 == self.messages.len();
        let actions = self.render_actions(
            &message,
            is_last,
            &message_focus_handle,
            group_name.clone(),
            window,
            cx,
        );
        let row = message_frame(&self.id, &message)
            .track_focus(&message_focus_handle)
            .group(group_name)
            .px(tokens.spacing.md)
            .py(tokens.spacing.sm);
        let bubble = v_flex()
            .id((
                ElementId::from((ElementId::from(self.id.clone()), message_id.clone())),
                "bubble",
            ))
            .debug_selector(move || format!("chat-message-bubble-{bubble_debug_id}"))
            .min_w_0()
            .gap(tokens.spacing.sm)
            .px(tokens.spacing.md)
            .py(tokens.spacing.md);
        let bubble = match appearance.bubble() {
            MessageBubble::Bordered => bubble
                .w_auto()
                .max_w(relative(0.82))
                .border_1()
                .border_color(cx.theme().border)
                .rounded(tokens.radius.md)
                .bg(match message.role {
                    ChatRole::User => cx.theme().secondary,
                    ChatRole::Assistant | ChatRole::System | ChatRole::Tool => {
                        cx.theme().background
                    }
                }),
            MessageBubble::Filled => bubble
                .w_auto()
                .max_w(relative(0.82))
                .rounded(tokens.radius.md)
                .bg(match message.role {
                    ChatRole::User => cx.theme().secondary,
                    ChatRole::Assistant | ChatRole::System | ChatRole::Tool => cx.theme().muted,
                }),
            MessageBubble::Plain => bubble.w_full(),
        };
        let row = if appearance.alignment() == MessageAlignment::Trailing {
            row.items_end()
        } else {
            row.items_start()
        };

        let arrived = self.arrivals.contains(&message_id);
        let arrival_id = ElementId::from((
            ElementId::from((ElementId::from(self.id.clone()), message_id.clone())),
            "arrival",
        ));
        let heading = div()
            .id(heading_id)
            .role(Role::Heading)
            .aria_label(author.clone())
            .text_token(tokens.typography.sm)
            .text_color(cx.theme().foreground)
            .child(author);
        let header = match message.branch {
            Some(position) => h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(tokens.spacing.sm)
                .child(heading)
                .child(self.render_branch_nav(&message_id, position, cx))
                .into_any_element(),
            None => heading.into_any_element(),
        };
        let row = row.child(
            bubble
                .child(header)
                .children(attachments)
                .child(content)
                .when(retryable_failure, |this| {
                    this.child(
                        retry_button(&self.id, &message_id, cx).on_click(cx.listener(
                            move |chat, _, _, cx| {
                                chat.retry(message_id.clone(), cx);
                            },
                        )),
                    )
                })
                .children(actions),
        );
        if arrived {
            reveal(row, arrival_id, window, cx).into_any_element()
        } else {
            row.into_any_element()
        }
    }
}

impl Chat {
    fn render_editor(
        &self,
        message_id: &SharedString,
        editor: Entity<TextareaState>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = cx.theme().semantic_tokens();
        let base = ElementId::from((ElementId::from(self.id.clone()), message_id.clone()));
        let editor_debug_id = message_id.to_string();
        let cancel_debug_id = message_id.to_string();
        let save_debug_id = message_id.to_string();
        v_flex()
            .id((base.clone(), "editor"))
            .debug_selector(move || format!("chat-edit-editor-{editor_debug_id}"))
            .w_full()
            .min_w_0()
            .gap(tokens.spacing.xs)
            .capture_action(cx.listener(|chat, _: &Escape, window, cx| {
                chat.cancel_edit(window, cx);
            }))
            .child(Textarea::new(&editor))
            .child(
                h_flex()
                    .justify_end()
                    .items_center()
                    .gap(tokens.spacing.xs)
                    .child(
                        outlined_control_with_label(
                            (base.clone(), "edit-cancel"),
                            "Cancel edit",
                            "Cancel",
                            cx,
                        )
                        .debug_selector(move || format!("chat-edit-cancel-{cancel_debug_id}"))
                        .on_click(cx.listener(|chat, _, window, cx| {
                            chat.cancel_edit(window, cx);
                        })),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("chat-edit-save-{save_debug_id}"))
                            .child(
                                LabeledButton::new((base, "edit-save"))
                                    .primary()
                                    .small()
                                    .label("Save")
                                    .on_click(cx.listener(|chat, _, window, cx| {
                                        chat.submit_edit(window, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_branch_nav(
        &self,
        message_id: &SharedString,
        position: BranchPosition,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = cx.theme().semantic_tokens();
        let base = ElementId::from((ElementId::from(self.id.clone()), message_id.clone()));
        let group_debug_id = message_id.to_string();
        let prev_debug_id = message_id.to_string();
        let next_debug_id = message_id.to_string();
        let prev_id = message_id.clone();
        let next_id = message_id.clone();
        let label: SharedString = position.label().into();
        h_flex()
            .id((base.clone(), "branches"))
            .role(Role::Group)
            .aria_label(label)
            .debug_selector(move || format!("chat-branches-{group_debug_id}"))
            .flex_none()
            .items_center()
            .gap(tokens.spacing.xxs)
            .child(
                icon_button(
                    (base.clone(), "branch-prev"),
                    IconName::ChevronLeft,
                    "Previous version",
                    cx,
                )
                .disabled(position.index == 0)
                .debug_selector(move || format!("chat-branch-prev-{prev_debug_id}"))
                .on_click(cx.listener(move |_, _, _, cx| {
                    if position.index > 0 {
                        cx.emit(ChatEvent::BranchSelected {
                            message_id: prev_id.clone(),
                            index: position.index - 1,
                        });
                    }
                })),
            )
            .child(meta(
                format!("{} / {}", position.index + 1, position.count),
                cx,
            ))
            .child(
                icon_button(
                    (base, "branch-next"),
                    IconName::ChevronRight,
                    "Next version",
                    cx,
                )
                .disabled(position.index + 1 >= position.count)
                .debug_selector(move || format!("chat-branch-next-{next_debug_id}"))
                .on_click(cx.listener(move |_, _, _, cx| {
                    if position.index + 1 < position.count {
                        cx.emit(ChatEvent::BranchSelected {
                            message_id: next_id.clone(),
                            index: position.index + 1,
                        });
                    }
                })),
            )
            .into_any_element()
    }
}

impl EventEmitter<ChatEvent> for Chat {}

impl Render for Chat {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        // The rem is a resolved-layout input, so a change to it invalidates
        // every measured row height. Reading it here costs nothing; the
        // reaction is deferred so that render itself neither mutates nor
        // notifies.
        let rem_size = window.rem_size();
        if !self.resolved_layout.matches(rem_size) {
            cx.defer_in(window, move |chat, _, cx| {
                chat.resolve_layout(rem_size, cx);
            });
        }

        let tokens = cx.theme().semantic_tokens();
        let transcript_id = ElementId::from((ElementId::from(self.id.clone()), "transcript"));
        let composer_id = ElementId::from((ElementId::from(self.id.clone()), "composer"));
        let unread = self.unread_message_ids.len();
        let show_jump = unread > 0 && !self.list_state.is_following_tail();
        let jump_label: SharedString = format!(
            "Jump to latest, {unread} unread message{}",
            if unread == 1 { "" } else { "s" }
        )
        .into();

        chat_frame(&self.id)
            .gap(tokens.spacing.sm)
            .child(
                transcript_frame(transcript_id)
                    .relative()
                    .flex_1()
                    .overflow_hidden()
                    .when(self.messages.is_empty(), |this| {
                        this.child(self.render_welcome(cx))
                    })
                    .when(!self.messages.is_empty(), |this| {
                        this.child(
                            list(self.list_state.clone(), cx.processor(Self::render_message))
                                .size_full(),
                        )
                        .vertical_scrollbar(&self.list_state)
                        .child(list_scroll_mask(&self.list_state))
                    }),
            )
            .when(show_jump, |this| {
                this.child(
                    jump_to_latest_button(&self.id, jump_label, cx)
                        .on_click(cx.listener(|chat, _, _, cx| chat.scroll_to_latest(cx))),
                )
            })
            .child(
                div()
                    .id(composer_id)
                    .debug_selector(|| "chat-composer".into())
                    .role(Role::Group)
                    .aria_label("Message composer")
                    .flex_none()
                    .child(self.prompt_bar.clone()),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(not(target_os = "macos"))]
    use crate::prompt_bar::PromptBarEvent;
    use crate::{
        prompt_bar::PromptBar,
        stream::{ProgressState, Progressive},
        streaming_text::{CitationRef, FollowUp},
    };
    use gpui::{
        Context, Element as _, Entity, KeyDownEvent, KeyUpEvent, Keystroke, ListOffset, Modifiers,
        Render, RenderOnce as _, Role, SharedString, Subscription, TestAppContext,
        VisualTestContext, Window, accesskit, canvas, px, size,
    };
    use gpui_component::theme::Theme;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, Mutex},
    };

    struct ChatHarness {
        chat: Entity<Chat>,
        events: Rc<RefCell<Vec<ChatEvent>>>,
        _subscription: Subscription,
    }

    impl ChatHarness {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let prompt = cx.new(|cx| PromptBar::new("chat-prompt", window, cx));
            let chat = cx.new(|cx| Chat::new("test-chat", prompt, window, cx));
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

    impl Render for ChatHarness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            self.chat.clone()
        }
    }

    fn message(index: usize) -> ChatMessage {
        ChatMessage::new(
            format!("m{index:04}"),
            if index.is_multiple_of(2) {
                ChatRole::Assistant
            } else {
                ChatRole::User
            },
            Progressive::complete(format!("Message {index}: selectable conversation prose.")),
        )
    }

    fn messages(range: std::ops::Range<usize>) -> Arc<[ChatMessage]> {
        Arc::from(range.map(message).collect::<Vec<_>>())
    }

    fn harness(cx: &mut TestAppContext) -> (Entity<ChatHarness>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (harness, cx) = cx.add_window_view(ChatHarness::new);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(640.), px(420.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (harness, cx)
    }

    fn set_messages(
        harness: &Entity<ChatHarness>,
        messages: Arc<[ChatMessage]>,
        cx: &mut VisualTestContext,
    ) {
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        cx.update(|window, cx| {
            chat.update(cx, |chat, cx| chat.set_messages(messages, window, cx));
            window.draw(cx).clear(cx);
        });
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

    fn top_anchor(chat: &Chat) -> Option<(SharedString, gpui::Pixels)> {
        let offset = chat.list_state.logical_scroll_top();
        chat.messages
            .get(offset.item_ix)
            .map(|message| (message.id.clone(), offset.offset_in_item))
    }

    /// Zooms the way the shell does: the theme carries the base type size and
    /// `Root` hands it to the window every frame.
    ///
    /// Two draws, because Chat notices the new rem while rendering and reacts
    /// afterwards — the first draw is where it sees the change, the second
    /// lays out what it re-measured. Nothing here calls `remeasure`; that Chat
    /// does it unprompted is the property under test.
    fn zoom_to(cx: &mut VisualTestContext, font_size: f32) {
        cx.update(|window, cx| {
            Theme::global_mut(cx).font_size = px(font_size);
            window.set_rem_size(Theme::global(cx).font_size);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    #[derive(Clone, Copy)]
    enum ChatControlKind {
        Retry,
        Jump,
    }

    struct CapturedControlA11y {
        role: Option<Role>,
        node: accesskit::Node,
    }

    struct ChatControlA11yProbe {
        kind: ChatControlKind,
        captured: Arc<Mutex<Option<CapturedControlA11y>>>,
    }

    impl Render for ChatControlA11yProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            let kind = self.kind;
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let button = match kind {
                        ChatControlKind::Retry => {
                            retry_button(&"chat".into(), &"retry-me".into(), cx)
                        }
                        ChatControlKind::Jump => jump_to_latest_button(
                            &"chat".into(),
                            "Jump to latest, 2 unread messages".into(),
                            cx,
                        ),
                    };
                    let element = button
                        .on_click(|_: &gpui::ClickEvent, _, _| {})
                        .render(window, cx)
                        .into_element();
                    let role = element.a11y_role();
                    let mut node = accesskit::Node::new(Role::Unknown);
                    element.write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") =
                        Some(CapturedControlA11y { role, node });
                },
                |_, _, _, _| {},
            )
        }
    }

    fn capture_chat_control(kind: ChatControlKind, cx: &mut TestAppContext) -> CapturedControlA11y {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ChatControlA11yProbe { kind, captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("chat control should be captured")
    }

    #[test]
    fn controlled_snapshots_require_unique_stable_message_ids() {
        let valid = [message(1), message(2)];
        assert!(message_ids_are_unique(&valid));

        let duplicate = [message(1), message(1)];
        assert!(!message_ids_are_unique(&duplicate));
    }

    #[gpui::test]
    fn chat_owned_controls_expose_production_role_name_and_click_action(cx: &mut TestAppContext) {
        let retry = capture_chat_control(ChatControlKind::Retry, cx);
        assert_eq!(retry.role, Some(Role::Button));
        assert_eq!(retry.node.label(), Some("Retry message"));
        assert!(retry.node.supports_action(accesskit::Action::Click));

        let jump = capture_chat_control(ChatControlKind::Jump, cx);
        assert_eq!(jump.role, Some(Role::Button));
        assert_eq!(jump.node.label(), Some("Jump to latest, 2 unread messages"));
        assert!(jump.node.supports_action(accesskit::Action::Click));
    }

    #[gpui::test]
    fn trailing_filled_message_moves_a_bounded_bubble_surface(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(
            &harness,
            Arc::from([
                ChatMessage::new(
                    "assistant",
                    ChatRole::Assistant,
                    Progressive::complete("A concise answer".to_owned()),
                )
                .with_appearance(ChatMessageAppearance::new(
                    MessageAlignment::Leading,
                    MessageBubble::Plain,
                )),
                ChatMessage::new(
                    "user",
                    ChatRole::User,
                    Progressive::complete("A short question".to_owned()),
                )
                .with_appearance(ChatMessageAppearance::new(
                    MessageAlignment::Trailing,
                    MessageBubble::Filled,
                )),
            ]),
            cx,
        );
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let row = cx
            .debug_bounds("chat-message-user")
            .expect("the user message row should render");
        let bubble = cx
            .debug_bounds("chat-message-bubble-user")
            .expect("the user bubble should render");
        let left_space = bubble.left() - row.left();
        let right_space = row.right() - bubble.right();

        assert!(
            bubble.size.width < row.size.width,
            "row={row:?}, bubble={bubble:?}"
        );
        assert!(left_space > right_space, "row={row:?}, bubble={bubble:?}");
        assert!(bubble.right() <= row.right());
    }

    #[gpui::test]
    fn prepend_preserves_the_prior_top_message_and_pixel_offset(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(10..70), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 20,
                offset_in_item: px(7.),
            });
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let before = chat.read_with(cx, |chat, _| top_anchor(chat));

        set_messages(&harness, messages(0..70), cx);

        assert_eq!(chat.read_with(cx, |chat, _| top_anchor(chat)), before);
        assert_eq!(
            chat.read_with(cx, |chat, _| chat.unread_count()),
            0,
            "older prepended history is not unread"
        );
    }

    #[gpui::test]
    fn zooming_re_measures_the_transcript_and_keeps_following_the_tail(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..60), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        assert!(chat.read_with(cx, |chat, _| chat.is_pinned_to_bottom()));

        // 100%, 150%, 200% of the 16px base.
        for font_size in [16., 24., 32.] {
            zoom_to(cx, font_size);

            chat.read_with(cx, |chat, _| {
                assert!(
                    chat.resolved_layout.matches(px(font_size)),
                    "Chat must notice {font_size}px type from its own render"
                );
                assert!(
                    chat.is_pinned_to_bottom(),
                    "a transcript already following the tail keeps following it at \
                     {font_size}px type"
                );
            });
            assert!(
                cx.debug_bounds("chat-message-m0059").is_some(),
                "the latest message stays reachable at {font_size}px type"
            );
        }
    }

    #[gpui::test]
    fn zooming_preserves_the_first_visible_message_when_not_following(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..60), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 12,
                offset_in_item: px(0.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let anchor = chat
            .read_with(cx, |chat, _| top_anchor(chat))
            .expect("the transcript should rest on a message")
            .0;

        for font_size in [16., 24., 32.] {
            zoom_to(cx, font_size);

            chat.read_with(cx, |chat, _| {
                assert!(chat.resolved_layout.matches(px(font_size)));
                assert_eq!(
                    top_anchor(chat).map(|(id, _)| id),
                    Some(anchor.clone()),
                    "the message that was first on screen stays first at {font_size}px type"
                );
                assert!(
                    !chat.is_pinned_to_bottom(),
                    "zooming must not jump a reader who had scrolled back to the tail"
                );
            });
            assert!(
                cx.debug_bounds("chat-message-m0012").is_some(),
                "the anchored message is still drawn at {font_size}px type"
            );
        }
    }

    #[gpui::test]
    fn append_follows_latest_only_while_pinned(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..40), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        assert!(chat.read_with(cx, |chat, _| chat.list_state.is_following_tail()));

        set_messages(&harness, messages(0..41), cx);

        assert!(chat.read_with(cx, |chat, _| chat.list_state.is_following_tail()));
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 0);
        assert!(cx.debug_bounds("chat-message-m0040").is_some());
    }

    #[gpui::test]
    fn offscreen_append_increments_unread_without_moving_the_anchor(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..60), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 12,
                offset_in_item: px(5.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let before = chat.read_with(cx, |chat, _| top_anchor(chat));

        set_messages(&harness, messages(0..63), cx);

        assert_eq!(chat.read_with(cx, |chat, _| top_anchor(chat)), before);
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 3);
        assert!(cx.debug_bounds("chat-jump-latest").is_some());
        assert!(cx.debug_bounds("chat-message-m0062").is_none());

        let jump = cx
            .debug_bounds("chat-jump-latest")
            .expect("named jump action should remain reachable");
        cx.simulate_click(jump.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 0);
        assert!(cx.debug_bounds("chat-message-m0062").is_some());
        harness.read_with(cx, |harness, _| {
            assert!(
                harness.events.borrow().contains(&ChatEvent::JumpedToLatest),
                "jumping should preserve the typed intent"
            );
        });
    }

    #[gpui::test]
    fn unread_reconciles_removed_messages_and_targets_the_latest_retained_id(
        cx: &mut TestAppContext,
    ) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..60), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 12,
                offset_in_item: px(5.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        set_messages(&harness, messages(0..63), cx);
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 3);

        set_messages(&harness, messages(0..62), cx);
        assert_eq!(
            chat.read_with(cx, |chat, _| chat.unread_count()),
            2,
            "removed unread IDs must leave the unread set"
        );
        let jump = cx
            .debug_bounds("chat-jump-latest")
            .expect("remaining unread messages should keep the jump action reachable");
        cx.simulate_click(jump.center(), Modifiers::default());
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("chat-message-m0061").is_some());
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 0);
    }

    #[gpui::test]
    fn clearing_the_conversation_clears_unread_state_and_jump_action(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..60), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 12,
                offset_in_item: px(5.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        set_messages(&harness, messages(0..63), cx);
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 3);

        set_messages(&harness, Arc::from([]), cx);

        assert!(chat.read_with(cx, |chat, _| chat.messages().is_empty()));
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 0);
        assert!(cx.debug_bounds("chat-jump-latest").is_none());

        set_messages(&harness, messages(100..101), cx);
        assert!(chat.read_with(cx, |chat, _| chat.is_pinned_to_bottom()));
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 0);
        assert!(cx.debug_bounds("chat-message-m0100").is_some());
    }

    #[gpui::test]
    fn streaming_growth_and_width_change_preserve_the_logical_anchor(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        let mut initial = (0..80).map(message).collect::<Vec<_>>();
        initial[24] = ChatMessage::new(
            "stream",
            ChatRole::Assistant,
            Progressive::running("A short streamed answer".to_owned()),
        );
        set_messages(&harness, Arc::from(initial.clone()), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 24,
                offset_in_item: px(6.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let before = chat.read_with(cx, |chat, _| top_anchor(chat));

        let mut grown = initial;
        let mut content = Progressive::running("A short streamed answer".to_owned());
        content.append(
            " that grows across several wrapped lines while the user reads it above the fold.",
        );
        grown[24] = ChatMessage::new("stream", ChatRole::Assistant, content);
        set_messages(&harness, Arc::from(grown), cx);
        assert_eq!(chat.read_with(cx, |chat, _| top_anchor(chat)), before);

        cx.simulate_resize(size(px(360.), px(420.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert_eq!(chat.read_with(cx, |chat, _| top_anchor(chat)), before);
    }

    #[gpui::test]
    fn one_thousand_messages_render_only_the_visible_stable_range(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..1_000), cx);

        assert!(cx.debug_bounds("chat-message-m0999").is_some());
        assert!(cx.debug_bounds("chat-message-m0000").is_none());

        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 500,
                offset_in_item: px(0.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert!(cx.debug_bounds("chat-message-m0500").is_some());
        assert!(cx.debug_bounds("chat-message-m0000").is_none());
        assert!(cx.debug_bounds("chat-message-m0999").is_none());
    }

    #[gpui::test]
    fn focused_virtual_message_row_survives_scroll_and_stable_replacement(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        let mut snapshot = (0..999).map(message).collect::<Vec<_>>();
        snapshot.push(
            ChatMessage::new(
                "retry-me",
                ChatRole::Assistant,
                Progressive::failed("Could not finish".to_owned(), "Network unavailable"),
            )
            .retryable(true),
        );
        set_messages(&harness, Arc::from(snapshot.clone()), cx);

        cx.update(|window, cx| window.focus_next(cx));
        activate_key(cx, "enter");
        harness.read_with(cx, |harness, _| {
            assert!(harness.events.borrow().iter().any(|event| matches!(
                event,
                ChatEvent::RetryRequested { message_id } if message_id == "retry-me"
            )));
            harness.events.borrow_mut().clear();
        });

        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 0,
                offset_in_item: px(3.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        snapshot.insert(0, message(2_000));
        set_messages(&harness, Arc::from(snapshot), cx);
        assert!(cx.debug_bounds("chat-message-m0000").is_some());
        assert!(cx.debug_bounds("chat-message-retry-me").is_some());

        activate_key(cx, "enter");
        harness.read_with(cx, |harness, _| {
            assert!(harness.events.borrow().iter().any(|event| matches!(
                event,
                ChatEvent::RetryRequested { message_id } if message_id == "retry-me"
            )));
        });
    }

    #[gpui::test]
    fn message_actions_and_keyboard_composer_forward_stable_typed_events(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        let failed = ChatMessage::new(
            "retry-me",
            ChatRole::Assistant,
            Progressive::failed("Could not finish".to_owned(), "Network unavailable"),
        )
        .retryable(true);
        let answered = ChatMessage::new(
            "answer",
            ChatRole::Assistant,
            Progressive::complete("Use [[cite:pricing]] for the comparison.".to_owned()),
        )
        .citations([CitationRef::new(
            "pricing",
            "Pricing",
            "Open pricing",
            "app://pricing",
        )])
        .follow_ups([FollowUp::new("compare", "Compare suppliers")]);
        set_messages(&harness, Arc::from(vec![failed, answered]), cx);

        let retry = cx
            .debug_bounds("chat-retry-retry-me")
            .expect("retry action should render");
        cx.simulate_click(retry.center(), Modifiers::default());
        let citation = cx
            .debug_bounds("streaming-citation-pricing")
            .expect("citation action should render");
        cx.simulate_click(citation.center(), Modifiers::default());

        let follow_up = cx
            .debug_bounds("streaming-follow-up-compare")
            .expect("follow-up action should render");
        cx.simulate_click(follow_up.center(), Modifiers::default());

        #[cfg(not(target_os = "macos"))]
        {
            let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
            let prompt = chat.read_with(cx, |chat, _| chat.prompt_bar.clone());
            cx.update(|window, cx| {
                prompt.update(cx, |prompt, cx| {
                    prompt.set_draft("Send from chat", window, cx);
                    prompt.focus(window, cx);
                });
            });
            cx.simulate_keystrokes("enter");
        }

        harness.read_with(cx, |harness, _| {
            let events = harness.events.borrow();
            assert!(events.iter().any(|event| matches!(
                event,
                ChatEvent::RetryRequested { message_id } if message_id == "retry-me"
            )));
            assert!(events.iter().any(|event| matches!(
                event,
                ChatEvent::FollowUpSelected { message_id, follow_up_id }
                    if message_id == "answer" && follow_up_id == "compare"
            )));
            assert!(events.iter().any(|event| matches!(
                event,
                ChatEvent::CitationActivated { message_id, citation_id, destination }
                    if message_id == "answer"
                        && citation_id == "pricing"
                        && destination == "app://pricing"
            )));
            #[cfg(not(target_os = "macos"))]
            assert!(events.iter().any(|event| matches!(
                event,
                ChatEvent::Prompt(PromptBarEvent::Submit { submission, .. })
                    if submission.text() == "Send from chat"
            )));
        });
    }

    #[gpui::test]
    fn rendered_follow_up_activates_from_keyboard_with_stable_ids(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(
            &harness,
            Arc::from([ChatMessage::new(
                "answer",
                ChatRole::Assistant,
                Progressive::complete("Choose the next comparison.".to_owned()),
            )
            .follow_ups([FollowUp::new("compare", "Compare suppliers")])]),
            cx,
        );

        cx.update(|window, cx| window.focus_next(cx));
        activate_key(cx, "enter");

        harness.read_with(cx, |harness, _| {
            assert!(harness.events.borrow().iter().any(|event| matches!(
                event,
                ChatEvent::FollowUpSelected { message_id, follow_up_id }
                    if message_id == "answer" && follow_up_id == "compare"
            )));
        });
    }

    #[gpui::test]
    fn switching_to_a_disjoint_conversation_is_not_a_burst_of_arrivals(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..12), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());

        // Read back through the transcript, so the next snapshot is not a
        // tail-following append.
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 2,
                offset_in_item: px(0.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        // A different conversation: no message identity in common.
        set_messages(&harness, messages(100..112), cx);
        cx.update(|window, cx| window.draw(cx).clear(cx));

        chat.read_with(cx, |chat, _| {
            assert_eq!(
                chat.unread_count(),
                0,
                "opening another conversation must not mark its history unread"
            );
            assert!(
                chat.arrivals.is_empty(),
                "a replacement is not twelve arrivals, so nothing should animate in or cue"
            );
            assert!(
                chat.pinned_to_bottom,
                "a different conversation opens at its end, not at the old scroll offset"
            );
        });
    }

    #[gpui::test]
    fn per_message_state_does_not_outlive_its_message(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..6), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());

        cx.update(|window, cx| {
            chat.update(cx, |chat, cx| {
                chat.feedback.insert("m0001".into(), true);
                chat.copied_message = Some("m0002".into());
                chat.begin_edit("m0003", window, cx);
            });
        });
        chat.read_with(cx, |chat, _| {
            assert!(chat.editing.is_some(), "the edit session must start");
        });

        set_messages(&harness, messages(100..106), cx);

        chat.read_with(cx, |chat, _| {
            assert!(
                chat.feedback.is_empty(),
                "feedback keyed by message must not accumulate across conversations"
            );
            assert_eq!(
                chat.copied_message, None,
                "the copied marker belonged to a message that is gone"
            );
            assert!(
                chat.editing.is_none(),
                "an open editor must not keep pointing at a removed message"
            );
        });
    }

    #[gpui::test]
    fn removing_the_edited_message_moves_focus_off_the_vanishing_editor(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..6), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());

        cx.update(|window, cx| {
            chat.update(cx, |chat, cx| chat.begin_edit("m0003", window, cx));
        });
        let editor = chat.read_with(cx, |chat, _| {
            chat.editing
                .as_ref()
                .expect("the edit session must start")
                .editor
                .clone()
        });
        cx.update(|window, cx| {
            assert!(
                editor.read(cx).focus_handle(cx).is_focused(window),
                "beginning an edit focuses its editor"
            );
        });

        // The application swaps in a conversation without the edited message.
        set_messages(&harness, messages(100..106), cx);

        cx.update(|window, cx| {
            assert!(
                !editor.read(cx).focus_handle(cx).is_focused(window),
                "focus must not stay on an editor that no longer exists"
            );
            let prompt = chat.read(cx).prompt_bar.clone();
            assert!(
                prompt.read(cx).focus_handle(cx).is_focused(window),
                "focus belongs on the composer, which is still there"
            );
        });
    }

    #[gpui::test]
    fn rendered_jump_to_latest_activates_from_keyboard(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        set_messages(&harness, messages(0..60), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| {
            chat.list_state.scroll_to(ListOffset {
                item_ix: 12,
                offset_in_item: px(5.),
            });
            chat.pinned_to_bottom = false;
            cx.notify();
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        set_messages(&harness, messages(0..62), cx);

        cx.update(|window, cx| window.focus_next(cx));
        activate_key(cx, "enter");
        cx.update(|window, cx| window.draw(cx).clear(cx));

        assert!(cx.debug_bounds("chat-message-m0061").is_some());
        assert_eq!(chat.read_with(cx, |chat, _| chat.unread_count()), 0);
        harness.read_with(cx, |harness, _| {
            assert!(
                harness.events.borrow().contains(&ChatEvent::JumpedToLatest),
                "keyboard activation should emit the typed jump intent"
            );
        });
    }

    #[gpui::test]
    fn constrained_chat_keeps_the_latest_message_and_composer_reachable(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        cx.simulate_resize(size(px(360.), px(300.)));
        set_messages(&harness, messages(0..80), cx);
        let chat = harness.read_with(cx, |harness, _| harness.chat.clone());
        chat.update(cx, |chat, cx| chat.scroll_to_latest(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let transcript = cx
            .debug_bounds("chat-transcript")
            .expect("transcript should render");
        let latest = cx
            .debug_bounds("chat-message-m0079")
            .expect("latest message should render");
        let composer = cx
            .debug_bounds("chat-composer")
            .expect("composer should render");
        assert!(latest.bottom() <= transcript.bottom());
        assert!(composer.bottom() <= px(300.));
    }

    #[test]
    fn failed_message_state_is_not_inferred_from_color() {
        let failed = ChatMessage::new(
            "failed",
            ChatRole::Assistant,
            Progressive::failed(String::new(), "Unavailable"),
        );
        assert!(matches!(
            failed.content().state(),
            ProgressState::Failed(reason) if reason == "Unavailable"
        ));
    }

    #[test]
    fn chat_log_list_and_message_expose_direct_named_semantics() {
        let chat = chat_frame(&"semantic-chat".into()).into_element();
        let mut chat_node = accesskit::Node::new(Role::Unknown);
        chat.write_a11y_info(&mut chat_node);
        assert_eq!(chat.a11y_role(), Some(Role::Log));
        assert_eq!(chat_node.author_id(), Some("chat.semantic-chat"));
        assert_eq!(chat_node.label(), Some("Conversation"));

        let transcript = transcript_frame("transcript".into()).into_element();
        let mut transcript_node = accesskit::Node::new(Role::Unknown);
        transcript.write_a11y_info(&mut transcript_node);
        assert_eq!(transcript.a11y_role(), Some(Role::List));
        assert_eq!(transcript_node.label(), Some("Messages"));

        let message = ChatMessage::new(
            "semantic-message",
            ChatRole::Assistant,
            Progressive::failed(String::new(), "Network unavailable"),
        )
        .author("Mighty");
        let row = message_frame(&"semantic-chat".into(), &message).into_element();
        let mut row_node = accesskit::Node::new(Role::Unknown);
        row.write_a11y_info(&mut row_node);
        assert_eq!(row.a11y_role(), Some(Role::ListItem));
        assert_eq!(row_node.author_id(), Some("chat.message.semantic-message"));
        assert_eq!(row_node.label(), Some("Mighty, Assistant message"));
        assert_eq!(row_node.description(), Some("Failed: Network unavailable"));
    }
}
