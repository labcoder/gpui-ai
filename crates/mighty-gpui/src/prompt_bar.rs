//! Hybrid-controlled prompt composition with native GPUI text editing.

use crate::control::composed_button;
use crate::stream::ProgressState;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, AppContext as _, Div, ElementId, Entity, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled, Subscription, Window, div,
    prelude::FluentBuilder as _,
};
use gpui_base::Button;
use gpui_component::{
    ActiveTheme as _, h_flex,
    input::{Enter, Escape, InputEvent, MoveDown, MoveUp, Textarea, TextareaState},
    scroll::ScrollableElement as _,
    v_flex,
};
use std::ops::Range;

/// A selectable model offered by a [`PromptBar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptModel {
    id: SharedString,
    label: SharedString,
    disabled: bool,
}

impl PromptModel {
    /// Creates an enabled model with stable identity and a visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            disabled: false,
        }
    }

    /// Sets whether the model can be selected.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
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

/// An application-owned attachment displayed by a [`PromptBar`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAttachment {
    id: SharedString,
    label: SharedString,
}

impl PromptAttachment {
    /// Creates an attachment with stable identity and a visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    /// Returns the stable attachment identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible attachment label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }
}

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
    } else if let Some(query) = token.strip_prefix('/') {
        (PromptTokenKind::Command, query)
    } else {
        return None;
    };
    (!query.chars().any(char::is_whitespace)).then(|| PromptToken {
        kind,
        range: start..cursor,
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
        attachment_ids: attachments.iter().map(|item| item.id.clone()).collect(),
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
    prompt_control(id, label, cx)
        .selected(expanded)
        .aria_expanded(expanded)
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
    token: Option<PromptToken>,
    filtered: Vec<SuggestionKey>,
    active_suggestion: Option<SuggestionKey>,
    model_menu_open: bool,
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
            token: None,
            filtered: Vec::new(),
            active_suggestion: None,
            model_menu_open: false,
            _subscriptions: vec![subscription],
        }
    }

    /// Replaces the model catalog while preserving a still-valid selection.
    pub fn set_models(
        &mut self,
        models: impl IntoIterator<Item = PromptModel>,
        cx: &mut gpui::Context<Self>,
    ) {
        let models: Vec<_> = models.into_iter().collect();
        if self.models == models {
            return;
        }
        self.models = models;
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
        self.selected_model = Some(model_id);
        cx.notify();
    }

    /// Replaces the `@` mention catalog.
    pub fn set_mentions(
        &mut self,
        mentions: impl IntoIterator<Item = PromptMention>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mentions: Vec<_> = mentions.into_iter().collect();
        if self.mentions != mentions {
            self.mentions = mentions;
            if !self.refresh_suggestions(cx) {
                cx.notify();
            }
        }
    }

    /// Replaces the `/` command catalog.
    pub fn set_commands(
        &mut self,
        commands: impl IntoIterator<Item = PromptCommand>,
        cx: &mut gpui::Context<Self>,
    ) {
        let commands: Vec<_> = commands.into_iter().collect();
        if self.commands != commands {
            self.commands = commands;
            if !self.refresh_suggestions(cx) {
                cx.notify();
            }
        }
    }

    /// Replaces application-owned attachments.
    pub fn set_attachments(
        &mut self,
        attachments: impl IntoIterator<Item = PromptAttachment>,
        cx: &mut gpui::Context<Self>,
    ) {
        let attachments: Vec<_> = attachments.into_iter().collect();
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
                if self.token.is_some() && self.active_suggestion.is_some() {
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
        if self.token.is_none() {
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

    fn capture_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut gpui::Context<Self>) {
        let handled = if self.token.is_some() {
            self.token = None;
            self.filtered.clear();
            self.active_suggestion = None;
            true
        } else if self.model_menu_open {
            self.model_menu_open = false;
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
        if self.token.is_some() && self.active_suggestion.is_some() {
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
        let (replacement, event) = match active {
            SuggestionKey::Mention(id) => {
                let Some(mention) = self.mentions.iter().find(|mention| mention.id == id) else {
                    return;
                };
                (
                    format!("@{} ", mention.label),
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
                    format!("/{} ", command.label),
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
        let model_buttons = self
            .models
            .iter()
            .map(|model| {
                let model_id = model.id.clone();
                let selected = self.selected_model.as_ref() == Some(&model.id);
                prompt_control(
                    (
                        gpui::ElementId::from(root_id.clone()),
                        format!("model-{}", model.id),
                    ),
                    model.label.clone(),
                    cx,
                )
                .role(Role::ListBoxOption)
                .disabled(model.disabled)
                .selected(selected)
                .aria_selected(selected)
                .w_full()
                .when(selected, |button| button.bg(cx.theme().accent))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.model_menu_open = false;
                    cx.emit(PromptBarEvent::ModelChanged {
                        id: this.id.clone(),
                        model_id: model_id.clone(),
                    });
                    cx.notify();
                }))
            })
            .collect::<Vec<_>>();
        let attachment_buttons = self
            .attachments
            .iter()
            .map(|attachment| {
                let attachment_id = attachment.id.clone();
                prompt_control(
                    (
                        gpui::ElementId::from(root_id.clone()),
                        format!("attachment-{}", attachment.id),
                    ),
                    format!("Remove {}", attachment.label),
                    cx,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.emit(PromptBarEvent::AttachmentRemoved {
                        id: this.id.clone(),
                        attachment_id: attachment_id.clone(),
                    });
                }))
            })
            .collect::<Vec<_>>();
        let selected_model_label = self
            .selected_model
            .as_ref()
            .and_then(|selected| self.models.iter().find(|model| &model.id == selected))
            .map(|model| model.label.clone())
            .unwrap_or_else(|| "Choose model".into());
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
            .rounded(tokens.radius.md)
            .bg(cx.theme().background)
            .capture_action(cx.listener(Self::capture_enter))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                this.capture_suggestion_action(Some(-1), cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                this.capture_suggestion_action(Some(1), cx);
            }))
            .capture_action(cx.listener(Self::capture_escape))
            .when(!attachment_buttons.is_empty(), |this| {
                this.child(
                    h_flex()
                        .id((gpui::ElementId::from(root_id.clone()), "attachments"))
                        .role(Role::Group)
                        .aria_label("Prompt attachments")
                        .flex_wrap()
                        .gap(tokens.spacing.xs)
                        .children(attachment_buttons),
                )
            })
            .child(Textarea::new(&self.editor))
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
                            .child(
                                prompt_model_control(
                                    (gpui::ElementId::from(root_id.clone()), "model"),
                                    selected_model_label,
                                    self.model_menu_open,
                                    cx,
                                )
                                .border_color(cx.theme().border)
                                .when(self.model_menu_open, |button| button.bg(cx.theme().accent))
                                .on_click(cx.listener(
                                    |this, _, _, cx| {
                                        this.model_menu_open = !this.model_menu_open;
                                        cx.notify();
                                    },
                                )),
                            )
                            .child(
                                prompt_control(
                                    (gpui::ElementId::from(root_id.clone()), "attach"),
                                    "Attach",
                                    cx,
                                )
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
                        .debug_selector(|| "prompt-bar-primary-action".to_owned())
                        .disabled(!running && draft.trim().is_empty())
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                if running {
                                    cx.emit(PromptBarEvent::CancelRequested {
                                        id: this.id.clone(),
                                    });
                                } else {
                                    this.submit(window, cx);
                                }
                            },
                        )),
                    ),
            )
            .when(self.model_menu_open && !model_buttons.is_empty(), |this| {
                this.child(
                    prompt_listbox(
                        (gpui::ElementId::from(root_id), "models").into(),
                        "Available models",
                    )
                    .max_h(tokens.spacing.xxl + tokens.spacing.xxl + tokens.spacing.xxl)
                    .overflow_y_scrollbar()
                    .children(model_buttons),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ProgressState, PromptAttachment, PromptBar, PromptBarEvent, PromptMention, PromptTokenKind,
        SuggestionKey, active_prompt_token, build_submission, prompt_control, prompt_frame,
        prompt_listbox, prompt_model_control, prompt_status, retain_active_suggestion,
    };
    use gpui::{
        AppContext as _, Element as _, Focusable as _, IntoElement as _, Render, RenderOnce as _,
        Role, StatefulInteractiveElement as _, TestAppContext, Window, accesskit, canvas, px, size,
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
    }

    struct ModelControlProbe {
        captured: CapturedControl,
    }

    struct PromptHarness {
        prompt: gpui::Entity<PromptBar>,
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
            self.prompt.clone()
        }
    }

    impl Render for ControlProbe {
        fn render(
            &mut self,
            _: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let captured = self.captured.clone();
            let selected_option = self.selected_option;
            canvas(
                move |_, window, cx| {
                    let control =
                        prompt_control("prompt-send", "Send prompt", cx).on_click(|_, _, _| {});
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
                    let control = prompt_model_control("prompt-model", "Balanced", true, cx)
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
    fn utf8_cursor_token_extraction_uses_byte_offsets() {
        let draft = "plan 🍦 @crème today";
        let cursor = draft.find(" today").expect("suffix should exist");
        let token = active_prompt_token(draft, cursor).expect("mention token should be active");

        assert_eq!(token.kind, PromptTokenKind::Mention);
        assert_eq!(&draft[token.range], "@crème");
        assert_eq!(token.query, "crème");
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
            .debug_bounds("prompt-bar-primary-action")
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
    fn suggestion_options_expose_selection_and_activation(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ControlProbe {
            captured,
            selected_option: true,
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
        assert_eq!(node.label(), Some("Balanced"));
        assert_eq!(node.is_expanded(), Some(true));
        assert!(node.supports_action(accesskit::Action::Click));
    }
}
