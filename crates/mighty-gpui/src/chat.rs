//! Controlled, virtualized chat transcript composed from mighty-gpui primitives.

use crate::{
    control::outlined_control,
    prompt_bar::{PromptBar, PromptBarEvent},
    stream::{ProgressState, StreamedContent},
    streaming_text::{CitationRef, FollowUp, StreamingText, StreamingTextEvent},
    theme::SemanticStyledExt as _,
};
use gpui::{
    AnyElement, App, Context, ElementId, Entity, EventEmitter, FocusHandle, FollowMode,
    InteractiveElement as _, IntoElement as _, ListAlignment, ListOffset, ListState,
    ParentElement as _, Render, Role, SharedString, Stateful, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, div, list, prelude::FluentBuilder as _, px,
};
use gpui_base::Button;
use gpui_component::{ActiveTheme as _, scroll::ScrollableElement as _, text::TextView, v_flex};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::Arc,
};

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
    retryable: bool,
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
            retryable: false,
        }
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
    /// The user activated an inline citation attached to a message.
    CitationActivated {
        /// Stable message identifier.
        message_id: SharedString,
        /// Stable citation identifier.
        citation_id: SharedString,
        /// Opaque application-owned destination.
        destination: SharedString,
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
    visible_range: Range<usize>,
    pinned_to_bottom: bool,
    unread_message_ids: HashSet<SharedString>,
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
            visible_range: 0..0,
            pinned_to_bottom: true,
            unread_message_ids: HashSet::new(),
            _prompt_subscription: prompt_subscription,
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
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if Arc::ptr_eq(&self.messages, &messages) || self.messages.as_ref() == messages.as_ref() {
            return;
        }
        if !message_ids_are_unique(&messages) {
            return;
        }

        let was_following = self.list_state.is_following_tail() && self.pinned_to_bottom;
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
        let appended_ids = messages
            .iter()
            .skip(last_retained_index.map_or(0, |index| index + 1))
            .filter(|message| !old_by_id.contains_key(&message.id))
            .map(|message| message.id.clone())
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
        self.messages = messages;
        if self.messages.is_empty() {
            self.list_state.set_follow_mode(FollowMode::Tail);
            self.list_state.scroll_to_end();
            self.unread_message_ids.clear();
            self.pinned_to_bottom = true;
        } else if was_following {
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
        }
    }

    fn retry(&mut self, message_id: SharedString, cx: &mut Context<Self>) {
        cx.emit(ChatEvent::RetryRequested { message_id });
    }

    fn render_message(
        &mut self,
        index: usize,
        _window: &mut Window,
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

        message_frame(&self.id, &message)
            .track_focus(&message_focus_handle)
            .gap(tokens.spacing.sm)
            .px(tokens.spacing.md)
            .py(tokens.spacing.md)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .bg(match message.role {
                ChatRole::User => cx.theme().secondary,
                ChatRole::Assistant | ChatRole::System | ChatRole::Tool => cx.theme().background,
            })
            .child(
                div()
                    .id(heading_id)
                    .role(Role::Heading)
                    .aria_label(author.clone())
                    .text_token(tokens.typography.xs)
                    .text_color(cx.theme().muted_foreground)
                    .child(author),
            )
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
            .into_any_element()
    }
}

impl EventEmitter<ChatEvent> for Chat {}

impl Render for Chat {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
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
                    .flex_1()
                    .overflow_hidden()
                    .when(self.messages.is_empty(), |this| {
                        this.child(
                            div()
                                .id((ElementId::from(self.id.clone()), "empty"))
                                .role(Role::Status)
                                .aria_label("No messages yet")
                                .p(tokens.spacing.md)
                                .text_token(tokens.typography.sm)
                                .text_color(cx.theme().muted_foreground)
                                .child("No messages yet"),
                        )
                    })
                    .when(!self.messages.is_empty(), |this| {
                        this.child(
                            list(self.list_state.clone(), cx.processor(Self::render_message))
                                .size_full(),
                        )
                        .vertical_scrollbar(&self.list_state)
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
        AppContext as _, Context, Element as _, Entity, KeyDownEvent, KeyUpEvent, Keystroke,
        ListOffset, Modifiers, Render, RenderOnce as _, Role, SharedString, Subscription,
        TestAppContext, VisualTestContext, Window, accesskit, canvas, px, size,
    };
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
