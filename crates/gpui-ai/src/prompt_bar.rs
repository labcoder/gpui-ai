//! Hybrid-controlled prompt composition with native GPUI text editing.

use crate::attachment::{AttachmentEvent, AttachmentStrip};
use crate::context_meter::format_tokens;
use crate::control::composed_button;
use crate::cues::{self, Cue};
use crate::stream::ProgressState;
use crate::surface::{eyebrow, meta};
use crate::theme::SemanticStyledExt as _;
use gpui::{
    AnyElement, App, AppContext as _, Bounds, Div, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Pixels,
    Render, Role, SharedString, Stateful, StatefulInteractiveElement as _, Styled, Subscription,
    Window, deferred, div, prelude::FluentBuilder as _,
};
use gpui_base::{Align, Button, POPUP_PRIORITY, Placement, Positioner};
use gpui_component::{
    ActiveTheme as _, ElementExt as _, Icon, IconName, Sizable as _, ThemeStyled as _, h_flex,
    input::{
        Enter, Escape, InputEvent, MoveDown, MoveEnd, MoveHome, MoveUp, Textarea, TextareaState,
    },
    scroll::ScrollableElement as _,
    v_flex,
};
use std::collections::HashSet;
use std::ops::Range;

/// A selectable model offered by a [`PromptBar`].
///
/// Beyond its stable ID and label a model may carry a provider (options are
/// grouped by it), a one-line description, and its context window, so the
/// picker reads like a catalog rather than a bare list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptModel {
    id: SharedString,
    label: SharedString,
    provider: Option<SharedString>,
    description: Option<SharedString>,
    context_window: Option<u64>,
    disabled: bool,
}

impl PromptModel {
    /// Creates an enabled model with stable identity and a visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            provider: None,
            description: None,
            context_window: None,
            disabled: false,
        }
    }

    /// Sets the provider the model is grouped under (for example "Anthropic").
    pub fn provider(mut self, provider: impl Into<SharedString>) -> Self {
        self.provider = Some(provider.into());
        self
    }

    /// Adds a one-line description shown under the label.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Records the context window in tokens, shown compactly (`200K`).
    pub fn context_window(mut self, tokens: u64) -> Self {
        self.context_window = Some(tokens);
        self
    }

    /// Sets whether the model can be selected.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Returns the provider name, when set.
    pub fn provider_name(&self) -> Option<&SharedString> {
        self.provider.as_ref()
    }

    /// Returns the description, when set.
    pub fn description_text(&self) -> Option<&SharedString> {
        self.description.as_ref()
    }

    /// Returns the context window in tokens, when set.
    pub fn context_window_tokens(&self) -> Option<u64> {
        self.context_window
    }

    /// Returns the stable model identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible model label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }
}

/// One `@`-mention suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptMention {
    id: SharedString,
    label: SharedString,
}

impl PromptMention {
    /// Creates a mention with stable identity and insertion text.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    /// Returns the stable mention identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible mention label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }
}

/// One `/` command suggestion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCommand {
    id: SharedString,
    label: SharedString,
    description: Option<SharedString>,
}

impl PromptCommand {
    /// Creates a command with stable identity and insertion text.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
        }
    }

    /// Adds secondary text used by the suggestion filter and accessible name.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Returns the stable command identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible command label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }
}

/// The composer's attachment type: the shared [`Attachment`](crate::attachment::Attachment) so a file keeps
/// one identity, kind, and thumbnail from the composer into the message.
pub use crate::attachment::Attachment as PromptAttachment;

/// A trimmed prompt snapshot emitted for application-owned submission work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptSubmission {
    text: SharedString,
    model_id: Option<SharedString>,
    attachment_ids: Vec<SharedString>,
}

impl PromptSubmission {
    /// Returns the submitted prompt text.
    pub fn text(&self) -> &SharedString {
        &self.text
    }

    /// Returns the selected model, when one is configured.
    pub fn model_id(&self) -> Option<&SharedString> {
        self.model_id.as_ref()
    }

    /// Returns stable IDs for the attachments present at submission time.
    pub fn attachment_ids(&self) -> &[SharedString] {
        &self.attachment_ids
    }
}

/// An interaction emitted by [`PromptBar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptBarEvent {
    /// The native editor draft changed.
    DraftChanged {
        /// Stable prompt-bar identifier.
        id: SharedString,
        /// Complete current draft.
        draft: SharedString,
    },
    /// The user submitted a non-empty prompt.
    Submit {
        /// Stable prompt-bar identifier.
        id: SharedString,
        /// Immutable submission snapshot.
        submission: PromptSubmission,
    },
    /// The user requested cancellation of running work.
    CancelRequested {
        /// Stable prompt-bar identifier.
        id: SharedString,
    },
    /// The user selected a model.
    ModelChanged {
        /// Stable prompt-bar identifier.
        id: SharedString,
        /// Stable selected-model identifier.
        model_id: SharedString,
    },
    /// The user inserted a mention.
    MentionSelected {
        /// Stable prompt-bar identifier.
        id: SharedString,
        /// Stable selected-mention identifier.
        mention_id: SharedString,
    },
    /// The user inserted a command.
    CommandSelected {
        /// Stable prompt-bar identifier.
        id: SharedString,
        /// Stable selected-command identifier.
        command_id: SharedString,
    },
    /// The user requested an attachment picker.
    AttachRequested {
        /// Stable prompt-bar identifier.
        id: SharedString,
    },
    /// The user removed an attachment.
    AttachmentRemoved {
        /// Stable prompt-bar identifier.
        id: SharedString,
        /// Stable removed-attachment identifier.
        attachment_id: SharedString,
    },
    /// The user requested application-owned prompt enhancement.
    EnhanceRequested {
        /// Stable prompt-bar identifier.
        id: SharedString,
        /// Complete draft to enhance.
        draft: SharedString,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PromptTokenKind {
    Mention,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PromptToken {
    kind: PromptTokenKind,
    range: Range<usize>,
    query: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SuggestionKey {
    Mention(SharedString),
    Command(SharedString),
}

fn clipped_char_boundary(text: &str, offset: usize) -> usize {
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn active_prompt_token(draft: &str, cursor: usize) -> Option<PromptToken> {
    let cursor = clipped_char_boundary(draft, cursor);
    let prefix = &draft[..cursor];
    let start = prefix
        .char_indices()
        .rev()
        .find_map(|(offset, character)| {
            character
                .is_whitespace()
                .then_some(offset + character.len_utf8())
        })
        .unwrap_or(0);
    let token = &draft[start..cursor];
    let (kind, query) = if let Some(query) = token.strip_prefix('@') {
        (PromptTokenKind::Mention, query)
    } else {
        let query = token.strip_prefix('/')?;
        (PromptTokenKind::Command, query)
    };
    let end = draft[cursor..]
        .char_indices()
        .find_map(|(offset, character)| character.is_whitespace().then_some(cursor + offset))
        .unwrap_or(draft.len());
    (!query.chars().any(char::is_whitespace)).then(|| PromptToken {
        kind,
        range: start..end,
        query: query.to_owned(),
    })
}

fn retain_active_suggestion(
    previous: Option<SuggestionKey>,
    filtered: &[SuggestionKey],
) -> Option<SuggestionKey> {
    previous
        .filter(|candidate| filtered.contains(candidate))
        .or_else(|| filtered.first().cloned())
}

fn build_submission(
    draft: &str,
    model_id: Option<SharedString>,
    attachments: &[PromptAttachment],
) -> Option<PromptSubmission> {
    let text = draft.trim();
    (!text.is_empty()).then(|| PromptSubmission {
        text: text.to_owned().into(),
        model_id,
        attachment_ids: attachments.iter().map(|item| item.id().clone()).collect(),
    })
}

fn prompt_frame(id: &SharedString) -> Stateful<Div> {
    v_flex()
        .id(id.clone())
        .role(Role::Group)
        .aria_label("Prompt composer")
        .w_full()
        .min_w_0()
}

fn prompt_listbox(id: ElementId, label: &'static str) -> Stateful<Div> {
    v_flex().id(id).role(Role::ListBox).aria_label(label)
}

fn prompt_status(id: ElementId, label: SharedString) -> Stateful<Div> {
    div().id(id).role(Role::Status).aria_label(label)
}

fn prompt_control(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut App,
) -> Button {
    prompt_control_with_tone(id, label, false, cx)
}

fn prompt_primary_control(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut App,
) -> Button {
    prompt_control_with_tone(id, label, true, cx)
}

fn prompt_model_control(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    expanded: bool,
    cx: &mut App,
) -> Button {
    let tokens = cx.theme().semantic_tokens();
    let label = label.into();
    let visible = label
        .strip_prefix("Model: ")
        .map(|visible| SharedString::from(visible.to_owned()))
        .unwrap_or_else(|| label.clone());
    prompt_option(
        id,
        label,
        h_flex()
            .items_center()
            .gap(tokens.spacing.xs)
            .child(
                Icon::new(IconName::Cpu)
                    .xsmall()
                    .text_color(cx.theme().muted_foreground),
            )
            .child(visible)
            .child(
                Icon::new(if expanded {
                    IconName::ChevronUp
                } else {
                    IconName::ChevronDown
                })
                .xsmall()
                .text_color(cx.theme().muted_foreground),
            ),
        cx,
    )
    .selected(expanded)
    .aria_expanded(expanded)
}

/// A menu row with custom content and the same geometry as [`prompt_control`].
fn prompt_option(
    id: impl Into<ElementId>,
    accessibility_label: impl Into<SharedString>,
    content: impl IntoElement,
    cx: &mut App,
) -> Button {
    let tokens = cx.theme().semantic_tokens();
    composed_button(id, accessibility_label)
        .flex()
        .items_center()
        .justify_start()
        .px(tokens.spacing.sm)
        .py(tokens.spacing.xs)
        .border_1()
        .border_color(cx.theme().transparent)
        .rounded(tokens.radius.sm)
        .bg(cx.theme().transparent)
        .text_token(tokens.typography.sm)
        .text_color(cx.theme().foreground)
        .hover(|style| style.bg(cx.theme().button_hover))
        .active(|style| style.bg(cx.theme().button_active))
        .focus_visible(|style| style.border_color(cx.theme().ring))
        .styles(|styles| {
            styles.disabled(|style| {
                style
                    .bg(cx.theme().muted)
                    .text_color(cx.theme().muted_foreground)
            })
        })
        .child(content)
}

/// Models grouped by provider in first-appearance order; ungrouped models
/// keep their place under a `None` heading.
fn model_groups(models: &[PromptModel]) -> Vec<(Option<SharedString>, Vec<&PromptModel>)> {
    let mut groups: Vec<(Option<SharedString>, Vec<&PromptModel>)> = Vec::new();
    for model in models {
        match groups
            .iter_mut()
            .find(|(provider, _)| *provider == model.provider)
        {
            Some((_, members)) => members.push(model),
            None => groups.push((model.provider.clone(), vec![model])),
        }
    }
    groups
}

fn retain_active_model(
    previous: Option<SharedString>,
    selected: Option<&SharedString>,
    models: &[PromptModel],
) -> Option<SharedString> {
    previous
        .filter(|candidate| {
            models
                .iter()
                .any(|model| &model.id == candidate && !model.disabled)
        })
        .or_else(|| {
            selected
                .filter(|selected| {
                    models
                        .iter()
                        .any(|model| &model.id == *selected && !model.disabled)
                })
                .cloned()
        })
        .or_else(|| {
            models
                .iter()
                .find(|model| !model.disabled)
                .map(|model| model.id.clone())
        })
}

fn prompt_control_with_tone(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    primary: bool,
    cx: &mut App,
) -> Button {
    let tokens = cx.theme().semantic_tokens();
    let label = label.into();
    let (background, foreground, border, hover, active) = if primary {
        (
            cx.theme().button_primary,
            cx.theme().button_primary_foreground,
            cx.theme().primary,
            cx.theme().button_primary_hover,
            cx.theme().button_primary_active,
        )
    } else {
        (
            cx.theme().transparent,
            cx.theme().foreground,
            cx.theme().transparent,
            cx.theme().button_hover,
            cx.theme().button_active,
        )
    };
    composed_button(id, label.clone())
        .flex()
        .items_center()
        .justify_center()
        .px(tokens.spacing.sm)
        .py(tokens.spacing.xs)
        .border_1()
        .border_color(border)
        .rounded(tokens.radius.sm)
        .bg(background)
        .text_token(tokens.typography.sm)
        .text_color(foreground)
        .hover(|style| style.bg(hover))
        .active(|style| style.bg(active))
        .focus_visible(|style| style.border_color(cx.theme().ring))
        .styles(|styles| {
            styles.disabled(|style| {
                style
                    .bg(cx.theme().muted)
                    .text_color(cx.theme().muted_foreground)
            })
        })
        .child(div().child(label))
}

/// A native, hybrid-controlled prompt composer.
///
/// The entity retains one upstream [`TextareaState`] so IME, selection, cursor,
/// clipboard, and multiline editing remain native. Applications own catalogs,
/// attachments, progress, and every asynchronous operation; the component owns
/// only transient editor, overlay, and focus state.
///
/// # Example
///
/// ```ignore
/// let prompt = cx.new(|cx| PromptBar::new("assistant-prompt", window, cx));
/// prompt.update(cx, |prompt, cx| {
///     prompt.set_models([PromptModel::new("fast", "Fast")], cx);
/// });
/// ```
pub struct PromptBar {
    id: SharedString,
    editor: Entity<TextareaState>,
    models: Vec<PromptModel>,
    selected_model: Option<SharedString>,
    mentions: Vec<PromptMention>,
    commands: Vec<PromptCommand>,
    attachments: Vec<PromptAttachment>,
    progress: ProgressState,
    last_draft: String,
    last_cursor: usize,
    token: Option<PromptToken>,
    filtered: Vec<SuggestionKey>,
    active_suggestion: Option<SuggestionKey>,
    model_menu_open: bool,
    active_model: Option<SharedString>,
    model_trigger_bounds: Bounds<Pixels>,
    model_trigger_rem_size: Pixels,
    _subscriptions: Vec<Subscription>,
}

impl PromptBar {
    /// Creates a prompt composer with one retained native textarea entity.
    pub fn new(
        id: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let editor = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 5)
                .submit_on_enter(true)
                .placeholder("Message the agent; use @ mentions or / commands")
        });
        let subscription = cx.subscribe_in(
            &editor,
            window,
            |this, editor, event: &InputEvent, window, cx| {
                this.on_input_event(editor, event, window, cx);
            },
        );
        let observation = cx.observe(&editor, |this, editor, cx| {
            let cursor = editor.read(cx).cursor();
            if this.last_cursor != cursor {
                this.last_cursor = cursor;
                this.refresh_suggestions(cx);
            }
        });
        Self {
            id: id.into(),
            editor,
            models: Vec::new(),
            selected_model: None,
            mentions: Vec::new(),
            commands: Vec::new(),
            attachments: Vec::new(),
            progress: ProgressState::Pending,
            last_draft: String::new(),
            last_cursor: 0,
            token: None,
            filtered: Vec::new(),
            active_suggestion: None,
            model_menu_open: false,
            active_model: None,
            model_trigger_bounds: Bounds::default(),
            model_trigger_rem_size: window.rem_size(),
            _subscriptions: vec![subscription, observation],
        }
    }

    /// Replaces the model catalog while preserving a still-valid selection.
    ///
    /// A catalog repeating a stable ID is ignored, atomically.
    pub fn set_models(
        &mut self,
        models: impl IntoIterator<Item = PromptModel>,
        cx: &mut gpui::Context<Self>,
    ) {
        let models: Vec<_> = models.into_iter().collect();
        if !stable_ids_are_unique(models.iter().map(PromptModel::id)) {
            return;
        }
        if self.models == models {
            return;
        }
        self.models = models;
        if self.models.is_empty() {
            self.model_menu_open = false;
        }
        if self.selected_model.as_ref().is_none_or(|selected| {
            !self
                .models
                .iter()
                .any(|model| &model.id == selected && !model.disabled)
        }) {
            self.selected_model = self
                .models
                .iter()
                .find(|model| !model.disabled)
                .map(|model| model.id.clone());
        }
        self.active_model = retain_active_model(
            self.active_model.take(),
            self.selected_model.as_ref(),
            &self.models,
        );
        cx.notify();
    }

    /// Applies a valid consumer-controlled model selection.
    pub fn set_selected_model(
        &mut self,
        model_id: impl Into<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) {
        let model_id = model_id.into();
        if self.selected_model.as_ref() == Some(&model_id)
            || !self
                .models
                .iter()
                .any(|model| model.id == model_id && !model.disabled)
        {
            return;
        }
        self.active_model = Some(model_id.clone());
        self.selected_model = Some(model_id);
        cx.notify();
    }

    /// Replaces the `@` mention catalog.
    ///
    /// A catalog repeating a stable ID is ignored, atomically.
    pub fn set_mentions(
        &mut self,
        mentions: impl IntoIterator<Item = PromptMention>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mentions: Vec<_> = mentions.into_iter().collect();
        if !stable_ids_are_unique(mentions.iter().map(PromptMention::id)) {
            return;
        }
        if self.mentions != mentions {
            self.mentions = mentions;
            if !self.refresh_suggestions(cx) {
                cx.notify();
            }
        }
    }

    /// Replaces the `/` command catalog.
    ///
    /// A catalog repeating a stable ID is ignored, atomically.
    pub fn set_commands(
        &mut self,
        commands: impl IntoIterator<Item = PromptCommand>,
        cx: &mut gpui::Context<Self>,
    ) {
        let commands: Vec<_> = commands.into_iter().collect();
        if !stable_ids_are_unique(commands.iter().map(PromptCommand::id)) {
            return;
        }
        if self.commands != commands {
            self.commands = commands;
            if !self.refresh_suggestions(cx) {
                cx.notify();
            }
        }
    }

    /// Replaces application-owned attachments.
    ///
    /// A snapshot repeating a stable ID is ignored, atomically.
    pub fn set_attachments(
        &mut self,
        attachments: impl IntoIterator<Item = PromptAttachment>,
        cx: &mut gpui::Context<Self>,
    ) {
        let attachments: Vec<_> = attachments.into_iter().collect();
        if !stable_ids_are_unique(attachments.iter().map(PromptAttachment::id)) {
            return;
        }
        if self.attachments != attachments {
            self.attachments = attachments;
            cx.notify();
        }
    }

    /// Replaces the application-owned progress snapshot.
    pub fn set_progress(&mut self, progress: ProgressState, cx: &mut gpui::Context<Self>) {
        if self.progress != progress {
            self.progress = progress;
            cx.notify();
        }
    }

    /// Replaces the native editor draft, placing the caret at its UTF-8 byte end
    /// without rebuilding the editor entity.
    pub fn set_draft(
        &mut self,
        draft: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let draft = draft.into().to_string();
        if self.last_draft == draft {
            return;
        }
        self.last_draft = draft.clone();
        let cursor = draft.len();
        self.last_cursor = cursor;
        self.editor.update(cx, |editor, cx| {
            editor.set_value(draft, window, cx);
            editor.set_selected_range(cursor..cursor, cx);
        });
        if !self.refresh_suggestions(cx) {
            cx.notify();
        }
    }

    /// Returns the complete current native-editor draft.
    pub fn draft(&self, cx: &App) -> SharedString {
        self.editor.read(cx).value().to_string().into()
    }

    /// Moves keyboard focus into the retained native textarea.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus_handle(cx).focus(window, cx);
    }

    fn toggle_model_menu(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.models.is_empty() {
            return;
        }
        self.model_menu_open = !self.model_menu_open;
        if self.model_menu_open {
            self.active_model = retain_active_model(
                self.active_model.take(),
                self.selected_model.as_ref(),
                &self.models,
            );
        }
        self.focus(window, cx);
        cx.notify();
    }

    fn close_model_menu(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if !self.model_menu_open {
            return;
        }
        self.model_menu_open = false;
        self.focus(window, cx);
        cx.notify();
    }

    fn move_active_model(&mut self, direction: isize, cx: &mut gpui::Context<Self>) {
        let enabled_models: Vec<_> = self.models.iter().filter(|model| !model.disabled).collect();
        if enabled_models.is_empty() {
            return;
        }
        let current_ix = self
            .active_model
            .as_ref()
            .and_then(|active| enabled_models.iter().position(|model| &model.id == active))
            .unwrap_or(0);
        let next_ix = if direction < 0 {
            current_ix
                .checked_sub(1)
                .unwrap_or(enabled_models.len() - 1)
        } else {
            (current_ix + 1) % enabled_models.len()
        };
        self.active_model = enabled_models.get(next_ix).map(|model| model.id.clone());
        cx.notify();
    }

    fn move_active_model_to_edge(&mut self, end: bool, cx: &mut gpui::Context<Self>) {
        self.active_model = if end {
            self.models
                .iter()
                .rev()
                .find(|model| !model.disabled)
                .map(|model| model.id.clone())
        } else {
            self.models
                .iter()
                .find(|model| !model.disabled)
                .map(|model| model.id.clone())
        };
        cx.notify();
    }

    fn confirm_active_model(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let Some(model_id) = self.active_model.clone().filter(|active| {
            self.models
                .iter()
                .any(|model| &model.id == active && !model.disabled)
        }) else {
            return;
        };
        self.model_menu_open = false;
        cx.emit(PromptBarEvent::ModelChanged {
            id: self.id.clone(),
            model_id,
        });
        self.focus(window, cx);
        cx.notify();
    }

    fn refresh_suggestions(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let token = {
            let editor = self.editor.read(cx);
            active_prompt_token(&editor.value(), editor.cursor())
        };
        let filtered: Vec<SuggestionKey> = token
            .as_ref()
            .map(|token| {
                let query = token.query.to_lowercase();
                match token.kind {
                    PromptTokenKind::Mention => self
                        .mentions
                        .iter()
                        .filter(|mention| mention.label.to_lowercase().contains(&query))
                        .map(|mention| SuggestionKey::Mention(mention.id.clone()))
                        .collect(),
                    PromptTokenKind::Command => self
                        .commands
                        .iter()
                        .filter(|command| {
                            command.label.to_lowercase().contains(&query)
                                || command.description.as_ref().is_some_and(|description| {
                                    description.to_lowercase().contains(&query)
                                })
                        })
                        .map(|command| SuggestionKey::Command(command.id.clone()))
                        .collect(),
                }
            })
            .unwrap_or_default();
        let changed = self.token != token || self.filtered != filtered;
        self.token = token;
        self.active_suggestion = retain_active_suggestion(self.active_suggestion.take(), &filtered);
        self.filtered = filtered;
        if changed {
            cx.notify();
        }
        changed
    }

    fn on_input_event(
        &mut self,
        editor: &Entity<TextareaState>,
        event: &InputEvent,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match event {
            InputEvent::Change => {
                let draft = editor.read(cx).value().to_string();
                self.last_cursor = editor.read(cx).cursor();
                self.refresh_suggestions(cx);
                if self.last_draft != draft {
                    self.last_draft = draft.clone();
                    cx.emit(PromptBarEvent::DraftChanged {
                        id: self.id.clone(),
                        draft: draft.into(),
                    });
                }
            }
            InputEvent::PressEnter { shift: false, .. } => {
                if self.model_menu_open {
                    self.confirm_active_model(window, cx);
                } else if self.token.is_some() && self.active_suggestion.is_some() {
                    self.insert_active_suggestion(window, cx);
                } else {
                    self.submit(window, cx);
                }
                cx.stop_propagation();
            }
            InputEvent::PressEnter { shift: true, .. } | InputEvent::Focus | InputEvent::Blur => {}
        }
    }

    fn move_active_suggestion(&mut self, direction: isize, cx: &mut gpui::Context<Self>) {
        if self.filtered.is_empty() {
            return;
        }
        let current = self
            .active_suggestion
            .as_ref()
            .and_then(|active| {
                self.filtered
                    .iter()
                    .position(|candidate| candidate == active)
            })
            .unwrap_or(0);
        let next = if direction < 0 {
            current.checked_sub(1).unwrap_or(self.filtered.len() - 1)
        } else {
            (current + 1) % self.filtered.len()
        };
        self.active_suggestion = self.filtered.get(next).cloned();
        cx.notify();
    }

    fn capture_suggestion_action(
        &mut self,
        direction: Option<isize>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.filtered.is_empty() {
            return;
        }
        match direction {
            Some(direction) => self.move_active_suggestion(direction, cx),
            None => {
                self.token = None;
                self.filtered.clear();
                self.active_suggestion = None;
                cx.notify();
            }
        }
        cx.stop_propagation();
    }

    fn capture_move_up(&mut self, _: &MoveUp, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.model_menu_open {
            self.move_active_model(-1, cx);
            cx.stop_propagation();
        } else {
            self.capture_suggestion_action(Some(-1), cx);
        }
    }

    fn capture_move_down(&mut self, _: &MoveDown, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.model_menu_open {
            self.move_active_model(1, cx);
            cx.stop_propagation();
        } else {
            self.capture_suggestion_action(Some(1), cx);
        }
    }

    fn capture_move_home(&mut self, _: &MoveHome, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.model_menu_open {
            self.move_active_model_to_edge(false, cx);
            cx.stop_propagation();
        }
    }

    fn capture_move_end(&mut self, _: &MoveEnd, _: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.model_menu_open {
            self.move_active_model_to_edge(true, cx);
            cx.stop_propagation();
        }
    }

    fn capture_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let handled = if self.model_menu_open {
            self.model_menu_open = false;
            true
        } else if self.token.is_some() {
            self.token = None;
            self.filtered.clear();
            self.active_suggestion = None;
            true
        } else {
            false
        };
        if handled {
            self.focus(window, cx);
            cx.notify();
            cx.stop_propagation();
        }
    }

    fn capture_enter(&mut self, action: &Enter, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if action.shift {
            return;
        }
        if self.model_menu_open {
            self.confirm_active_model(window, cx);
        } else if self.token.is_some() && self.active_suggestion.is_some() {
            self.insert_active_suggestion(window, cx);
        } else {
            self.submit(window, cx);
        }
        cx.stop_propagation();
    }

    fn insert_active_suggestion(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let (Some(token), Some(active)) = (self.token.clone(), self.active_suggestion.clone())
        else {
            return;
        };
        let separator = {
            let draft = self.editor.read(cx).value();
            if draft
                .get(token.range.end..)
                .and_then(|suffix| suffix.chars().next())
                .is_none_or(|character| !character.is_whitespace())
            {
                " "
            } else {
                ""
            }
        };
        let (replacement, event) = match active {
            SuggestionKey::Mention(id) => {
                let Some(mention) = self.mentions.iter().find(|mention| mention.id == id) else {
                    return;
                };
                (
                    format!("@{}{separator}", mention.label),
                    PromptBarEvent::MentionSelected {
                        id: self.id.clone(),
                        mention_id: mention.id.clone(),
                    },
                )
            }
            SuggestionKey::Command(id) => {
                let Some(command) = self.commands.iter().find(|command| command.id == id) else {
                    return;
                };
                (
                    format!("/{}{separator}", command.label),
                    PromptBarEvent::CommandSelected {
                        id: self.id.clone(),
                        command_id: command.id.clone(),
                    },
                )
            }
        };
        self.editor.update(cx, |editor, cx| {
            editor.set_selected_range(token.range, cx);
            editor.replace(&replacement, window, cx);
        });
        self.token = None;
        self.filtered.clear();
        self.active_suggestion = None;
        cx.emit(event);
        cx.notify();
    }

    fn submit(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) {
        if self.progress == ProgressState::Running {
            return;
        }
        let draft = self.editor.read(cx).value().to_string();
        let Some(submission) =
            build_submission(&draft, self.selected_model.clone(), &self.attachments)
        else {
            return;
        };
        cx.emit(PromptBarEvent::Submit {
            id: self.id.clone(),
            submission,
        });
        cues::emit(cx, Cue::Submitted);
        self.last_draft.clear();
        self.editor.update(cx, |editor, cx| {
            editor.set_value("", window, cx);
        });
        self.token = None;
        self.filtered.clear();
        self.active_suggestion = None;
        cx.notify();
    }

    fn suggestion_label(&self, key: &SuggestionKey) -> Option<SharedString> {
        match key {
            SuggestionKey::Mention(id) => self
                .mentions
                .iter()
                .find(|mention| &mention.id == id)
                .map(|mention| format!("@{}", mention.label).into()),
            SuggestionKey::Command(id) => self
                .commands
                .iter()
                .find(|command| &command.id == id)
                .map(|command| match &command.description {
                    Some(description) => format!("/{} — {description}", command.label).into(),
                    None => format!("/{}", command.label).into(),
                }),
        }
    }

    fn render_model_picker(
        &self,
        root_id: &SharedString,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let tokens = cx.theme().semantic_tokens();
        let mut model_options = Vec::new();
        for (provider, members) in model_groups(&self.models) {
            if let Some(provider) = provider {
                model_options.push(
                    eyebrow(provider.clone(), cx)
                        .px(tokens.spacing.sm)
                        .pt(tokens.spacing.xs)
                        .into_any_element(),
                );
            }
            for model in members {
                let model_id = model.id.clone();
                let model_selector = format!("prompt-bar-model-option-{}", model.id);
                let selected = self.selected_model.as_ref() == Some(&model.id);
                let active = self.active_model.as_ref() == Some(&model.id);
                let content = h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap(tokens.spacing.sm)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .items_start()
                            .child(div().child(model.label.clone()))
                            .when_some(model.description.clone(), |this, description| {
                                this.child(
                                    div()
                                        .w_full()
                                        .truncate()
                                        .text_token(tokens.typography.xs)
                                        .text_color(cx.theme().muted_foreground)
                                        .child(description),
                                )
                            }),
                    )
                    .when_some(model.context_window, |this, window_tokens| {
                        this.child(
                            meta(format!("{} ctx", format_tokens(window_tokens)), cx).flex_none(),
                        )
                    })
                    .when(selected, |this| {
                        this.child(
                            Icon::new(IconName::Check)
                                .xsmall()
                                .text_color(cx.theme().primary),
                        )
                    });
                model_options.push(
                    prompt_option(
                        (
                            gpui::ElementId::from(root_id.clone()),
                            format!("model-{}", model.id),
                        ),
                        model.label.clone(),
                        content,
                        cx,
                    )
                    .when_some(model.description.clone(), |button, description| {
                        button.aria_description(description)
                    })
                    .debug_selector(move || model_selector.clone())
                    .role(Role::ListBoxOption)
                    .disabled(model.disabled)
                    .selected(selected)
                    .aria_selected(selected)
                    .w_full()
                    .when(active, |button| button.bg(cx.theme().accent))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.active_model = Some(model_id.clone());
                        this.confirm_active_model(window, cx);
                    }))
                    .into_any_element(),
                );
            }
        }

        let surface = prompt_listbox(
            (gpui::ElementId::from(root_id.clone()), "models").into(),
            "Available models",
        )
        .debug_selector(|| "prompt-bar-model-picker".to_owned())
        .occlude()
        .w(tokens.spacing.xxl * 10.0)
        .max_h(tokens.spacing.xxl * 7.0)
        .overflow_y_scrollbar()
        .p(tokens.spacing.xs)
        .popover_style(cx)
        .on_mouse_down_out(cx.listener(|this, _, window, cx| {
            this.close_model_menu(window, cx);
        }))
        .children(model_options);

        deferred(
            Positioner::side(self.model_trigger_bounds)
                .placement(Placement::Bottom)
                .align(Align::Start)
                .offset(tokens.spacing.xs)
                .child(surface),
        )
        .with_priority(POPUP_PRIORITY)
        .into_any_element()
    }
}

impl EventEmitter<PromptBarEvent> for PromptBar {}

impl Focusable for PromptBar {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Render for PromptBar {
    fn render(&mut self, _: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let root_id = self.id.clone();
        let suggestions = self
            .filtered
            .iter()
            .filter_map(|key| {
                let label = self.suggestion_label(key)?;
                let key = key.clone();
                let selected = self.active_suggestion.as_ref() == Some(&key);
                Some(
                    prompt_control(
                        (
                            gpui::ElementId::from(root_id.clone()),
                            format!("suggestion-{key:?}"),
                        ),
                        label.clone(),
                        cx,
                    )
                    .role(Role::ListBoxOption)
                    .selected(selected)
                    .aria_selected(selected)
                    .w_full()
                    .when(selected, |button| button.bg(cx.theme().accent))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.active_suggestion = Some(key.clone());
                        this.insert_active_suggestion(window, cx);
                    })),
                )
            })
            .collect::<Vec<_>>();
        let attachments = self.attachments.clone();
        let selected_model_label: SharedString = self
            .selected_model
            .as_ref()
            .and_then(|selected| self.models.iter().find(|model| &model.id == selected))
            .map(|model| format!("Model: {}", model.label).into())
            .unwrap_or_else(|| "No models available".into());
        let draft = self.editor.read(cx).value().to_string();
        let running = self.progress == ProgressState::Running;
        let progress_text = match &self.progress {
            ProgressState::Pending => None,
            ProgressState::Running => Some(SharedString::from("Running; cancel is available")),
            ProgressState::Complete => Some(SharedString::from("Ready for another prompt")),
            ProgressState::Failed(reason) => Some(format!("Failed: {reason}").into()),
        };

        prompt_frame(&self.id)
            .gap(tokens.spacing.sm)
            .p(tokens.spacing.md)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.lg)
            .bg(tokens.colors.surface)
            .capture_action(cx.listener(Self::capture_enter))
            .capture_action(cx.listener(Self::capture_move_up))
            .capture_action(cx.listener(Self::capture_move_down))
            .capture_action(cx.listener(Self::capture_move_home))
            .capture_action(cx.listener(Self::capture_move_end))
            .capture_action(cx.listener(Self::capture_escape))
            .when(!attachments.is_empty(), |this| {
                this.child(
                    AttachmentStrip::new((gpui::ElementId::from(root_id.clone()), "attachments"))
                        .label("Prompt attachments")
                        .items(attachments)
                        .removable(true)
                        .compact(true)
                        .on_event(cx.listener(|this, event: &AttachmentEvent, _, cx| {
                            if let AttachmentEvent::Removed { id } = event {
                                cx.emit(PromptBarEvent::AttachmentRemoved {
                                    id: this.id.clone(),
                                    attachment_id: id.clone(),
                                });
                            }
                        })),
                )
            })
            .child(
                div()
                    .debug_selector(|| "prompt-bar-editor".to_owned())
                    .w_full()
                    .child(Textarea::new(&self.editor)),
            )
            .when(!suggestions.is_empty(), |this| {
                this.child(
                    prompt_listbox(
                        (gpui::ElementId::from(root_id.clone()), "suggestions").into(),
                        "Prompt suggestions",
                    )
                    .max_h(tokens.spacing.xxl + tokens.spacing.xxl + tokens.spacing.xxl)
                    .overflow_y_scrollbar()
                    .children(suggestions),
                )
            })
            .when_some(progress_text, |this, progress_text| {
                this.child(
                    prompt_status(
                        (gpui::ElementId::from(root_id.clone()), "progress").into(),
                        progress_text.clone(),
                    )
                    .text_token(tokens.typography.xs)
                    .text_color(cx.theme().muted_foreground)
                    .child(progress_text),
                )
            })
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .flex_wrap()
                    .gap(tokens.spacing.xs)
                    .child(
                        h_flex()
                            .flex_wrap()
                            .gap(tokens.spacing.xs)
                            .child(if self.models.is_empty() {
                                prompt_status(
                                    (gpui::ElementId::from(root_id.clone()), "model-empty").into(),
                                    selected_model_label.clone(),
                                )
                                .debug_selector(|| "prompt-bar-model-empty".to_owned())
                                .text_token(tokens.typography.sm)
                                .text_color(cx.theme().muted_foreground)
                                .child(selected_model_label)
                                .into_any_element()
                            } else {
                                let prompt = cx.entity();
                                div()
                                    .on_prepaint(move |bounds, window, cx| {
                                        let rem_size = window.rem_size();
                                        let changed = prompt.update(cx, |prompt, _| {
                                            let changed = prompt.model_trigger_bounds != bounds
                                                || prompt.model_trigger_rem_size != rem_size;
                                            prompt.model_trigger_bounds = bounds;
                                            prompt.model_trigger_rem_size = rem_size;
                                            changed
                                        });
                                        if changed {
                                            window.request_animation_frame();
                                        }
                                    })
                                    .child(
                                        prompt_model_control(
                                            (gpui::ElementId::from(root_id.clone()), "model"),
                                            selected_model_label,
                                            self.model_menu_open,
                                            cx,
                                        )
                                        .debug_selector(|| "prompt-bar-model-trigger".to_owned())
                                        .border_color(cx.theme().border)
                                        .when(self.model_menu_open, |button| {
                                            button.bg(cx.theme().accent)
                                        })
                                        .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                            cx.stop_propagation();
                                        })
                                        .on_click(
                                            cx.listener(|this, _, window, cx| {
                                                this.toggle_model_menu(window, cx);
                                            }),
                                        ),
                                    )
                                    .into_any_element()
                            })
                            .child(
                                prompt_control(
                                    (gpui::ElementId::from(root_id.clone()), "attach"),
                                    "Attach",
                                    cx,
                                )
                                .debug_selector(|| "prompt-bar-attach-control".to_owned())
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        cx.emit(PromptBarEvent::AttachRequested {
                                            id: this.id.clone(),
                                        });
                                    },
                                )),
                            )
                            .child(
                                prompt_control(
                                    (gpui::ElementId::from(root_id.clone()), "enhance"),
                                    "Enhance",
                                    cx,
                                )
                                .debug_selector(|| "prompt-bar-enhance-control".to_owned())
                                .disabled(draft.trim().is_empty() || running)
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        let draft = this.editor.read(cx).value().to_string();
                                        cx.emit(PromptBarEvent::EnhanceRequested {
                                            id: this.id.clone(),
                                            draft: draft.into(),
                                        });
                                    },
                                )),
                            ),
                    )
                    .child(
                        prompt_primary_control(
                            (gpui::ElementId::from(root_id.clone()), "submit"),
                            if running { "Cancel" } else { "Send" },
                            cx,
                        )
                        .when(running, |button| {
                            button.debug_selector(|| "prompt-bar-cancel-control".to_owned())
                        })
                        .when(!running, |button| {
                            button.debug_selector(|| "prompt-bar-send-control".to_owned())
                        })
                        .disabled(!running && draft.trim().is_empty())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if running {
                                    cx.emit(PromptBarEvent::CancelRequested {
                                        id: this.id.clone(),
                                    });
                                    cues::emit(cx, Cue::Cancelled);
                                } else {
                                    this.submit(window, cx);
                                }
                            },
                        )),
                    ),
            )
            .when(self.model_menu_open, |this| {
                this.child(self.render_model_picker(&root_id, cx))
            })
    }
}

/// Rejects a catalog that repeats a stable ID.
///
/// Repeated IDs alias `ElementId`s, so a second entry would silently take the
/// first one's focus, hover, and reveal state. [`Chat`](crate::chat::Chat) and
/// [`RecordsTable`](crate::records_table::RecordsTable) reject malformed
/// controlled snapshots the same way.
fn stable_ids_are_unique<'a>(mut ids: impl Iterator<Item = &'a SharedString>) -> bool {
    let mut seen = HashSet::new();
    ids.all(|id| seen.insert(id))
}

#[cfg(test)]
mod tests {
    use super::{
        ProgressState, PromptAttachment, PromptBar, PromptBarEvent, PromptCommand, PromptMention,
        PromptModel, PromptTokenKind, SuggestionKey, active_prompt_token, build_submission,
        model_groups, prompt_control, prompt_frame, prompt_listbox, prompt_model_control,
        prompt_status, retain_active_suggestion, stable_ids_are_unique,
    };
    use gpui::{
        AppContext as _, Element as _, Focusable as _, IntoElement as _, ParentElement as _,
        Render, RenderOnce as _, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
        TestAppContext, Window, accesskit, canvas, div, point, px, size,
    };
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
        sync::{Arc, Mutex},
    };

    type CapturedControl = Arc<Mutex<Option<(Option<Role>, accesskit::Node)>>>;

    struct ControlProbe {
        captured: CapturedControl,
        selected_option: bool,
        disabled: bool,
    }

    struct ModelControlProbe {
        captured: CapturedControl,
    }

    struct PromptHarness {
        prompt: gpui::Entity<PromptBar>,
        bottom_aligned: bool,
        _subscription: gpui::Subscription,
    }

    impl PromptHarness {
        fn new(
            events: Rc<RefCell<Vec<PromptBarEvent>>>,
            window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> Self {
            let prompt = cx.new(|cx| {
                let mut prompt = PromptBar::new("keyboard-prompt", window, cx);
                prompt.set_mentions(
                    [
                        PromptMention::new("creamery", "Creamery"),
                        PromptMention::new("suppliers", "Suppliers"),
                    ],
                    cx,
                );
                prompt.set_draft("@", window, cx);
                prompt
            });
            let _subscription = cx.subscribe(&prompt, move |_, _, event, _| {
                events.borrow_mut().push(event.clone());
            });
            Self {
                prompt,
                bottom_aligned: false,
                _subscription,
            }
        }

        fn with_models(
            events: Rc<RefCell<Vec<PromptBarEvent>>>,
            bottom_aligned: bool,
            window: &mut Window,
            cx: &mut gpui::Context<Self>,
        ) -> Self {
            let prompt = cx.new(|cx| {
                let mut prompt = PromptBar::new("model-prompt", window, cx);
                prompt.set_models(
                    [
                        PromptModel::new("fast", "Fast")
                            .provider("Lab")
                            .description("Fast responses")
                            .context_window(64_000),
                        PromptModel::new("disabled", "Disabled")
                            .provider("Lab")
                            .disabled(true),
                        PromptModel::new("balanced", "Balanced")
                            .provider("Cloud")
                            .description("Everyday work")
                            .context_window(128_000),
                        PromptModel::new("precise", "Precise")
                            .provider("Cloud")
                            .description("Detailed reasoning")
                            .context_window(200_000),
                    ],
                    cx,
                );
                prompt
            });
            let _subscription = cx.subscribe(&prompt, move |_, _, event, _| {
                events.borrow_mut().push(event.clone());
            });
            Self {
                prompt,
                bottom_aligned,
                _subscription,
            }
        }
    }

    impl Render for PromptHarness {
        fn render(
            &mut self,
            _: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            if self.bottom_aligned {
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .justify_end()
                    .child(self.prompt.clone())
                    .into_any_element()
            } else {
                self.prompt.clone().into_any_element()
            }
        }
    }

    fn draw(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn open_model_picker(cx: &mut gpui::VisualTestContext) {
        let trigger = cx
            .debug_bounds("prompt-bar-model-trigger")
            .expect("the model trigger should render");
        cx.simulate_click(trigger.center(), Default::default());
        draw(cx);
    }

    impl Render for ControlProbe {
        fn render(
            &mut self,
            _: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let captured = self.captured.clone();
            let selected_option = self.selected_option;
            let disabled = self.disabled;
            canvas(
                move |_, window, cx| {
                    let control = prompt_control("prompt-send", "Send prompt", cx)
                        .disabled(disabled)
                        .on_click(|_, _, _| {});
                    let control = if selected_option {
                        control.role(Role::ListBoxOption).aria_selected(true)
                    } else {
                        control
                    };
                    let element = control.render(window, cx).into_element();
                    let role = element.a11y_role();
                    let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
                    element.write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") =
                        Some((role, node));
                },
                |_, _, _, _| {},
            )
        }
    }

    impl Render for ModelControlProbe {
        fn render(
            &mut self,
            _: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let control = prompt_model_control("prompt-model", "Model: Balanced", true, cx)
                        .on_click(|_, _, _| {});
                    let element = control.render(window, cx).into_element();
                    let role = element.a11y_role();
                    let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
                    element.write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") =
                        Some((role, node));
                },
                |_, _, _, _| {},
            )
        }
    }

    #[test]
    fn models_group_by_provider_in_first_appearance_order() {
        let models = [
            PromptModel::new("a", "A").provider("Anthropic"),
            PromptModel::new("local", "Local"),
            PromptModel::new("b", "B").provider("Anthropic"),
            PromptModel::new("o", "O").provider("OpenAI"),
        ];
        let groups = model_groups(&models);
        let providers: Vec<Option<&str>> = groups
            .iter()
            .map(|(provider, _)| provider.as_deref())
            .collect();
        assert_eq!(providers, [Some("Anthropic"), None, Some("OpenAI")]);
        let anthropic: Vec<&str> = groups[0].1.iter().map(|model| model.id.as_ref()).collect();
        assert_eq!(anthropic, ["a", "b"]);
        assert_eq!(
            PromptModel::new("x", "X")
                .provider("Anthropic")
                .description("Everyday")
                .context_window(200_000)
                .context_window_tokens(),
            Some(200_000)
        );
    }

    #[test]
    fn utf8_cursor_token_extraction_uses_byte_offsets() {
        let draft = "plan 🍦 @crème today";
        let cursor = draft.find(" today").expect("suffix should exist");
        let token = active_prompt_token(draft, cursor).expect("mention token should be active");

        assert_eq!(token.kind, PromptTokenKind::Mention);
        assert_eq!(&draft[token.range], "@crème");
        assert_eq!(token.query, "crème");
    }

    #[test]
    fn mid_token_range_includes_the_untyped_suffix_for_replacement() {
        let draft = "Ask @Creamery about pricing";
        let cursor = draft.find("amery").expect("mention suffix should exist");
        let token = active_prompt_token(draft, cursor).expect("mention token should be active");

        assert_eq!(&draft[token.range], "@Creamery");
        assert_eq!(token.query, "Cre");
    }

    #[test]
    fn stable_duplicate_label_suggestions_retain_identity() {
        let first = SuggestionKey::Mention("first".into());
        let second = SuggestionKey::Mention("second".into());
        let filtered = vec![first.clone(), second.clone()];

        assert_eq!(
            retain_active_suggestion(Some(second.clone()), &filtered),
            Some(second)
        );
        assert_ne!(
            PromptMention::new("first", "Sam").id(),
            PromptMention::new("second", "Sam").id()
        );
    }

    #[gpui::test]
    fn visible_suggestion_label_changes_notify_the_prompt_entity(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (prompt, cx) = cx.add_window_view(|window, cx| PromptBar::new("prompt", window, cx));
        prompt.update(cx, |prompt, cx| {
            prompt.set_mentions([PromptMention::new("creamery", "Creamery")], cx);
        });
        cx.update(|window, cx| {
            prompt.update(cx, |prompt, cx| prompt.set_draft("@cr", window, cx));
        });
        assert!(prompt.read_with(cx, |prompt, _| prompt.token.is_some()));

        let notifications = Rc::new(Cell::new(0));
        let observed = notifications.clone();
        let _subscription =
            cx.update(|_, cx| cx.observe(&prompt, move |_, _| observed.set(observed.get() + 1)));

        prompt.update(cx, |prompt, cx| {
            prompt.set_mentions([PromptMention::new("creamery", "Creamery team")], cx);
        });

        assert_eq!(notifications.get(), 1);
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn cursor_only_keyboard_change_refreshes_the_active_token(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (prompt, cx) = cx.add_window_view(|window, cx| {
            let mut prompt = PromptBar::new("prompt", window, cx);
            prompt.set_mentions(
                [
                    PromptMention::new("creamery", "Creamery"),
                    PromptMention::new("suppliers", "Suppliers"),
                ],
                cx,
            );
            prompt.set_draft("Ask @Creamery then @Suppliers", window, cx);
            prompt
        });
        cx.update(|window, cx| {
            prompt.update(cx, |prompt, cx| prompt.focus(window, cx));
            window.draw(cx).clear(cx);
        });

        cx.simulate_keystrokes("left left left left left left left left");

        assert_eq!(
            prompt.read_with(cx, |prompt, _| prompt
                .token
                .as_ref()
                .map(|token| token.query.clone())),
            Some("S".to_owned())
        );
        assert_eq!(
            prompt.read_with(cx, |prompt, _| prompt.filtered.clone()),
            vec![SuggestionKey::Mention("suppliers".into())]
        );
    }

    #[gpui::test]
    fn cursor_only_mouse_equivalent_refreshes_and_replaces_the_whole_token(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(captured_events, window, cx));
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| {
                prompt.set_draft("Ask @Creamery now", window, cx);
                let cursor = "Ask @Cre".len();
                prompt.editor.update(cx, |editor, cx| {
                    editor.set_selected_range(cursor..cursor, cx)
                });
            });
            window.draw(cx).clear(cx);
        });

        let prompt = harness.read_with(cx, |harness, _| harness.prompt.clone());
        cx.update(|window, cx| {
            prompt.update(cx, |prompt, cx| prompt.insert_active_suggestion(window, cx));
        });

        assert_eq!(
            prompt.read_with(cx, |prompt, cx| prompt.draft(cx)),
            "Ask @Creamery now"
        );
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn unmatched_token_does_not_capture_native_multiline_up(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(events, window, cx));
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| {
                prompt.set_draft("first line\n@unmatched", window, cx);
                prompt.focus(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        let before = harness.read_with(cx, |harness, cx| {
            harness.prompt.read(cx).editor.read(cx).cursor()
        });

        cx.simulate_keystrokes("up");

        let after = harness.read_with(cx, |harness, cx| {
            harness.prompt.read(cx).editor.read(cx).cursor()
        });
        assert!(after < before, "native multiline up should move the caret");
    }

    #[gpui::test]
    fn empty_model_catalog_closes_the_menu(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (prompt, cx) = cx.add_window_view(|window, cx| PromptBar::new("prompt", window, cx));
        prompt.update(cx, |prompt, cx| {
            prompt.set_models([super::PromptModel::new("balanced", "Balanced")], cx);
            prompt.model_menu_open = true;
            prompt.set_models([], cx);
        });

        assert!(!prompt.read_with(cx, |prompt, _| prompt.model_menu_open));
        assert!(prompt.read_with(cx, |prompt, _| prompt.models.is_empty()));
    }

    #[gpui::test]
    fn closed_model_picker_constructs_no_model_options(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (harness, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(events, false, window, cx)
        });
        draw(cx);

        assert!(cx.debug_bounds("prompt-bar-model-trigger").is_some());
        assert!(cx.debug_bounds("prompt-bar-model-picker").is_none());
        for selector in [
            "prompt-bar-model-option-fast",
            "prompt-bar-model-option-disabled",
            "prompt-bar-model-option-balanced",
            "prompt-bar-model-option-precise",
        ] {
            assert!(cx.debug_bounds(selector).is_none());
        }
        assert!(!harness.read_with(cx, |harness, cx| {
            harness.prompt.read(cx).model_menu_open
        }));
    }

    #[gpui::test]
    fn opening_model_picker_keeps_composer_height_and_floats(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(events, false, window, cx)
        });
        draw(cx);
        let attach_before = cx
            .debug_bounds("prompt-bar-attach-control")
            .expect("attach should render before opening");

        open_model_picker(cx);

        let attach_after = cx
            .debug_bounds("prompt-bar-attach-control")
            .expect("attach should remain rendered");
        let trigger = cx
            .debug_bounds("prompt-bar-model-trigger")
            .expect("model trigger should remain rendered");
        let picker = cx
            .debug_bounds("prompt-bar-model-picker")
            .expect("open picker should render");
        assert_eq!(attach_after.origin.y, attach_before.origin.y);
        assert!(
            picker.top() >= trigger.bottom() || picker.bottom() <= trigger.top(),
            "picker {picker:?} must float outside trigger {trigger:?}"
        );
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn model_picker_keys_move_by_stable_id_and_skip_disabled_models(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (harness, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(events, false, window, cx)
        });
        draw(cx);
        open_model_picker(cx);
        let active = |cx: &mut gpui::VisualTestContext| {
            harness.read_with(cx, |harness, cx| {
                harness.prompt.read(cx).active_model.clone()
            })
        };

        assert_eq!(active(cx), Some("fast".into()));
        cx.simulate_keystrokes("down");
        assert_eq!(active(cx), Some("balanced".into()));
        cx.simulate_keystrokes("down up");
        assert_eq!(active(cx), Some("balanced".into()));
        cx.simulate_keystrokes("home");
        assert_eq!(active(cx), Some("fast".into()));
        cx.simulate_keystrokes("end");
        assert_eq!(active(cx), Some("precise".into()));
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn model_picker_enter_emits_the_active_id_once_and_closes(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let (harness, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(captured_events, false, window, cx)
        });
        draw(cx);
        open_model_picker(cx);
        events.borrow_mut().clear();

        cx.simulate_keystrokes("down enter");
        draw(cx);

        assert!(!harness.read_with(cx, |harness, cx| {
            harness.prompt.read(cx).model_menu_open
        }));
        assert!(cx.debug_bounds("prompt-bar-model-picker").is_none());
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| matches!(
                    event,
                    PromptBarEvent::ModelChanged { model_id, .. } if model_id == "balanced"
                ))
                .count(),
            1
        );
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn model_picker_escape_closes_without_emitting(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let (harness, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(captured_events, false, window, cx)
        });
        draw(cx);
        open_model_picker(cx);
        events.borrow_mut().clear();

        cx.simulate_keystrokes("escape");
        draw(cx);

        assert!(!harness.read_with(cx, |harness, cx| {
            harness.prompt.read(cx).model_menu_open
        }));
        assert!(events.borrow().is_empty());
    }

    #[gpui::test]
    fn outside_click_dismisses_the_model_picker(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (harness, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(events, false, window, cx)
        });
        cx.simulate_resize(size(px(800.), px(600.)));
        draw(cx);
        open_model_picker(cx);

        cx.simulate_click(point(px(790.), px(590.)), Default::default());
        draw(cx);

        assert!(!harness.read_with(cx, |harness, cx| {
            harness.prompt.read(cx).model_menu_open
        }));
        assert!(cx.debug_bounds("prompt-bar-model-picker").is_none());
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn closing_model_picker_restores_editor_for_immediate_typing(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (harness, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(events, false, window, cx)
        });
        draw(cx);
        open_model_picker(cx);

        cx.simulate_keystrokes("down enter x");

        assert_eq!(
            harness.read_with(cx, |harness, cx| harness.prompt.read(cx).draft(cx)),
            "x"
        );
    }

    #[gpui::test]
    fn bottom_docked_model_picker_flips_above_its_trigger(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (_, cx) = cx.add_window_view(move |window, cx| {
            PromptHarness::with_models(events, true, window, cx)
        });
        cx.simulate_resize(size(px(500.), px(320.)));
        draw(cx);
        open_model_picker(cx);

        let trigger = cx
            .debug_bounds("prompt-bar-model-trigger")
            .expect("bottom trigger should render");
        let picker = cx
            .debug_bounds("prompt-bar-model-picker")
            .expect("bottom picker should render");
        assert!(
            picker.bottom() <= trigger.top(),
            "bottom picker {picker:?} should flip above trigger {trigger:?}"
        );
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn keyboard_navigation_inserts_the_active_stable_suggestion(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(captured_events, window, cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| prompt.focus(window, cx));
        });

        cx.simulate_keystrokes("down");
        assert_eq!(
            harness.read_with(cx, |harness, cx| {
                harness.prompt.read(cx).active_suggestion.clone()
            }),
            Some(SuggestionKey::Mention("suppliers".into()))
        );
        cx.simulate_keystrokes("enter");

        let draft = harness.read_with(cx, |harness, cx| harness.prompt.read(cx).draft(cx));
        assert_eq!(draft, "@Suppliers ");
        assert!(events.borrow().iter().any(|event| matches!(
            event,
            PromptBarEvent::MentionSelected { mention_id, .. } if mention_id == "suppliers"
        )));
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn shift_enter_preserves_multiline_editing_without_submitting(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(captured_events, window, cx));
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| prompt.set_draft("first", window, cx));
            window.draw(cx).clear(cx);
        });
        events.borrow_mut().clear();
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| prompt.focus(window, cx));
        });

        cx.simulate_keystrokes("shift-enter");

        let draft = harness.read_with(cx, |harness, cx| harness.prompt.read(cx).draft(cx));
        assert_eq!(draft, "first\n");
        assert!(
            !events
                .borrow()
                .iter()
                .any(|event| matches!(event, PromptBarEvent::Submit { .. }))
        );
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn plain_enter_submits_once_and_leaves_no_newline(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(captured_events, window, cx));
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| prompt.set_draft("send this", window, cx));
            window.draw(cx).clear(cx);
        });
        events.borrow_mut().clear();
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| prompt.focus(window, cx));
        });

        cx.simulate_keystrokes("enter");

        let draft = harness.read_with(cx, |harness, cx| harness.prompt.read(cx).draft(cx));
        assert_eq!(draft, "");
        assert_eq!(
            events
                .borrow()
                .iter()
                .filter(|event| matches!(event, PromptBarEvent::Submit { .. }))
                .count(),
            1
        );
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn escape_closes_the_model_menu_without_dropping_editor_focus(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(events, window, cx));
        cx.update(|window, cx| {
            let prompt = harness.read(cx).prompt.clone();
            prompt.update(cx, |prompt, cx| {
                prompt.set_draft("compare suppliers", window, cx);
                prompt.set_models([super::PromptModel::new("balanced", "Balanced")], cx);
                prompt.model_menu_open = true;
                prompt.focus(window, cx);
                cx.notify();
            });
            window.draw(cx).clear(cx);
        });

        cx.simulate_keystrokes("escape");

        assert!(!harness.read_with(cx, |harness, cx| {
            harness.prompt.read(cx).model_menu_open
        }));
        assert!(cx.update(|window, cx| {
            harness
                .read(cx)
                .prompt
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        }));
    }

    #[gpui::test]
    fn constrained_width_keeps_the_primary_action_reachable(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(events, window, cx));
        harness.update(cx, |harness, cx| {
            harness.prompt.update(cx, |prompt, cx| {
                prompt.set_progress(ProgressState::Running, cx)
            });
        });
        cx.simulate_resize(size(px(300.), px(350.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let primary = cx
            .debug_bounds("prompt-bar-cancel-control")
            .expect("the primary action should remain rendered");
        assert!(
            primary.size.width > px(0.),
            "primary action {primary:?} must retain a visible width"
        );
        assert!(
            primary.right() <= px(300.),
            "primary action {primary:?} must remain within the 300px viewport"
        );
    }

    #[test]
    fn active_suggestion_falls_back_safely_after_catalog_change() {
        let available = vec![SuggestionKey::Mention("remaining".into())];
        assert_eq!(
            retain_active_suggestion(Some(SuggestionKey::Mention("removed".into())), &available),
            available.first().cloned()
        );
        assert_eq!(retain_active_suggestion(None, &[]), None);
    }

    #[test]
    fn empty_submission_is_rejected_and_attachment_identity_is_preserved() {
        assert!(build_submission("  \n ", None, &[]).is_none());
        let attachments = [
            PromptAttachment::new("sales", "sales.csv"),
            PromptAttachment::new("brief", "brief.md"),
        ];
        let submission = build_submission("  summarize these  ", Some("fast".into()), &attachments)
            .expect("non-empty draft should submit");

        assert_eq!(submission.text(), "summarize these");
        assert_eq!(submission.model_id(), Some(&"fast".into()));
        assert_eq!(submission.attachment_ids(), &["sales", "brief"]);
    }

    #[test]
    fn production_frames_expose_named_group_and_listbox_semantics() {
        let root = prompt_frame(&"prompt".into()).into_element();
        let mut root_node = accesskit::Node::new(Role::Unknown);
        root.write_a11y_info(&mut root_node);
        assert_eq!(root.a11y_role(), Some(Role::Group));
        assert_eq!(root_node.label(), Some("Prompt composer"));

        let listbox = prompt_listbox("suggestions".into(), "Prompt suggestions").into_element();
        let mut listbox_node = accesskit::Node::new(Role::Unknown);
        listbox.write_a11y_info(&mut listbox_node);
        assert_eq!(listbox.a11y_role(), Some(Role::ListBox));
        assert_eq!(listbox_node.label(), Some("Prompt suggestions"));

        let status =
            prompt_status("progress".into(), "Running; cancel is available".into()).into_element();
        let mut status_node = accesskit::Node::new(Role::Unknown);
        status.write_a11y_info(&mut status_node);
        assert_eq!(status.a11y_role(), Some(Role::Status));
        assert_eq!(status_node.label(), Some("Running; cancel is available"));
    }

    #[gpui::test]
    fn production_controls_are_named_keyboard_activatable_buttons(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ControlProbe {
            captured,
            selected_option: false,
            disabled: false,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let (role, node) = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("control semantics should be captured");
        assert_eq!(role, Some(Role::Button));
        assert_eq!(node.label(), Some("Send prompt"));
        assert!(node.supports_action(accesskit::Action::Click));
    }

    #[gpui::test]
    fn disabled_production_control_exposes_no_click_action(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ControlProbe {
            captured,
            selected_option: false,
            disabled: true,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let (_, node) = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("disabled control semantics should be captured");
        assert!(!node.supports_action(accesskit::Action::Click));
    }

    #[gpui::test]
    fn suggestion_options_expose_selection_and_activation(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ControlProbe {
            captured,
            selected_option: true,
            disabled: false,
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let (role, node) = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("option semantics should be captured");
        assert_eq!(role, Some(Role::ListBoxOption));
        assert_eq!(node.label(), Some("Send prompt"));
        assert_eq!(node.is_selected(), Some(true));
        assert!(node.supports_action(accesskit::Action::Click));
    }

    #[gpui::test]
    fn model_trigger_exposes_its_expanded_state(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ModelControlProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let (role, node) = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("model trigger semantics should be captured");
        assert_eq!(role, Some(Role::Button));
        assert_eq!(node.label(), Some("Model: Balanced"));
        assert_eq!(node.is_expanded(), Some(true));
        assert!(node.supports_action(accesskit::Action::Click));
    }

    #[test]
    fn repeated_stable_ids_are_rejected() {
        let unique = [
            SharedString::from("balanced"),
            SharedString::from("precise"),
        ];
        assert!(stable_ids_are_unique(unique.iter()));

        let repeated = [
            SharedString::from("balanced"),
            SharedString::from("precise"),
            SharedString::from("balanced"),
        ];
        assert!(!stable_ids_are_unique(repeated.iter()));
        assert!(stable_ids_are_unique(std::iter::empty()));
    }

    #[gpui::test]
    fn catalogs_repeating_a_stable_id_are_ignored_atomically(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (prompt, cx) =
            cx.add_window_view(|window, cx| PromptBar::new("duplicates", window, cx));

        let valid = || {
            [
                PromptModel::new("balanced", "Balanced"),
                PromptModel::new("precise", "Precise"),
            ]
        };
        cx.update(|_, cx| {
            prompt.update(cx, |prompt, cx| {
                prompt.set_models(valid(), cx);
                prompt.set_mentions([PromptMention::new("docs", "docs")], cx);
                prompt.set_commands([PromptCommand::new("summarize", "summarize")], cx);
            });
        });

        cx.update(|_, cx| {
            prompt.update(cx, |prompt, cx| {
                // Each of these repeats an ID, so each must be refused whole
                // rather than installing a catalog with aliased ElementIds.
                prompt.set_models(
                    [
                        PromptModel::new("fast", "Fast"),
                        PromptModel::new("fast", "Fast again"),
                    ],
                    cx,
                );
                prompt.set_mentions(
                    [
                        PromptMention::new("specs", "specs"),
                        PromptMention::new("specs", "specs again"),
                    ],
                    cx,
                );
                prompt.set_commands(
                    [
                        PromptCommand::new("explain", "explain"),
                        PromptCommand::new("explain", "explain again"),
                    ],
                    cx,
                );
            });
        });

        prompt.read_with(cx, |prompt, _| {
            assert_eq!(
                prompt.models,
                valid().to_vec(),
                "a malformed model catalog must leave the previous one untouched"
            );
            assert_eq!(prompt.mentions.len(), 1, "mentions must be unchanged");
            assert_eq!(prompt.commands.len(), 1, "commands must be unchanged");
        });
    }

    #[cfg_attr(
        target_os = "macos",
        ignore = "pinned GPUI TestWindow has no native macOS handle for focused TextareaState"
    )]
    #[gpui::test]
    fn composer_grows_per_line_and_stops_at_its_auto_grow_cap(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let events = Rc::new(RefCell::new(Vec::new()));
        let captured_events = events.clone();
        let (harness, cx) =
            cx.add_window_view(move |window, cx| PromptHarness::new(captured_events, window, cx));
        let measure = |cx: &mut gpui::VisualTestContext, draft: String| {
            cx.update(|window, cx| {
                let prompt = harness.read(cx).prompt.clone();
                prompt.update(cx, |prompt, cx| prompt.set_draft(draft, window, cx));
                window.draw(cx).clear(cx);
            });
            cx.debug_bounds("prompt-bar-editor")
                .expect("the composer should render")
                .size
                .height
        };
        let one = measure(cx, "first".into());
        let three = measure(cx, "first\nsecond\nthird".into());
        let five = measure(cx, ["l1", "l2", "l3", "l4", "l5"].join("\n"));
        let nine = measure(
            cx,
            (1..=9)
                .map(|line| format!("line {line}"))
                .collect::<Vec<_>>()
                .join("\n"),
        );
        // The composer is deliberately multi-line: each added line grows it
        // until the auto-grow cap, and past the cap it scrolls instead of
        // growing — the half of the upstream input contract the single-line
        // fields must not have.
        assert!(three > one, "{three:?} vs {one:?}");
        assert!(five > three, "{five:?} vs {three:?}");
        assert_eq!(nine, five);
    }
}
