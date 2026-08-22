//! Streamed markdown answers with sources and follow-up suggestions.

use crate::stream::{ProgressState, StreamedContent};
use crate::theme::SemanticStyledExt as _;
use crate::{control::outlined_control, handlers::SharedHandler};
use gpui::{
    App, ClickEvent, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    RenderOnce, Role, ScrollHandle, SharedString, StatefulInteractiveElement as _, StyleRefinement,
    Styled, Window, div, prelude::FluentBuilder as _,
};
use gpui_base::Button;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, StyledExt as _, h_flex, text::TextView, v_flex,
};
use std::rc::Rc;

/// A source backing a streamed answer, shown as a chip under the text.
#[derive(Debug, Clone)]
pub struct SourceRef {
    title: SharedString,
}

/// A stable follow-up suggestion shown after an answer settles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    id: SharedString,
    label: SharedString,
}

/// A stable inline citation supplied by the application.
///
/// The destination is opaque application data. [`StreamingText`] never opens
/// it directly; activation returns it through
/// [`StreamingTextEvent::CitationActivated`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CitationRef {
    id: SharedString,
    label: SharedString,
    title: SharedString,
    destination: SharedString,
}

impl CitationRef {
    /// Creates a citation with stable identity and an application-owned target.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        title: impl Into<SharedString>,
        destination: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            title: title.into(),
            destination: destination.into(),
        }
    }

    /// Returns the stable application-level citation identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible citation label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// Returns the accessible citation title.
    pub fn title(&self) -> &SharedString {
        &self.title
    }

    /// Returns the opaque destination passed back on activation.
    pub fn destination(&self) -> &SharedString {
        &self.destination
    }

    fn internal_url(&self) -> String {
        format!("gpui-ai-citation://{}", percent_encode_id(&self.id))
    }
}

impl FollowUp {
    /// Creates a follow-up with a stable application-level identifier.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    fn selected_event(&self) -> StreamingTextEvent {
        StreamingTextEvent::FollowUpSelected {
            id: self.id.clone(),
        }
    }
}

/// An interaction emitted by [`StreamingText`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingTextEvent {
    /// A follow-up suggestion was selected.
    FollowUpSelected {
        /// Stable follow-up identifier.
        id: SharedString,
    },
    /// An inline citation was activated.
    CitationActivated {
        /// Stable application-level citation identifier.
        id: SharedString,
        /// Opaque application-owned navigation destination.
        destination: SharedString,
    },
}

fn percent_encode_id(id: &str) -> String {
    let mut encoded = String::with_capacity(id.len());
    for byte in id.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn escape_markdown_label(label: &str) -> String {
    let mut escaped = String::with_capacity(label.len());
    for character in label.chars() {
        if matches!(character, '\\' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn escape_markdown_title(title: &str) -> String {
    let mut escaped = String::with_capacity(title.len());
    for character in title.chars() {
        match character {
            '\\' | '"' => {
                escaped.push('\\');
                escaped.push(character);
            }
            '\r' | '\n' => escaped.push(' '),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[derive(Clone, Copy)]
struct FenceCandidate<'a> {
    marker: u8,
    length: usize,
    trailing: &'a str,
}

#[derive(Clone, Copy)]
struct OpenFence {
    marker: u8,
    length: usize,
    quote_depth: usize,
}

fn block_quote_content(mut line: &str) -> (usize, &str) {
    let mut depth = 0;
    loop {
        let bytes = line.as_bytes();
        let indentation = bytes.iter().take_while(|byte| **byte == b' ').count();
        if indentation > 3 || bytes.get(indentation) != Some(&b'>') {
            return (depth, line);
        }

        depth += 1;
        line = &line[indentation + 1..];
        if line.starts_with([' ', '\t']) {
            line = &line[1..];
        }
    }
}

fn fence_candidate(line: &str) -> Option<FenceCandidate<'_>> {
    let bytes = line.as_bytes();
    let indentation = bytes.iter().take_while(|byte| **byte == b' ').count();
    if indentation > 3 {
        return None;
    }
    let marker = *bytes.get(indentation)?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let length = bytes[indentation..]
        .iter()
        .take_while(|byte| **byte == marker)
        .count();
    (length >= 3).then_some(FenceCandidate {
        marker,
        length,
        trailing: &line[indentation + length..],
    })
}

fn opens_fence(candidate: FenceCandidate<'_>) -> bool {
    candidate.marker != b'`' || !candidate.trailing.contains('`')
}

fn closes_fence(candidate: FenceCandidate<'_>, open: OpenFence) -> bool {
    candidate.marker == open.marker
        && candidate.length >= open.length
        && candidate
            .trailing
            .trim_end_matches(['\r', '\n'])
            .bytes()
            .all(|byte| matches!(byte, b' ' | b'\t'))
}

#[cfg(test)]
fn transform_citation_markers(
    source: &str,
    citations: &[CitationRef],
    interactive: bool,
) -> String {
    transform_citations(source, citations, interactive).markdown
}

struct CitationTransform {
    markdown: String,
    referenced: Vec<CitationRef>,
}

fn transform_citations(
    source: &str,
    citations: &[CitationRef],
    interactive: bool,
) -> CitationTransform {
    let mut transformed = String::with_capacity(source.len());
    let mut referenced = Vec::new();
    let mut fence: Option<OpenFence> = None;
    let mut inline_ticks: Option<usize> = None;

    for line in source.split_inclusive('\n') {
        let (quote_depth, quote_content) = block_quote_content(line);
        let candidate = fence_candidate(quote_content);

        if let Some(open) = fence {
            if quote_depth >= open.quote_depth {
                transformed.push_str(line);
                if quote_depth == open.quote_depth
                    && candidate.is_some_and(|candidate| closes_fence(candidate, open))
                {
                    fence = None;
                }
                continue;
            }
            fence = None;
        }

        if inline_ticks.is_none()
            && let Some(candidate) = candidate
            && opens_fence(candidate)
        {
            fence = Some(OpenFence {
                marker: candidate.marker,
                length: candidate.length,
                quote_depth,
            });
            transformed.push_str(line);
            continue;
        }

        let mut offset = 0;
        while offset < line.len() {
            let remainder = &line[offset..];
            if remainder.starts_with('`') {
                let run = remainder.bytes().take_while(|byte| *byte == b'`').count();
                transformed.push_str(&remainder[..run]);
                match inline_ticks {
                    Some(open) if open == run => inline_ticks = None,
                    None => inline_ticks = Some(run),
                    _ => {}
                }
                offset += run;
                continue;
            }

            if inline_ticks.is_none()
                && let Some(marker) = remainder.strip_prefix("[[cite:")
                && let Some(end) = marker.find("]]")
            {
                let id = &marker[..end];
                if let Some(citation) = citations.iter().find(|citation| citation.id == id) {
                    if !referenced
                        .iter()
                        .any(|referenced: &CitationRef| referenced.id == citation.id)
                    {
                        referenced.push(citation.clone());
                    }
                    let label = escape_markdown_label(&citation.label);
                    if interactive {
                        transformed.push('[');
                        transformed.push_str(&label);
                        transformed.push_str("](");
                        transformed.push_str(&citation.internal_url());
                        transformed.push_str(" \"");
                        transformed.push_str(&escape_markdown_title(&citation.title));
                        transformed.push_str("\")");
                    } else {
                        transformed.push_str("\\[");
                        transformed.push_str(&label);
                        transformed.push_str("\\]");
                    }
                    offset += "[[cite:".len() + end + "]]".len();
                    continue;
                }
            }

            if let Some(character) = remainder.chars().next() {
                transformed.push(character);
                offset += character.len_utf8();
            } else {
                break;
            }
        }
    }

    CitationTransform {
        markdown: transformed,
        referenced,
    }
}

fn citation_event_for_url(url: &str, citations: &[CitationRef]) -> Option<StreamingTextEvent> {
    citations
        .iter()
        .find(|citation| citation.internal_url() == url)
        .map(|citation| StreamingTextEvent::CitationActivated {
            id: citation.id.clone(),
            destination: citation.destination.clone(),
        })
}

fn citation_companion_link(
    root_id: ElementId,
    citation: CitationRef,
    handler: SharedHandler<StreamingTextEvent>,
    cx: &mut App,
) -> gpui_base::Link {
    let tokens = cx.theme().semantic_tokens();
    let event = StreamingTextEvent::CitationActivated {
        id: citation.id.clone(),
        destination: citation.destination.clone(),
    };
    let local_id = SharedString::from(format!("citation-{}", citation.id));
    let debug_id = citation.id.to_string();
    let visible_label = SharedString::from(format!("Citation: {}", citation.label));

    gpui_base::Link::new((root_id, local_id))
        .debug_selector(move || format!("streaming-citation-{debug_id}"))
        .accessibility_label(citation.title)
        .flex()
        .flex_none()
        .items_center()
        .px(tokens.spacing.sm)
        .py(tokens.spacing.xxs)
        .border_1()
        .border_color(tokens.colors.border)
        .rounded(tokens.radius.sm)
        .bg(tokens.colors.surface)
        .text_token(tokens.typography.xs)
        .text_color(tokens.colors.surface_foreground)
        .hover(|style| style.bg(tokens.colors.accent))
        .active(|style| style.bg(tokens.colors.secondary))
        .focus(|style| style.border_color(tokens.colors.ring))
        .focus_visible(|style| style.border_color(tokens.colors.ring))
        .on_activate(move |_, window, cx| handler(&event, window, cx))
        .child(div().child(visible_label))
}

fn follow_up_button(
    follow_up: FollowUp,
    handler: Option<SharedHandler<StreamingTextEvent>>,
    cx: &mut App,
) -> Button {
    let debug_id = follow_up.id.to_string();
    let event = follow_up.selected_event();
    outlined_control(follow_up.id.clone(), follow_up.label, cx)
        .debug_selector(move || format!("streaming-follow-up-{debug_id}"))
        .accessibility_id(format!("follow-up-{}", follow_up.id))
        .when_some(handler, |this, handler| {
            this.on_click(move |_: &ClickEvent, window, cx| handler(&event, window, cx))
        })
}

struct CitationDispatcher {
    citations: Vec<CitationRef>,
    on_event: SharedHandler<StreamingTextEvent>,
}

impl CitationDispatcher {
    fn activate(&self, url: &str, event: &ClickEvent, window: &mut Window, cx: &mut App) -> bool {
        if event.is_right_click() {
            return true;
        }
        let Some(event) = citation_event_for_url(url, &self.citations) else {
            return false;
        };
        (self.on_event)(&event, window, cx);
        true
    }
}

impl SourceRef {
    /// Creates a source chip with a display title.
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            title: title.into(),
        }
    }
}

/// A streamed markdown answer: live text, then sources and follow-ups.
///
/// The component renders a [`StreamedContent`] snapshot — the application
/// owns the content and re-renders as chunks arrive. While streaming, a
/// cursor glyph marks the end of the text; on failure the reason is shown
/// under the answer.
///
/// # Example
///
/// ```ignore
/// StreamingText::new("answer", &self.answer)
///     .citations([
///         CitationRef::new("pricing", "Pricing report", "Open pricing report", "app://pricing"),
///     ])
///     .sources(["pricing.md", "suppliers.csv"])
///     .follow_ups([
///         FollowUp::new("delivery", "Compare delivery times"),
///         FollowUp::new("history", "Show price history"),
///     ])
///     .on_event(cx.listener(|this, event: &StreamingTextEvent, _, cx| {
///         // Route citation destinations or start follow-up work in the application.
///     }))
/// ```
#[derive(IntoElement)]
pub struct StreamingText {
    id: ElementId,
    style: StyleRefinement,
    text: SharedString,
    state: ProgressState,
    sources: Vec<SourceRef>,
    citations: Vec<CitationRef>,
    follow_ups: Vec<FollowUp>,
    on_event: Option<SharedHandler<StreamingTextEvent>>,
}

impl StreamingText {
    /// Creates the component from a snapshot of streamed content.
    pub fn new(id: impl Into<ElementId>, content: &StreamedContent) -> Self {
        Self {
            id: id.into(),
            style: StyleRefinement::default(),
            text: SharedString::from(content.text().to_string()),
            state: content.state().clone(),
            sources: Vec::new(),
            citations: Vec::new(),
            follow_ups: Vec::new(),
            on_event: None,
        }
    }

    /// Adds source chips shown once streaming has finished.
    pub fn sources(mut self, sources: impl IntoIterator<Item = impl Into<SharedString>>) -> Self {
        self.sources = sources.into_iter().map(SourceRef::new).collect();
        self
    }

    /// Adds pre-built [`SourceRef`]s (equivalent to [`Self::sources`] today;
    /// kept separate so richer source metadata can grow here).
    pub fn source_refs(mut self, sources: impl IntoIterator<Item = SourceRef>) -> Self {
        self.sources = sources.into_iter().collect();
        self
    }

    /// Adds inline citation metadata resolved from `[[cite:<stable-id>]]`
    /// markers in the streamed Markdown source.
    ///
    /// At the current upstream pin, the inline Markdown glyph is pointer-only.
    /// When an event handler is present, the component therefore renders a
    /// named companion Link as the keyboard and AccessKit authority; both
    /// representations emit the same [`StreamingTextEvent::CitationActivated`].
    pub fn citations(mut self, citations: impl IntoIterator<Item = CitationRef>) -> Self {
        self.citations = citations.into_iter().collect();
        self
    }

    /// Adds follow-up suggestions shown once streaming has finished.
    pub fn follow_ups(mut self, follow_ups: impl IntoIterator<Item = FollowUp>) -> Self {
        self.follow_ups = follow_ups.into_iter().collect();
        self
    }

    /// Handles typed answer interactions.
    pub fn on_event(
        mut self,
        handler: impl Fn(&StreamingTextEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_event = Some(Rc::new(handler));
        self
    }
}

impl Styled for StreamingText {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for StreamingText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let streaming = self.state == ProgressState::Running;
        let settled = self.state == ProgressState::Complete;
        let interactive_citations = self.on_event.is_some();
        let transformed = transform_citations(&self.text, &self.citations, interactive_citations);
        let source = if streaming {
            format!("{}▌", transformed.markdown)
        } else {
            transformed.markdown
        };
        let referenced_citations = transformed.referenced;
        let root_id = self.id.clone();
        let on_event = self.on_event.clone();
        let failure = match &self.state {
            ProgressState::Failed(reason) => Some(reason.clone()),
            _ => None,
        };
        let dispatcher = on_event.as_ref().map(|handler| {
            let initial_citations = referenced_citations.clone();
            let initial_handler = handler.clone();
            let dispatcher =
                window.use_keyed_state((root_id.clone(), "citation-dispatch"), cx, move |_, _| {
                    CitationDispatcher {
                        citations: initial_citations,
                        on_event: initial_handler,
                    }
                });
            dispatcher.update(cx, |dispatcher, _| {
                dispatcher.citations = referenced_citations.clone();
                dispatcher.on_event = handler.clone();
            });
            dispatcher
        });
        let mut answer = TextView::markdown((root_id.clone(), "answer"), source).selectable(true);
        if let Some(dispatcher) = dispatcher {
            let dispatcher = dispatcher.downgrade();
            answer = answer.on_link_click(move |url, event, window, cx| {
                let handled = dispatcher
                    .update(cx, |dispatcher, cx| {
                        dispatcher.activate(url, event, window, cx)
                    })
                    .is_ok_and(|handled| handled);
                if !handled && !event.is_right_click() {
                    cx.open_url(url);
                }
            });
        }
        let citation_scroll_handle = window
            .use_keyed_state((root_id.clone(), "citation-scroll-handle"), cx, |_, _| {
                ScrollHandle::new()
            })
            .read(cx)
            .clone();

        v_flex()
            .id(self.id)
            .w_full()
            .min_w_0()
            .role(Role::Article)
            .aria_label("Answer")
            .when_some(failure.clone(), |this, reason| {
                this.aria_description(reason)
            })
            .gap(tokens.spacing.md)
            .child(
                div()
                    .debug_selector(|| "streaming-text-body".to_owned())
                    .text_token(tokens.typography.sm)
                    .text_color(cx.theme().foreground)
                    .child(answer),
            )
            .when_some(failure, |this, reason| {
                this.child(
                    h_flex()
                        .items_center()
                        .gap(tokens.spacing.xs)
                        .text_token(tokens.typography.xs)
                        .text_color(cx.theme().danger)
                        .child(Icon::new(IconName::CircleX).xsmall())
                        .child(reason),
                )
            })
            .when_some(on_event.clone(), |this, handler| {
                this.when(!referenced_citations.is_empty(), |this| {
                    this.child(
                        v_flex()
                            .id((root_id.clone(), "inline-citations"))
                            .role(Role::Group)
                            .aria_label("Inline citation links")
                            .tab_group()
                            .gap(tokens.spacing.xxs)
                            .child(
                                h_flex()
                                    .id((root_id.clone(), "inline-citation-scroll"))
                                    .debug_selector(|| "streaming-citation-scroll".to_owned())
                                    .w_full()
                                    .max_w_full()
                                    .min_w_0()
                                    .overflow_x_scroll()
                                    .track_scroll(&citation_scroll_handle)
                                    .gap(tokens.spacing.xs)
                                    .children(referenced_citations.into_iter().map(|citation| {
                                        citation_companion_link(
                                            root_id.clone(),
                                            citation,
                                            handler.clone(),
                                            cx,
                                        )
                                    })),
                            ),
                    )
                })
            })
            .when(settled && !self.sources.is_empty(), |this| {
                this.child(
                    h_flex()
                        .id((root_id.clone(), "sources"))
                        .role(Role::List)
                        .aria_label("Sources")
                        .flex_wrap()
                        .gap(tokens.spacing.xs)
                        .children(self.sources.into_iter().enumerate().map(|(ix, source)| {
                            let accessibility_label = source.title.clone();
                            h_flex()
                                .id((root_id.clone(), ix.to_string()))
                                .role(Role::ListItem)
                                .aria_label(accessibility_label)
                                .items_center()
                                .gap(tokens.spacing.xs)
                                .px(tokens.spacing.sm)
                                .py(tokens.spacing.xxs)
                                .text_token(tokens.typography.xs)
                                .text_color(cx.theme().muted_foreground)
                                .bg(cx.theme().secondary)
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded(tokens.radius.full)
                                .child(Icon::new(IconName::File).xsmall())
                                .child(source.title)
                        })),
                )
            })
            .when(settled && !self.follow_ups.is_empty(), |this| {
                let handler = on_event.clone();
                this.child(
                    h_flex()
                        .id((root_id, "follow-ups"))
                        .role(Role::Group)
                        .aria_label("Follow-up suggestions")
                        .flex_wrap()
                        .gap(tokens.spacing.xs)
                        .children(
                            self.follow_ups
                                .into_iter()
                                .map(|follow_up| follow_up_button(follow_up, handler.clone(), cx)),
                        ),
                )
            })
            .refine_style(&self.style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stream::Progressive;
    use gpui::{Element as _, accesskit, canvas};
    use std::sync::{Arc, Mutex};

    fn citations() -> [CitationRef; 2] {
        [
            CitationRef::new("pricing", "Pricing", "Open pricing source", "app://pricing"),
            CitationRef::new(
                "supplier notes",
                "Supplier [notes]",
                "Open \"supplier\" notes",
                "app://suppliers?section=notes",
            ),
        ]
    }

    #[test]
    fn maps_all_shared_lifecycle_states() {
        let cases = [
            Progressive::pending("answer".to_owned()),
            Progressive::running("answer".to_owned()),
            Progressive::complete("answer".to_owned()),
            Progressive::failed("answer".to_owned(), "offline"),
        ];
        for content in cases {
            assert_eq!(
                StreamingText::new("answer", &content).state,
                *content.state()
            );
        }
    }

    #[test]
    fn duplicate_follow_up_labels_emit_distinct_stable_ids() {
        let first = FollowUp::new("first", "Try again");
        let second = FollowUp::new("second", "Try again");
        assert_eq!(
            first.selected_event(),
            StreamingTextEvent::FollowUpSelected { id: "first".into() }
        );
        assert_eq!(
            second.selected_event(),
            StreamingTextEvent::FollowUpSelected {
                id: "second".into()
            }
        );
    }

    #[test]
    fn complete_known_markers_become_internal_markdown_links() {
        assert_eq!(
            transform_citation_markers("See [[cite:pricing]].", &citations(), true),
            "See [Pricing](gpui-ai-citation://pricing \"Open pricing source\")."
        );
        assert_eq!(
            transform_citation_markers("See [[cite:supplier notes]].", &citations(), true),
            "See [Supplier \\[notes\\]](gpui-ai-citation://supplier%20notes \"Open \\\"supplier\\\" notes\")."
        );
    }

    #[test]
    fn unknown_and_incomplete_markers_remain_readable_literals() {
        assert_eq!(
            transform_citation_markers("Unknown [[cite:missing]].", &citations(), true),
            "Unknown [[cite:missing]]."
        );
        assert_eq!(
            transform_citation_markers("Partial [[cite:pricing", &citations(), true),
            "Partial [[cite:pricing"
        );
    }

    #[test]
    fn noninteractive_citations_render_visible_non_link_labels() {
        assert_eq!(
            transform_citation_markers("See [[cite:pricing]].", &citations(), false),
            "See \\[Pricing\\]."
        );
    }

    #[test]
    fn markers_inside_inline_and_fenced_code_are_not_rewritten() {
        let source = concat!(
            "Outside [[cite:pricing]] and `inline [[cite:pricing]]`.\n\n",
            "```text\n[[cite:pricing]]\n```\n",
            "~~~\n[[cite:pricing]]\n~~~\n"
        );
        let expected = concat!(
            "Outside [Pricing](gpui-ai-citation://pricing \"Open pricing source\") and `inline [[cite:pricing]]`.\n\n",
            "```text\n[[cite:pricing]]\n```\n",
            "~~~\n[[cite:pricing]]\n~~~\n"
        );
        assert_eq!(
            transform_citation_markers(source, &citations(), true),
            expected
        );
    }

    #[test]
    fn trailing_text_cannot_close_a_fenced_code_block() {
        let source = concat!(
            "```rust\n",
            "[[cite:pricing]]\n",
            "```not-a-close\n",
            "[[cite:pricing]]\n",
            "```\n",
            "Outside [[cite:pricing]]."
        );
        let expected = concat!(
            "```rust\n",
            "[[cite:pricing]]\n",
            "```not-a-close\n",
            "[[cite:pricing]]\n",
            "```\n",
            "Outside [Pricing](gpui-ai-citation://pricing \"Open pricing source\")."
        );

        assert_eq!(
            transform_citation_markers(source, &citations(), true),
            expected
        );
    }

    #[test]
    fn block_quote_fences_protect_markers_and_accept_longer_closers() {
        let source = concat!(
            "> ```rust\n",
            "> [[cite:pricing]]\n",
            "> ````\n",
            "Outside [[cite:pricing]]."
        );
        let expected = concat!(
            "> ```rust\n",
            "> [[cite:pricing]]\n",
            "> ````\n",
            "Outside [Pricing](gpui-ai-citation://pricing \"Open pricing source\")."
        );

        assert_eq!(
            transform_citation_markers(source, &citations(), true),
            expected
        );
    }

    #[test]
    fn duplicate_labels_route_by_stable_id_and_preserve_destination() {
        let refs = [
            CitationRef::new("first", "Report", "Open first", "app://first"),
            CitationRef::new("second", "Report", "Open second", "app://second"),
        ];

        assert_eq!(
            citation_event_for_url("gpui-ai-citation://second", &refs),
            Some(StreamingTextEvent::CitationActivated {
                id: "second".into(),
                destination: "app://second".into(),
            })
        );
    }

    #[test]
    fn only_complete_referenced_citations_receive_companion_controls() {
        let transformed = transform_citations(
            "[[cite:pricing]] again [[cite:pricing]] then [[cite:supplier notes",
            &citations(),
            true,
        );

        assert_eq!(transformed.referenced.len(), 1);
        assert_eq!(transformed.referenced[0].id(), "pricing");
    }

    struct CapturedCitationA11y {
        role: Option<Role>,
        node: accesskit::Node,
    }

    struct CitationA11yProbe {
        captured: Arc<Mutex<Option<CapturedCitationA11y>>>,
    }

    impl gpui::Render for CitationA11yProbe {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let element = citation_companion_link(
                        ElementId::from("answer"),
                        CitationRef::new(
                            "pricing",
                            "Pricing",
                            "Open pricing source",
                            "app://pricing",
                        ),
                        Rc::new(|_, _, _| {}),
                        cx,
                    )
                    .render(window, cx)
                    .into_element();
                    let role = element.a11y_role();
                    let mut node = accesskit::Node::new(role.unwrap_or(Role::Unknown));
                    element.write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") =
                        Some(CapturedCitationA11y { role, node });
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn companion_link_exposes_role_name_and_click_action(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| CitationA11yProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let captured = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("citation link should be captured");
        assert_eq!(captured.role, Some(Role::Link));
        assert_eq!(captured.node.label(), Some("Open pricing source"));
        assert!(captured.node.supports_action(accesskit::Action::Click));
    }

    struct FollowUpA11yProbe {
        captured: Arc<Mutex<Option<CapturedCitationA11y>>>,
    }

    impl gpui::Render for FollowUpA11yProbe {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let element = follow_up_button(
                        FollowUp::new("compare", "Compare suppliers"),
                        Some(Rc::new(|_, _, _| {})),
                        cx,
                    )
                    .render(window, cx)
                    .into_element();
                    let role = element.a11y_role();
                    let mut node = accesskit::Node::new(Role::Unknown);
                    element.write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") =
                        Some(CapturedCitationA11y { role, node });
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn follow_up_exposes_production_role_name_and_click_action(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| FollowUpA11yProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let captured = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("follow-up button should be captured");
        assert_eq!(captured.role, Some(Role::Button));
        assert_eq!(captured.node.label(), Some("Compare suppliers"));
        assert!(captured.node.supports_action(accesskit::Action::Click));
    }
}
