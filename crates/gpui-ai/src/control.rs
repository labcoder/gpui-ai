use crate::sizing::SizeTokens;
use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ElementId, InteractiveElement as _, MouseButton, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div,
};
use gpui_base::Button;
use gpui_component::ActiveTheme as _;

pub(crate) fn composed_button(
    id: impl Into<ElementId>,
    accessibility_label: impl Into<SharedString>,
) -> Button {
    Button::new(id).accessibility_label(accessibility_label)
}

pub(crate) fn outlined_control(
    id: impl Into<ElementId>,
    accessibility_label: impl Into<SharedString>,
    cx: &mut App,
) -> Button {
    let label = accessibility_label.into();
    outlined_control_with_label(id, label.clone(), label, cx)
}

pub(crate) fn outlined_control_with_label(
    id: impl Into<ElementId>,
    accessibility_label: impl Into<SharedString>,
    visible_label: impl Into<SharedString>,
    cx: &mut App,
) -> Button {
    let tokens = cx.theme().semantic_tokens();
    let sizes = SizeTokens::read(cx);
    composed_button(id, accessibility_label)
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            gpui_base::GlobalState::suppress_text_selection(cx);
        })
        .flex()
        .items_center()
        .justify_center()
        // Compact pill geometry shared by every small control in the
        // library — filter chips, row CTAs, toggles. One height from the
        // size policy, one radius, one text style, so controls look like
        // one family next to tables.
        .h(sizes.control_sm())
        .px(tokens.spacing.sm)
        .border_1()
        .border_color(cx.theme().border)
        .rounded(tokens.radius.md)
        .bg(cx.theme().transparent)
        .text_token(tokens.typography.sm)
        .text_color(cx.theme().foreground)
        .hover(|style| style.bg(cx.theme().button_hover))
        .active(|style| style.bg(cx.theme().button_active))
        .focus_visible(|style| style.border_color(cx.theme().ring))
        // Compact controls are often placed in fixed-width table columns.
        // Keep their visible label inside the control instead of allowing a
        // long title to paint over the adjacent column; the full accessible
        // label remains on the Button.
        .child(div().min_w_0().truncate().child(visible_label.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Element as _, IntoElement as _, Render, RenderOnce as _, Role, TestAppContext, Window,
        accesskit, canvas,
    };
    use std::sync::{Arc, Mutex};

    mod control_metrics {
        use super::super::*;
        use crate::sizing::SizeTokens;
        use gpui::{
            Context, IntoElement, Render, TestAppContext, VisualTestContext, Window, div, px,
        };
        use gpui_component::h_flex;

        struct MetricsProbe;

        impl Render for MetricsProbe {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div().w(px(420.)).h(px(160.)).child(
                    h_flex()
                        .items_start()
                        .gap(px(8.))
                        .child(
                            outlined_control("metrics-outlined", "Open", cx)
                                .debug_selector(|| "metrics-outlined".into()),
                        )
                        .child(
                            crate::surface::icon_button(
                                "metrics-icon",
                                gpui_component::IconName::Search,
                                "Search",
                                cx,
                            )
                            .debug_selector(|| "metrics-icon".into()),
                        ),
                )
            }
        }

        /// Every composed control resolves its height through the size
        /// policy — the six coincidental heights the 0.4.0 audit found
        /// cannot come back without failing here.
        #[gpui::test]
        fn composed_controls_take_their_heights_from_the_size_policy(cx: &mut TestAppContext) {
            cx.update(crate::init);
            let (_, cx) = cx.add_window_view(|_, _| MetricsProbe);
            let cx: &mut VisualTestContext = cx;
            cx.update(|window, cx| window.draw(cx).clear(cx));

            let sizes = cx.update(|_, cx| *SizeTokens::read(cx));
            let outlined = cx
                .debug_bounds("metrics-outlined")
                .expect("the outlined control should render");
            assert_eq!(
                outlined.size.height,
                sizes.control_sm(),
                "outlined_control must stand exactly control_sm tall"
            );
            let icon = cx
                .debug_bounds("metrics-icon")
                .expect("the icon button should render");
            assert_eq!(
                icon.size.height,
                sizes.control_sm(),
                "icon buttons are control_sm"
            );
            assert_eq!(
                icon.size.width,
                sizes.control_sm(),
                "icon buttons are square"
            );
        }

        /// The policy is a policy: replacing it before init moves every
        /// composed control, which is the customization contract.
        #[gpui::test]
        fn a_replaced_size_policy_moves_the_composed_controls(cx: &mut TestAppContext) {
            cx.update(|cx| {
                SizeTokens::default().with_control_sm(px(30.)).set(cx);
                crate::init(cx);
            });
            let (_, cx) = cx.add_window_view(|_, _| MetricsProbe);
            let cx: &mut VisualTestContext = cx;
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let outlined = cx
                .debug_bounds("metrics-outlined")
                .expect("the outlined control should render");
            assert_eq!(outlined.size.height, px(30.));
        }
    }

    type CapturedNode = Arc<Mutex<Option<(Option<Role>, accesskit::Node)>>>;

    struct Probe {
        captured: CapturedNode,
    }

    impl Render for Probe {
        fn render(
            &mut self,
            _: &mut Window,
            _: &mut gpui::Context<Self>,
        ) -> impl gpui::IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let element = composed_button("probe", "Open pricing.md")
                        .on_click(|_, _, _| {})
                        .render(window, cx)
                        .into_element();
                    let role = element.a11y_role();
                    let mut node = accesskit::Node::new(Role::Unknown);
                    element.write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") =
                        Some((role, node));
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn composed_button_exposes_its_name_and_activation(cx: &mut TestAppContext) {
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| Probe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let (role, node) = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("button node should be captured");
        assert_eq!(role, Some(Role::Button));
        assert_eq!(node.label(), Some("Open pricing.md"));
        assert!(node.supports_action(accesskit::Action::Click));
    }
}
