//! Voice controls: dictation and speech as typed intent.
//!
//! gpui-ai does not capture audio or synthesize speech. [`VoiceControls`]
//! renders the dictate and speak controls for a [`VoiceState`] the
//! application owns, shows the live input level and interim transcript it
//! is handed, and reports every press as a [`VoiceEvent`] so the
//! application starts, stops, or cancels the real audio work.

use crate::{
    control::{composed_button, outlined_control_with_label},
    handlers::SharedHandler,
    motion::{MotionTokens, breathing_with},
    surface::icon_button,
    theme::SemanticStyledExt as _,
};
use gpui::{
    AnyElement, App, ClickEvent, ElementId, InteractiveElement as _, IntoElement,
    ParentElement as _, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, rems,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, spinner::Spinner,
    v_flex,
};
use std::rc::Rc;

/// Where the application's audio pipeline is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VoiceState {
    /// Nothing is recording or playing.
    Idle,
    /// The microphone is open; `level` is the current input level (0–1).
    Listening {
        /// Normalized input level used for the meter.
        level: f32,
    },
    /// Audio stopped; speech-to-text is still finishing.
    Transcribing,
    /// Text-to-speech is playing.
    Speaking,
}

impl VoiceState {
    /// The short status word read to assistive technology.
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Listening { .. } => "Listening",
            Self::Transcribing => "Transcribing",
            Self::Speaking => "Speaking",
        }
    }

    /// Whether the microphone is open.
    pub fn is_listening(self) -> bool {
        matches!(self, Self::Listening { .. })
    }
}

/// An interaction emitted by [`VoiceControls`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEvent {
    /// The user asked to start dictating.
    DictationStarted,
    /// The user asked to stop and transcribe what was heard.
    DictationStopped,
    /// The user discarded the dictation in progress.
    DictationCancelled,
    /// The user asked for the latest response to be read aloud.
    SpeakRequested,
    /// The user stopped playback.
    SpeakStopped,
}

/// Dictate and speak controls with a live level meter and interim transcript.
///
/// # Example
///
/// ```ignore
/// VoiceControls::new("voice", VoiceState::Listening { level: 0.6 })
///     .transcript("compare the three suppl…")
///     .speakable(true)
///     .on_event(|event, _, _| { /* VoiceEvent::DictationStopped … */ })
/// ```
#[derive(IntoElement)]
pub struct VoiceControls {
    id: SharedString,
    style: StyleRefinement,
    state: VoiceState,
    transcript: Option<SharedString>,
    speakable: bool,
    on_event: Option<SharedHandler<VoiceEvent>>,
}

impl VoiceControls {
    /// Creates the controls for the given state.
    pub fn new(id: impl Into<SharedString>, state: VoiceState) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            state,
            transcript: None,
            speakable: false,
            on_event: None,
        }
    }

    /// Shows the interim transcript under the controls.
    pub fn transcript(mut self, transcript: impl Into<SharedString>) -> Self {
        let transcript = transcript.into();
        self.transcript = (!transcript.is_empty()).then_some(transcript);
        self
    }

    /// Offers a Speak / Stop control for reading responses aloud.
    pub fn speakable(mut self, speakable: bool) -> Self {
        self.speakable = speakable;
        self
    }

    /// Handles typed interactions. Without a handler the controls are inert.
    pub fn on_event(
        mut self,
        handler: impl Fn(&VoiceEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

impl Styled for VoiceControls {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for VoiceControls {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let root_id = ElementId::from(self.id.clone());
        let debug_id = self.id.to_string();
        let handler = self.on_event;
        let state = self.state;

        let (dictate_name, dictate_label, dictate_event): (&str, &str, Option<VoiceEvent>) =
            match state {
                VoiceState::Idle | VoiceState::Speaking => (
                    "Start dictation",
                    "Dictate",
                    Some(VoiceEvent::DictationStarted),
                ),
                VoiceState::Listening { .. } => (
                    "Stop dictation",
                    "Listening",
                    Some(VoiceEvent::DictationStopped),
                ),
                VoiceState::Transcribing => ("Transcribing", "Transcribing", None),
            };
        let indicator: AnyElement = match state {
            VoiceState::Listening { level } => breathing_with(
                MotionTokens::read(cx).breathing(),
                level_meter(level, cx),
                (root_id.clone(), "level-breath"),
            )
            .into_any_element(),
            VoiceState::Transcribing => Spinner::new().xsmall().into_any_element(),
            VoiceState::Idle | VoiceState::Speaking => div()
                .flex_none()
                .size(rems(0.5))
                .rounded(tokens.radius.full)
                .bg(cx.theme().muted_foreground.opacity(0.6))
                .into_any_element(),
        };
        let dictate_debug = debug_id.clone();
        let dictate = {
            let handler = handler.clone();
            let event = dictate_event.clone();
            outlined_control_with_label(
                (root_id.clone(), "dictate"),
                dictate_name,
                dictate_label,
                cx,
            )
            .debug_selector(move || format!("voice-dictate-{dictate_debug}"))
            .disabled(dictate_event.is_none() || handler.is_none())
            .when(state.is_listening(), |this| {
                this.border_color(cx.theme().primary)
                    .text_color(cx.theme().primary)
            })
            .gap(tokens.spacing.xs)
            .child(indicator)
            .on_click(move |_: &ClickEvent, window, cx| {
                if let (Some(handler), Some(event)) = (&handler, &event) {
                    handler(event, window, cx)
                }
            })
        };
        // The visible label is the last child of the pill; put the indicator
        // before it by rebuilding the children order.
        let cancel = (state.is_listening() && handler.is_some()).then(|| {
            let handler = handler.clone();
            let cancel_debug = debug_id.clone();
            icon_button(
                (root_id.clone(), "cancel"),
                IconName::Close,
                "Cancel dictation",
                cx,
            )
            .debug_selector(move || format!("voice-cancel-{cancel_debug}"))
            .on_click(move |_: &ClickEvent, window, cx| {
                if let Some(handler) = &handler {
                    handler(&VoiceEvent::DictationCancelled, window, cx)
                }
            })
        });
        let speak = self.speakable.then(|| {
            let handler = handler.clone();
            let speak_debug = debug_id.clone();
            let speaking = state == VoiceState::Speaking;
            let (name, icon, event) = if speaking {
                ("Stop speaking", IconName::Pause, VoiceEvent::SpeakStopped)
            } else {
                (
                    "Read the latest response aloud",
                    IconName::Play,
                    VoiceEvent::SpeakRequested,
                )
            };
            composed_button((root_id.clone(), "speak"), name)
                .debug_selector(move || format!("voice-speak-{speak_debug}"))
                .flex()
                .items_center()
                .gap(tokens.spacing.xs)
                .min_h(tokens.spacing.lg)
                .px(tokens.spacing.sm)
                .py(tokens.spacing.xxs)
                .rounded(tokens.radius.md)
                .border_1()
                .border_color(if speaking {
                    cx.theme().primary
                } else {
                    cx.theme().border
                })
                .text_token(tokens.typography.sm)
                .text_color(if speaking {
                    cx.theme().primary
                } else {
                    cx.theme().foreground
                })
                .hover(|style| style.bg(cx.theme().button_hover))
                .active(|style| style.bg(cx.theme().button_active))
                .focus_visible(|style| style.border_color(cx.theme().ring))
                .disabled(handler.is_none())
                .child(Icon::new(icon).xsmall())
                .child(div().child(if speaking { "Stop" } else { "Speak" }))
                .on_click(move |_: &ClickEvent, window, cx| {
                    if let Some(handler) = &handler {
                        handler(&event, window, cx)
                    }
                })
        });
        let transcript_debug = debug_id.clone();
        let transcript = self.transcript.map(|text| {
            div()
                .id((root_id.clone(), "transcript"))
                .role(Role::Status)
                .aria_label(format!("Heard: {text}"))
                .debug_selector(move || format!("voice-transcript-{transcript_debug}"))
                .min_w_0()
                .truncate()
                .text_token(tokens.typography.sm)
                .italic()
                .text_color(cx.theme().muted_foreground)
                .child(text)
        });

        v_flex()
            .id(self.id)
            .role(Role::Group)
            .aria_label(format!(
                "Voice controls, {}",
                state.label().to_ascii_lowercase()
            ))
            .debug_selector(move || format!("voice-{debug_id}"))
            .min_w_0()
            .gap(tokens.spacing.xs)
            .child(
                h_flex()
                    .items_center()
                    .gap(tokens.spacing.xs)
                    .child(dictate)
                    .children(cancel)
                    .children(speak),
            )
            .children(transcript)
            .refine_style(&self.style)
    }
}

/// Four bars that rise with the input level; heights stay in rems so the
/// meter scales with the UI.
fn level_meter(level: f32, cx: &App) -> gpui::Div {
    let tokens = cx.theme().semantic_tokens();
    let level = level.clamp(0.0, 1.0);
    const WEIGHTS: [f32; 4] = [0.55, 1.0, 0.75, 0.45];
    h_flex()
        .flex_none()
        .items_end()
        .h(rems(0.9))
        .gap(tokens.spacing.xxs)
        .children(WEIGHTS.iter().map(|weight| {
            div()
                .w(rems(0.18))
                .h(rems(0.25 + 0.65 * level * weight))
                .rounded(tokens.radius.full)
                .bg(cx.theme().primary)
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_describe_themselves() {
        assert_eq!(VoiceState::Listening { level: 0.2 }.label(), "Listening");
        assert!(VoiceState::Listening { level: 0.2 }.is_listening());
        assert!(!VoiceState::Transcribing.is_listening());
    }
}
