use gpui::{App, Application, WindowOptions, prelude::*};
use gpui_ai::prelude::*;
use gpui_component::Root;

struct Agent;

impl Render for Agent {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ToolChip::new("edit", "Edit main.rs").status(ToolStatus::Running)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        gpui_component::init(cx);
        gpui_ai::init(cx);

        cx.open_window(WindowOptions::default(), |window, cx| {
            let agent = cx.new(|_| Agent);
            // Root owns the theme, the rem size, and the layers dialogs and
            // popovers are drawn into. Every gpui-component app has one.
            cx.new(|cx| Root::new(agent.into(), window, cx))
        })
        .unwrap();
    });
}
