//! Theme-aware text labels for composed gpui-component buttons.

use gpui::{
    App, InteractiveElement as _, IntoElement, ParentElement as _, Pixels, RenderOnce,
    SharedString, Styled as _, Window, div, relative,
};
use gpui_component::{ActiveTheme as _, button::Button};

/// Adds a text label without squeezing its glyphs into a one-em clipping box.
///
/// This composes the upstream button's child slot: sizes, colors, focus,
/// activation, icons, and disabled states remain owned by gpui-component.
/// The label inherits the button's font size and the theme's body-text leading.
pub trait ButtonLabelExt {
    /// Sets the visible text and accessible name. Use instead of `Button::label`.
    ///
    /// A later `accessibility_label` call can supply a different spoken name.
    /// Long labels still truncate horizontally; descenders and accents retain
    /// the theme's line-height clearance. Call once per button.
    ///
    /// ```
    /// use gpui_ai::ButtonLabelExt as _;
    /// use gpui_component::button::Button;
    ///
    /// let button = Button::new("apply").text_label("Apply changes");
    /// ```
    fn text_label(self, label: impl Into<SharedString>) -> Self;
}

impl ButtonLabelExt for Button {
    fn text_label(self, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        self.accessibility_label(label.clone())
            .child(ButtonText { label })
    }
}

#[derive(IntoElement)]
struct ButtonText {
    label: SharedString,
}

impl RenderOnce for ButtonText {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let body = cx.theme().typography_tokens().sm;
        // A relative leading follows every upstream Button size and rem zoom.
        // In particular, do not use Button::label's one-em overflow mask: on
        // DirectWrite it cuts off the bottom of y/g/p and tall accented glyphs.
        // Named after what it says, so a geometry test can find it. Dropped
        // unread outside cfg(test), so the name costs nothing to carry.
        let named = self.label.clone();
        div()
            .debug_selector(move || format!("button-label-{named}"))
            .min_w_0()
            .truncate()
            .line_height(relative(f32::from(body.line_height) / f32::from(body.size)))
            // Padding below, so centring the taller box raises the glyphs by
            // half of it. Growing the box is the only lever here: the button
            // centres whatever it is given, and a line box cannot say where
            // its baseline should land.
            .pb(cap_lift(window, cx, body.size) * 2.0)
            .child(self.label)
    }
}

/// How far a label has to rise for its ink to sit in the middle of a control.
///
/// GPUI centres a line's **ascent-to-descent** box: `padding_top` is
/// `(line_height - ascent - descent) / 2` and the baseline follows the ascent.
/// That box is not what anyone looks at. A font's ascent reserves room for
/// accents and tall diacritics that a word like `Approve` never uses, so the
/// glyphs hang low inside it — measured on a 24px button, eight pixels above
/// the caps and three below the descender.
///
/// The box people actually see runs from the cap height to the descent, and
/// all of the ascent-to-descent box's excess over it sits on top. Removing
/// half of that excess centres the visible one. Descenders keep hanging below
/// the baseline, which is what descenders are for, but they stop touching the
/// edge.
///
/// It is a font metric rather than a chosen number, so it follows the theme's
/// typeface and size without anyone re-tuning it — and it costs two cached
/// metric reads per label, not a measured layout.
fn cap_lift(window: &Window, cx: &App, size: Pixels) -> Pixels {
    let text = cx.text_system();
    let font = text.resolve_font(&window.text_style().font());
    ((text.ascent(font, size) - text.cap_height(font, size)) / 2.0).max(Pixels::ZERO)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Context, Element as _, Render, Role, TestAppContext, accesskit, canvas,
        prelude::FluentBuilder as _,
    };
    use gpui_component::Disableable as _;
    use std::sync::{Arc, Mutex};

    struct Probe {
        nodes: Arc<Mutex<Vec<accesskit::Node>>>,
    }

    impl Render for Probe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let nodes = self.nodes.clone();
            canvas(
                move |_, window, cx| {
                    for disabled in [false, true] {
                        let mut button = Button::new("label-probe")
                            .text_label("Apply gyjpq ÅÉ ﬁ ﬂ")
                            .disabled(disabled)
                            .when(disabled, |button| button.accessibility_label("Unavailable"))
                            .on_click(|_, _, _| {})
                            .render(window, cx)
                            .into_any_element();
                        // The styled Button composes a base Button. Resolve its
                        // view before inspecting the actual semantic div, not
                        // the transparent RenderOnce wrapper (which has no role).
                        let (_, mut rendered) = button
                            .downcast_mut::<gpui::ViewElement<gpui_base::Button>>()
                            .expect("upstream styled button composes base Button")
                            .request_layout(None, None, window, cx);
                        let button = rendered
                            .as_mut()
                            .expect("base button renders a child")
                            .downcast_mut::<gpui::Stateful<gpui::Div>>()
                            .expect("base button renders its semantic div");
                        assert_eq!(button.a11y_role(), Some(Role::Button));
                        let mut node = accesskit::Node::new(Role::Button);
                        button.write_a11y_info(&mut node);
                        nodes.lock().expect("capture lock").push(node);
                    }
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn composed_label_keeps_upstream_name_and_activation(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let nodes = Arc::new(Mutex::new(Vec::new()));
        let result = nodes.clone();
        let (_, cx) = cx.add_window_view(|_, _| Probe { nodes });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let nodes = result.lock().expect("capture lock");
        assert_eq!(nodes[0].label(), Some("Apply gyjpq ÅÉ ﬁ ﬂ"));
        assert!(nodes[0].supports_action(accesskit::Action::Click));
        assert!(!nodes[0].is_disabled());
        assert_eq!(nodes[1].label(), Some("Unavailable"));
        assert!(!nodes[1].supports_action(accesskit::Action::Click));
    }
}
