//! Selection-anchored actions for readable Markdown content.

use crate::control::composed_button;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, AppContext as _, Bounds, ElementId, Entity, EventEmitter, InteractiveElement as _,
    IntoElement, MouseButton, MouseDownEvent, MouseUpEvent, ParentElement as _, Pixels, Point,
    Render, Role, ScrollHandle, SharedString, Size, Stateful, StatefulInteractiveElement as _,
    Styled, Subscription, Window, div, point, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, ElementExt as _,
    text::{TextView, TextViewState},
};

/// One application-owned action offered for the current text selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionAction {
    id: SharedString,
    label: SharedString,
}

impl SelectionAction {
    /// Creates an action with stable application identity and a visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
        }
    }

    /// Returns the stable application identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible and accessible label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }
}

/// An interaction emitted by [`SelectionActions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionActionsEvent {
    /// The user invoked an action for a settled, non-empty selection.
    Invoked {
        /// Stable selection-surface identifier.
        id: SharedString,
        /// Stable application action identifier.
        action_id: SharedString,
        /// Trimmed selected text at activation time.
        selected_text: SharedString,
    },
}

fn invocation_event(
    id: impl Into<SharedString>,
    action: &SelectionAction,
    selected_text: impl Into<SharedString>,
) -> SelectionActionsEvent {
    SelectionActionsEvent::Invoked {
        id: id.into(),
        action_id: action.id.clone(),
        selected_text: selected_text.into(),
    }
}

fn settled_selection(text: &str) -> Option<&str> {
    let selected = text.trim();
    (!selected.is_empty()).then_some(selected)
}

fn clamp_pixels(value: Pixels, minimum: Pixels, maximum: Pixels) -> Pixels {
    if value < minimum {
        minimum
    } else if value > maximum {
        maximum
    } else {
        value
    }
}

fn clamp_anchor(
    pointer: Point<Pixels>,
    root: Bounds<Pixels>,
    overlay: Size<Pixels>,
    inset: Pixels,
) -> Point<Pixels> {
    let local = point(pointer.x - root.origin.x, pointer.y - root.origin.y);
    let max_x = (root.size.width - overlay.width - inset).max(inset);
    let max_y = (root.size.height - overlay.height - inset).max(inset);
    point(
        clamp_pixels(local.x, inset, max_x),
        clamp_pixels(local.y, inset, max_y),
    )
}

fn available_overlay_size(
    root: Bounds<Pixels>,
    preferred: Size<Pixels>,
    inset: Pixels,
) -> Size<Pixels> {
    let horizontal_insets = inset + inset;
    let vertical_insets = inset + inset;
    Size::new(
        preferred
            .width
            .min((root.size.width - horizontal_insets).max(Pixels::default())),
        preferred
            .height
            .min((root.size.height - vertical_insets).max(Pixels::default())),
    )
}

fn selection_control(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    cx: &mut App,
) -> gpui_base::Button {
    let tokens = cx.theme().semantic_tokens();
    let label = label.into();
    composed_button(id, label.clone())
        .flex()
        .items_center()
        .justify_center()
        .px(tokens.spacing.sm)
        .py(tokens.spacing.xs)
        .border_1()
        .border_color(tokens.colors.border)
        .rounded(tokens.radius.sm)
        .bg(tokens.colors.surface)
        .text_token(tokens.typography.sm)
        .text_color(tokens.colors.surface_foreground)
        .hover(|style| style.bg(tokens.colors.accent))
        .active(|style| style.bg(tokens.colors.secondary))
        .focus_visible(|style| style.border_color(tokens.colors.ring))
        .child(div().child(label))
}

fn selection_toolbar_frame(
    id: SharedString,
    anchor: Point<Pixels>,
    maximum_size: Size<Pixels>,
    scroll_handle: &ScrollHandle,
    cx: &mut App,
) -> Stateful<gpui::Div> {
    let tokens = cx.theme().semantic_tokens();
    div()
        .id((ElementId::from(id), "toolbar"))
        .debug_selector(|| "selection-actions-toolbar".to_owned())
        .role(Role::Toolbar)
        .aria_label("Selection actions")
        .tab_group()
        .absolute()
        .occlude()
        .left(anchor.x)
        .top(anchor.y)
        .max_w(maximum_size.width)
        .max_h(maximum_size.height)
        .overflow_x_scroll()
        .track_scroll(scroll_handle)
        .flex()
        .items_center()
        .gap(tokens.spacing.xs)
        .p(tokens.spacing.xs)
        .border_1()
        .border_color(tokens.colors.border)
        .rounded(tokens.radius.md)
        .bg(tokens.colors.surface)
        .shadow(tokens.shadow.md.clone())
}

fn defer_selection_settle(
    entity: Entity<SelectionActions>,
    pointer: Point<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    window.defer(cx, move |_, cx| {
        entity.update(cx, |this, cx| this.settle_selection(Some(pointer), cx));
    });
}

/// Selectable Markdown with application-owned actions anchored to a settled selection.
///
/// The entity owns one upstream [`TextViewState`], preserving native GPUI
/// selection, copy, and select-all behavior. It emits immutable action
/// requests; applications retain all durable results and asynchronous work.
///
/// # Example
///
/// ```ignore
/// let selection = cx.new(|cx| {
///     SelectionActions::new("answer", "Select part of this answer", window, cx)
/// });
/// selection.update(cx, |selection, cx| {
///     selection.set_actions([
///         SelectionAction::new("ask", "Ask"),
///         SelectionAction::new("explain", "Explain"),
///     ], cx);
/// });
/// ```
pub struct SelectionActions {
    id: SharedString,
    markdown: SharedString,
    text: Entity<TextViewState>,
    actions: Vec<SelectionAction>,
    selected_text: SharedString,
    drag_active: bool,
    pointer_anchor: Option<Point<Pixels>>,
    root_bounds: Option<Bounds<Pixels>>,
    toolbar_scroll: ScrollHandle,
    toolbar_pointer_active: bool,
    _subscriptions: Vec<Subscription>,
}

impl SelectionActions {
    /// Creates a selectable Markdown surface.
    pub fn new(
        id: impl Into<SharedString>,
        markdown: impl Into<SharedString>,
        _window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let markdown = markdown.into();
        let text = cx.new(|cx| TextViewState::markdown(markdown.as_ref(), cx).selectable(true));
        let observation = cx.observe(&text, |this, text, cx| {
            if this.drag_active {
                return;
            }
            let raw = text.read(cx).selected_text();
            if settled_selection(&raw).is_some() {
                this.apply_selection(&raw, None, cx);
            } else {
                let selection = cx.weak_entity();
                cx.defer(move |cx| {
                    let _ = selection.update(cx, |this, cx| {
                        if this.drag_active || this.toolbar_pointer_active {
                            return;
                        }
                        let raw = this.text.read(cx).selected_text();
                        if settled_selection(&raw).is_none() {
                            this.apply_selection(&raw, None, cx);
                        }
                    });
                });
            }
        });
        Self {
            id: id.into(),
            markdown,
            text,
            actions: Vec::new(),
            selected_text: SharedString::default(),
            drag_active: false,
            pointer_anchor: None,
            root_bounds: None,
            toolbar_scroll: ScrollHandle::new(),
            toolbar_pointer_active: false,
            _subscriptions: vec![observation],
        }
    }

    /// Replaces the Markdown and clears any stale selection and toolbar.
    pub fn set_markdown(
        &mut self,
        markdown: impl Into<SharedString>,
        cx: &mut gpui::Context<Self>,
    ) {
        let markdown = markdown.into();
        if self.markdown == markdown {
            return;
        }
        self.markdown = markdown.clone();
        self.text
            .update(cx, |text, cx| text.set_text(markdown.as_ref(), cx));
        self.selected_text = SharedString::default();
        self.pointer_anchor = None;
        self.drag_active = false;
        self.toolbar_pointer_active = false;
        cx.notify();
    }

    /// Replaces the application-owned action catalog.
    pub fn set_actions(
        &mut self,
        actions: impl IntoIterator<Item = SelectionAction>,
        cx: &mut gpui::Context<Self>,
    ) {
        self.actions = actions.into_iter().collect();
        cx.notify();
    }

    /// Returns the current settled, trimmed selection.
    pub fn selected_text(&self) -> &SharedString {
        &self.selected_text
    }

    /// Clears native and composite selection state.
    pub fn clear_selection(&mut self, cx: &mut gpui::Context<Self>) {
        self.text.update(cx, TextViewState::clear_selection);
        self.selected_text = SharedString::default();
        self.pointer_anchor = None;
        self.drag_active = false;
        self.toolbar_pointer_active = false;
        cx.notify();
    }

    fn settle_selection(&mut self, pointer: Option<Point<Pixels>>, cx: &mut gpui::Context<Self>) {
        self.drag_active = false;
        let raw = self.text.read(cx).selected_text();
        self.apply_selection(&raw, pointer, cx);
    }

    fn clear_cached_selection(&mut self, cx: &mut gpui::Context<Self>) {
        let changed = !self.selected_text.is_empty() || self.pointer_anchor.is_some();
        self.selected_text = SharedString::default();
        self.pointer_anchor = None;
        self.drag_active = false;
        self.toolbar_pointer_active = false;
        if changed {
            cx.notify();
        }
    }

    fn apply_selection(
        &mut self,
        raw: &str,
        pointer: Option<Point<Pixels>>,
        cx: &mut gpui::Context<Self>,
    ) {
        let selected = settled_selection(raw).unwrap_or_default();
        let next: SharedString = selected.to_owned().into();
        let changed = self.selected_text != next;
        self.selected_text = next;
        if self.selected_text.is_empty() {
            self.pointer_anchor = None;
        } else if let Some(pointer) = pointer {
            self.pointer_anchor = Some(pointer);
        } else if self.pointer_anchor.is_none() {
            self.pointer_anchor = self.root_bounds.map(|bounds| bounds.center());
        }
        if changed || !self.selected_text.is_empty() {
            cx.notify();
        }
    }

    fn toolbar(&self, cx: &mut gpui::Context<Self>) -> Stateful<gpui::Div> {
        let root_id = self.id.clone();
        let selected_text = self.selected_text.clone();
        let actions = self.actions.clone();
        let tokens = cx.theme().semantic_tokens();
        let preferred_size = Size::new(tokens.spacing.xxl * 9., tokens.spacing.xxl * 2.);
        let inset = tokens.spacing.sm;
        let maximum_size = self
            .root_bounds
            .map(|bounds| available_overlay_size(bounds, preferred_size, inset))
            .unwrap_or(preferred_size);
        let maximum_control_width = (maximum_size.width - tokens.spacing.md).max(Pixels::default());
        let buttons = actions
            .into_iter()
            .map(|action| {
                let event = invocation_event(root_id.clone(), &action, selected_text.clone());
                let action_id = action.id.clone();
                let label = action.label.clone();
                selection_control(
                    (ElementId::from(root_id.clone()), action_id.clone()),
                    label.clone(),
                    cx,
                )
                .max_w(maximum_control_width)
                .overflow_hidden()
                .debug_selector(move || format!("selection-action-{action_id}"))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        if event.button == MouseButton::Left {
                            this.toolbar_pointer_active = true;
                            cx.stop_propagation();
                        }
                    }),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.emit(event.clone());
                    this.toolbar_pointer_active = false;
                }))
            })
            .collect::<Vec<_>>();

        selection_toolbar_frame(
            self.id.clone(),
            self.toolbar_anchor(maximum_size, inset),
            maximum_size,
            &self.toolbar_scroll,
            cx,
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, _, _| {
                this.toolbar_pointer_active = false;
            }),
        )
        .children(buttons)
    }

    fn toolbar_anchor(&self, overlay_size: Size<Pixels>, inset: Pixels) -> Point<Pixels> {
        match (self.pointer_anchor, self.root_bounds) {
            (Some(pointer), Some(bounds)) => {
                let below_selection = point(pointer.x, pointer.y + inset + inset);
                clamp_anchor(below_selection, bounds, overlay_size, inset)
            }
            _ => point(inset, inset),
        }
    }
}

impl EventEmitter<SelectionActionsEvent> for SelectionActions {}

impl Render for SelectionActions {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let root_id = self.id.clone();
        let entity_for_layout = cx.entity().clone();
        let text_surface = div()
            .id((ElementId::from(root_id.clone()), "text"))
            .size_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.toolbar_pointer_active {
                        return;
                    }
                    this.drag_active = true;
                    this.toolbar_pointer_active = false;
                    this.selected_text = SharedString::default();
                    this.pointer_anchor = None;
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|_, event: &MouseUpEvent, window, cx| {
                    defer_selection_settle(cx.entity().clone(), event.position, window, cx);
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|_, event: &MouseUpEvent, window, cx| {
                    defer_selection_settle(cx.entity().clone(), event.position, window, cx);
                }),
            )
            .on_key_up(cx.listener(|_, _, window, cx| {
                let entity = cx.entity().clone();
                window.defer(cx, move |_, cx| {
                    entity.update(cx, |this, cx| this.settle_selection(None, cx));
                });
            }))
            .child(TextView::new(&self.text).selectable(true));

        div()
            .id(ElementId::from(root_id))
            .debug_selector(|| "selection-actions-surface".to_owned())
            .role(Role::Group)
            .aria_label("Selectable content with actions")
            .relative()
            .size_full()
            .min_h(tokens.spacing.xxl * 3.)
            .overflow_y_scroll()
            .p(tokens.spacing.md)
            .border_1()
            .border_color(tokens.colors.border)
            .rounded(tokens.radius.lg)
            .bg(tokens.colors.background)
            .on_mouse_down_out(cx.listener(|this, event: &MouseDownEvent, _, cx| {
                if event.button == MouseButton::Left {
                    this.clear_cached_selection(cx);
                }
            }))
            .on_prepaint(move |bounds, _, cx| {
                entity_for_layout.update(cx, |this, cx| {
                    if this.root_bounds != Some(bounds) {
                        this.root_bounds = Some(bounds);
                        cx.notify();
                    }
                });
            })
            .child(text_surface)
            .when(
                !self.drag_active && !self.selected_text.is_empty() && !self.actions.is_empty(),
                |surface| surface.child(self.toolbar(cx)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Bounds, Element as _, RenderOnce as _, Size, TestAppContext, accesskit, canvas, point, px,
        size,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn duplicate_labels_keep_stable_action_identity() {
        let actions = vec![
            SelectionAction::new("ask-short", "Ask"),
            SelectionAction::new("ask-deep", "Ask"),
        ];

        let event = invocation_event("answer", &actions[1], "selected words");

        assert_eq!(
            event,
            SelectionActionsEvent::Invoked {
                id: "answer".into(),
                action_id: "ask-deep".into(),
                selected_text: "selected words".into(),
            }
        );
    }

    #[test]
    fn whitespace_only_selection_is_suppressed() {
        assert_eq!(settled_selection("  \n\t  "), None);
        assert_eq!(settled_selection("  useful words  "), Some("useful words"));
    }

    #[test]
    fn root_relative_anchor_is_clamped_inside_the_surface() {
        let root = Bounds::new(point(px(100.), px(40.)), size(px(240.), px(120.)));
        let overlay: Size<_> = size(px(90.), px(32.));

        assert_eq!(
            clamp_anchor(point(px(334.), px(156.)), root, overlay, px(8.)),
            point(px(142.), px(80.))
        );
        assert_eq!(
            clamp_anchor(point(px(80.), px(20.)), root, overlay, px(8.)),
            point(px(8.), px(8.))
        );
    }

    struct ControlProbe {
        captured: Arc<Mutex<Option<accesskit::Node>>>,
        toolbar: Arc<Mutex<Option<accesskit::Node>>>,
    }

    impl Render for ControlProbe {
        fn render(&mut self, _: &mut Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            let toolbar = self.toolbar.clone();
            canvas(
                move |_, window, cx| {
                    let mut node = accesskit::Node::new(Role::Button);
                    selection_control("ask", "Ask about selection", cx)
                        .on_click(|_, _, _| {})
                        .render(window, cx)
                        .into_element()
                        .write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") = Some(node);
                    let toolbar_element = selection_toolbar_frame(
                        "probe".into(),
                        point(px(0.), px(0.)),
                        size(px(240.), px(64.)),
                        &ScrollHandle::new(),
                        cx,
                    )
                    .into_element();
                    let mut toolbar_node =
                        accesskit::Node::new(toolbar_element.a11y_role().unwrap_or(Role::Unknown));
                    toolbar_element.write_a11y_info(&mut toolbar_node);
                    *toolbar.lock().expect("toolbar mutex should be available") =
                        Some(toolbar_node);
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn production_action_control_is_named_and_keyboard_activatable(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let toolbar = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let toolbar_result = toolbar.clone();
        let (_, cx) = cx.add_window_view(move |_, _| ControlProbe { captured, toolbar });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let node = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("button node should be captured");
        assert_eq!(node.role(), Role::Button);
        assert_eq!(node.label(), Some("Ask about selection"));
        assert!(node.supports_action(accesskit::Action::Click));

        let toolbar = toolbar_result
            .lock()
            .expect("toolbar mutex should be available")
            .take()
            .expect("toolbar node should be captured");
        assert_eq!(toolbar.role(), Role::Toolbar);
        assert_eq!(toolbar.label(), Some("Selection actions"));
    }
}
