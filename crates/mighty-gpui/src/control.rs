use crate::theme::SemanticStyledExt as _;
use gpui::{
    App, ElementId, InteractiveElement as _, ParentElement as _, SharedString,
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
    let tokens = cx.theme().semantic_tokens();
    let label = accessibility_label.into();
    composed_button(id, label.clone())
        .flex()
        .items_center()
        .justify_center()
        .px(tokens.spacing.sm)
        .py(tokens.spacing.xxs)
        .border_1()
        .border_color(cx.theme().border)
        .rounded(tokens.radius.sm)
        .bg(cx.theme().transparent)
        .text_token(tokens.typography.xs)
        .text_color(cx.theme().foreground)
        .hover(|style| style.bg(cx.theme().button_hover))
        .active(|style| style.bg(cx.theme().button_active))
        .focus_visible(|style| style.border_color(cx.theme().ring))
        .child(div().child(label))
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Element as _, IntoElement as _, Render, RenderOnce as _, Role, TestAppContext, Window,
        accesskit, canvas,
    };
    use std::sync::{Arc, Mutex};

    struct Probe {
        captured: Arc<Mutex<Option<(Option<Role>, accesskit::Node)>>>,
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
