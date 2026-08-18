use gpui::{ElementId, SharedString};
use gpui_base::Button;

pub(crate) fn composed_button(
    id: impl Into<ElementId>,
    accessibility_label: impl Into<SharedString>,
) -> Button {
    Button::new(id).accessibility_label(accessibility_label)
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
        captured: Arc<Mutex<Option<accesskit::Node>>>,
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
                    let mut node = accesskit::Node::new(Role::Button);
                    composed_button("probe", "Open pricing.md")
                        .on_click(|_, _, _| {})
                        .render(window, cx)
                        .into_element()
                        .write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") = Some(node);
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

        let node = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("button node should be captured");
        assert_eq!(node.role(), Role::Button);
        assert_eq!(node.label(), Some("Open pricing.md"));
        assert!(node.supports_action(accesskit::Action::Click));
    }
}
