//! Proof that gpui-ai components compose inside upstream's `DockArea`.
//!
//! gpui-ai ships no docking of its own and implements no `Panel`: a panel's
//! name, title, persistence, zoom, and close behavior are application
//! decisions, and a library that answered them would be choosing for its
//! consumers. What the library owes a dock host is a component that stops
//! insisting on its own box — [`SidebarNavPresentation::Embedded`] — and this
//! module is the host side of that bargain, written once here so the seam has
//! a consumer that exercises it.
//!
//! The adapters below are deliberately gallery-only. They are what an
//! application would write, not what the library should have written.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyView, App, Context, Div, ElementId, Entity, EventEmitter, FocusHandle, Focusable, Render,
    Role, SharedString, Stateful, Subscription, Window, div, rems,
};
use gpui_ai::prelude::*;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::dock::{
    BasePanel, DockArea, DockLayout, DockPlacement, DockSkin, Panel, PanelEvent, panel_handle,
};
use gpui_component::{ActiveTheme as _, Selectable as _, h_flex, v_flex};

/// Width of the sidebar's dock along a vertical edge.
const SIDE_DOCK_SIZE_IN_REMS: f32 = 14.25;
/// Height of the sidebar's dock along the bottom edge.
///
/// Shorter than [`SIDE_DOCK_SIZE_IN_REMS`] and far wider: a bottom dock is the case
/// that fails when a navigation keeps a rail's width, which is the whole
/// reason this story can move the sidebar.
const BOTTOM_DOCK_SIZE_IN_REMS: f32 = 9.375;
/// Initial width of the thread-list pane in the center split.
const THREAD_PANE_SIZE_IN_REMS: f32 = 14.5;
/// Initial height of the artifact pane in the nested vertical split.
const ARTIFACT_PANE_SIZE_IN_REMS: f32 = 11.25;

/// A gallery-only [`Panel`] wrapping one embedded [`SidebarNav`].
///
/// Everything the dock asks a panel — its stable name, its tab title, whether
/// it may be closed or zoomed — is answered here rather than by the component,
/// because every one of those answers belongs to the application that placed
/// it. The component's only contribution is that it fills the box the panel
/// gives it.
pub(crate) struct DockNavPanel {
    nav: Entity<SidebarNav>,
    name: &'static str,
    title: SharedString,
    focus: FocusHandle,
}

impl DockNavPanel {
    /// Wraps `nav` under a stable panel name and a tab title.
    ///
    /// The panel does not set the presentation: an embedded navigation is what
    /// the caller must hand it, and a standalone one placed here would keep a
    /// rail's width inside a dock that already sized it.
    pub(crate) fn new(
        nav: Entity<SidebarNav>,
        name: &'static str,
        title: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        debug_assert_eq!(
            nav.read(cx).presentation(),
            SidebarNavPresentation::Embedded,
            "a docked navigation must be embedded so the dock owns its size",
        );
        Self {
            nav,
            name,
            title: title.into(),
            focus: cx.focus_handle(),
        }
    }

    /// The wrapped navigation, for a host that drives it after docking.
    #[cfg(test)]
    pub(crate) fn nav(&self) -> &Entity<SidebarNav> {
        &self.nav
    }
}

impl EventEmitter<PanelEvent> for DockNavPanel {}

impl Focusable for DockNavPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BasePanel for DockNavPanel {
    fn panel_name(&self) -> &'static str {
        self.name
    }
}

impl Panel for DockNavPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.title.clone()
    }
}

impl Render for DockNavPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        panel_body(self.name, &self.focus).child(self.nav.clone())
    }
}

/// A gallery-only [`Panel`] hosting one already-built view.
///
/// The thread list, the chat, and the artifact host differ only in what they
/// draw, so one adapter carries all three rather than three near-identical
/// ones.
pub(crate) struct DockViewPanel {
    body: AnyView,
    name: &'static str,
    title: SharedString,
    focus: FocusHandle,
}

impl DockViewPanel {
    /// Wraps `body` under a stable panel name and a tab title.
    pub(crate) fn new(
        body: impl Into<AnyView>,
        name: &'static str,
        title: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            body: body.into(),
            name,
            title: title.into(),
            focus: cx.focus_handle(),
        }
    }
}

impl EventEmitter<PanelEvent> for DockViewPanel {}

impl Focusable for DockViewPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BasePanel for DockViewPanel {
    fn panel_name(&self) -> &'static str {
        self.name
    }
}

impl Panel for DockViewPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.title.clone()
    }
}

impl Render for DockViewPanel {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        panel_body(self.name, &self.focus).child(self.body.clone())
    }
}

/// The box every adapter draws: the panel's own bounds, nothing else.
///
/// `min_w_0`/`min_h_0` are what keep a child inside those bounds when the dock
/// shrinks a region below the child's content size; without them a flex item
/// refuses to shrink past its content and paints over its neighbor.
fn panel_body(name: &'static str, focus: &FocusHandle) -> Stateful<Div> {
    div()
        .id(ElementId::from(SharedString::from(name)))
        .debug_selector(move || format!("dock-panel-{name}"))
        .track_focus(focus)
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_hidden()
}

/// Renders the stateless artifact panel so it can be docked like an entity.
struct ArtifactHost {
    artifact: Artifact,
}

impl Render for ArtifactHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .min_h_0()
            .child(ArtifactPanel::new("dock-artifact", &self.artifact).view(ArtifactView::Source))
    }
}

/// The dock-composition story: one `DockArea`, four gpui-ai components.
///
/// The layout is a nested tree rather than a row of panes so the story proves
/// the case that matters — a horizontal split whose second child is a vertical
/// split — and the sidebar's dock moves between the three edges upstream
/// offers, because `DockPlacement` has no `Top`.
pub(crate) struct DockCompositionStory {
    dock: Entity<DockArea>,
    /// The skin owns the installed renderer; dropping it would strip the
    /// area's chrome.
    _skin: Rc<DockSkin>,
    nav_panel: Entity<DockNavPanel>,
    threads_panel: Entity<DockViewPanel>,
    chat_panel: Entity<DockViewPanel>,
    artifact_panel: Entity<DockViewPanel>,
    placement: DockPlacement,
    _subscriptions: Vec<Subscription>,
}

impl DockCompositionStory {
    /// Every edge the sidebar can be docked to, in switcher order.
    ///
    /// Upstream has no `DockPlacement::Top`, so three is the whole set.
    pub(crate) const PLACEMENTS: [DockPlacement; 3] = [
        DockPlacement::Left,
        DockPlacement::Right,
        DockPlacement::Bottom,
    ];

    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let nav = cx.new(|cx| {
            // The one line that makes this composition possible: the dock owns
            // placement and size, so the navigation contributes neither.
            let mut nav = SidebarNav::new("dock-nav", window, cx)
                .with_presentation(SidebarNavPresentation::Embedded);
            nav.set_sections(crate::gallery::creamery_sidebar_sections(), cx);
            nav.set_active_item("all-orders", cx);
            nav
        });
        let threads = cx.new(|cx| {
            let mut list = ThreadList::new("dock-threads", window, cx);
            list.set_sections(crate::gallery::demo_thread_sections(), cx);
            list.set_active(Some("supplier-pricing"), cx);
            list
        });
        let prompt = cx.new(|cx| PromptBar::new("dock-prompt", window, cx));
        let chat = cx.new(|cx| {
            let mut chat = Chat::new("dock-chat", prompt, window, cx);
            chat.set_messages(dock_messages(), window, cx);
            chat
        });
        let artifact = cx.new(|_| ArtifactHost {
            artifact: dock_artifact(),
        });

        let nav_panel =
            cx.new(|cx| DockNavPanel::new(nav.clone(), "dock-navigation", "Navigation", cx));
        let threads_panel =
            cx.new(|cx| DockViewPanel::new(threads.clone(), "dock-threads", "Conversations", cx));
        let chat_panel = cx.new(|cx| DockViewPanel::new(chat, "dock-chat", "Chat", cx));
        let artifact_panel =
            cx.new(|cx| DockViewPanel::new(artifact, "dock-artifact", "Artifact", cx));

        let (dock, skin) = DockSkin::dock_area("dock-composition", None, window, cx);

        // Selection is emitted, never applied: the host owns the active item
        // here exactly as it does outside a dock.
        let subscriptions = vec![cx.subscribe(&nav, |_, nav, event: &SidebarNavEvent, cx| {
            if let SidebarNavEvent::Selected { item_id, .. } = event {
                nav.update(cx, |nav, cx| nav.set_active_item(item_id.clone(), cx));
            }
        })];

        let mut story = Self {
            dock,
            _skin: skin,
            nav_panel,
            threads_panel,
            chat_panel,
            artifact_panel,
            placement: DockPlacement::Left,
            _subscriptions: subscriptions,
        };
        story.apply_layout(window, cx);
        story
    }

    /// Re-dock the sidebar to another edge and rebuild the layout.
    pub(crate) fn set_placement(
        &mut self,
        placement: DockPlacement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.placement == placement {
            return;
        }
        self.placement = placement;
        self.apply_layout(window, cx);
        cx.notify();
    }

    /// Installs the center tree and the sidebar's dock.
    ///
    /// The center is rebuilt with the dock so the layout is described in one
    /// place: upstream reconciles the tree by value, so re-describing an
    /// unchanged center costs a comparison, not a rebuild of its panels.
    fn apply_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rem_size = window.rem_size();
        // Nested on purpose. A horizontal split holds the conversation list
        // beside a vertical split, so the answer and the artifact it produced
        // share one column: two axes, three panes, one region.
        let center = DockLayout::h_split()
            .child(
                DockLayout::tabs().panel_view(panel_handle(self.threads_panel.clone()), cx),
                Some(rem_size * THREAD_PANE_SIZE_IN_REMS),
            )
            .child(
                DockLayout::v_split()
                    .child(
                        DockLayout::tabs().panel_view(panel_handle(self.chat_panel.clone()), cx),
                        None,
                    )
                    .child(
                        DockLayout::tabs()
                            .panel_view(panel_handle(self.artifact_panel.clone()), cx),
                        Some(rem_size * ARTIFACT_PANE_SIZE_IN_REMS),
                    ),
                None,
            );
        let sidebar = DockLayout::tabs().panel_view(panel_handle(self.nav_panel.clone()), cx);
        let placement = self.placement;
        let size = if placement == DockPlacement::Bottom {
            rem_size * BOTTOM_DOCK_SIZE_IN_REMS
        } else {
            rem_size * SIDE_DOCK_SIZE_IN_REMS
        };

        self.dock.update(cx, |area, cx| {
            area.set_center(center, window, cx);
            for edge in Self::PLACEMENTS {
                if edge != placement && area.has_dock(edge) {
                    area.remove_dock(edge, window, cx);
                }
            }
            area.set_dock(placement, sidebar, window, cx);
            area.set_dock_size(placement, size, window, cx);
        });
    }
}

/// What the composition tests read. Nothing the story itself needs.
#[cfg(test)]
impl DockCompositionStory {
    /// The edge the sidebar is currently docked to.
    pub(crate) fn placement(&self) -> DockPlacement {
        self.placement
    }

    /// The dock area, for tests that read its regions.
    pub(crate) fn dock(&self) -> &Entity<DockArea> {
        &self.dock
    }

    /// The docked navigation.
    pub(crate) fn nav(&self, cx: &App) -> Entity<SidebarNav> {
        self.nav_panel.read(cx).nav().clone()
    }

    /// Marks every panel dirty so the next frame re-renders all four.
    ///
    /// Upstream draws a panel's view through `AnyView::cached`, so a panel
    /// whose entity is clean is replayed from the previous frame's element
    /// tree and registers no debug bounds for the current one. The navigation,
    /// the thread list, and the chat all notify on their own; the artifact
    /// host is fully static and never does, so a frame that is about to be
    /// measured has to ask for it. This buys nothing at runtime — a clean
    /// panel drawing from the cache is exactly right there — and exists only
    /// so a measurement reads the composition rather than the cache.
    pub(crate) fn invalidate_panels(&self, cx: &mut App) {
        self.nav_panel.update(cx, |_, cx| cx.notify());
        self.threads_panel.update(cx, |_, cx| cx.notify());
        self.chat_panel.update(cx, |_, cx| cx.notify());
        self.artifact_panel.update(cx, |_, cx| cx.notify());
    }

    /// Every panel's focus handle, paired with its stable panel name.
    ///
    /// A panel's handle is the identity that survives the dock rebuilding its
    /// tree, so it is what focus is asserted against.
    pub(crate) fn panel_focus_handles(&self, cx: &App) -> Vec<(&'static str, FocusHandle)> {
        vec![
            (
                self.nav_panel.read(cx).panel_name(),
                self.nav_panel.focus_handle(cx),
            ),
            (
                self.threads_panel.read(cx).panel_name(),
                self.threads_panel.focus_handle(cx),
            ),
            (
                self.chat_panel.read(cx).panel_name(),
                self.chat_panel.focus_handle(cx),
            ),
            (
                self.artifact_panel.read(cx).panel_name(),
                self.artifact_panel.focus_handle(cx),
            ),
        ]
    }
}

impl Render for DockCompositionStory {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let placement = self.placement;
        v_flex()
            .id("dock-composition-story")
            .debug_selector(|| "dock-composition-story".into())
            .gap(tokens.spacing.sm)
            .child(
                h_flex()
                    .id("dock-composition-placement")
                    .debug_selector(|| "dock-composition-placement".into())
                    .flex_none()
                    .gap(tokens.spacing.xs)
                    .role(Role::Toolbar)
                    .aria_label("Sidebar dock edge")
                    .children(Self::PLACEMENTS.map(|edge| {
                        let label = placement_label(edge);
                        Button::new(format!("dock-place-{label}"))
                            .selected(edge == placement)
                            .when(edge == placement, |button| button.primary())
                            .when(edge != placement, |button| button.outline())
                            .label(label)
                            .on_click(cx.listener(move |story, _, window, cx| {
                                story.set_placement(edge, window, cx);
                            }))
                    })),
            )
            .child(
                // The area draws into whatever box it is given and nothing
                // more, so the story supplies the bounded one.
                div()
                    .id("dock-composition-host")
                    .debug_selector(|| "dock-composition-host".into())
                    .w_full()
                    .h(rems(26.25))
                    .flex_none()
                    .min_w_0()
                    .overflow_hidden()
                    .border_1()
                    .border_color(cx.theme().border)
                    .rounded(tokens.radius.lg)
                    .child(self.dock.clone()),
            )
    }
}

/// The switcher label for one dock edge.
fn placement_label(placement: DockPlacement) -> &'static str {
    match placement {
        DockPlacement::Left => "Left",
        DockPlacement::Right => "Right",
        DockPlacement::Bottom => "Bottom",
        DockPlacement::Center => "Center",
    }
}

/// A short settled exchange, so the docked chat has content to reach.
fn dock_messages() -> std::sync::Arc<[ChatMessage]> {
    std::sync::Arc::from(
        [
            ChatMessage::new(
                "dock-question",
                ChatRole::User,
                StreamedContent::done("Which supplier is the safest choice this week?"),
            ),
            ChatMessage::new(
                "dock-answer",
                ChatRole::Assistant,
                StreamedContent::done(
                    "Northwind holds the only confirmed Friday slot, and its pistachio lot \
                     cleared inspection on Tuesday. The draft order is in the artifact below.",
                ),
            ),
        ]
        .as_slice(),
    )
}

/// The artifact the docked answer produced.
fn dock_artifact() -> Artifact {
    Artifact::new(
        "dock-order",
        "order-northwind.toml",
        StreamedContent::done(
            "[order]\nsupplier = \"northwind\"\nlot = \"pistachio-24\"\ndeliver_by = \
             \"friday\"\n\n[terms]\nnet_days = 30\n",
        ),
    )
    .kind(ArtifactKind::Code)
    .language("toml")
    // Versions give the header its switcher, which is the artifact content the
    // composition tests reach for inside the dock.
    .versions([
        ArtifactVersion::new("v1", "Draft"),
        ArtifactVersion::new("v2", "Confirmed"),
    ])
    .active_version("v2")
}

#[cfg(test)]
mod tests {
    use super::{DockCompositionStory, DockPlacement, SidebarNavPresentation};
    use gpui::{
        Bounds, Entity, Modifiers, Pixels, ScrollDelta, ScrollWheelEvent, TestAppContext,
        VisualTestContext, px, size,
    };
    use gpui_component::ActiveTheme as _;
    use gpui_component::theme::Theme;

    /// Every panel's box paired with one selector for content inside it.
    ///
    /// The pairs are what "reachable" means here: the panel drew, and the
    /// gpui-ai component inside it drew within the panel's bounds.
    const PANEL_CONTENT: [(&str, &str); 4] = [
        ("dock-panel-dock-navigation", "sidebar-nav-dock-nav"),
        ("dock-panel-dock-threads", "thread-supplier-pricing"),
        ("dock-panel-dock-chat", "chat-transcript"),
        ("dock-panel-dock-artifact", "artifact-versions-dock-order"),
    ];

    /// Settles pending work, then draws a frame every panel took part in.
    ///
    /// See [`DockCompositionStory::invalidate_panels`]: upstream caches panel
    /// views, so the measured frame has to be one none of them sat out.
    fn settle(cx: &mut VisualTestContext, story: &Entity<DockCompositionStory>) {
        for _ in 0..2 {
            cx.run_until_parked();
            // One update: a notify flushed on its own would be drawn — and
            // cleaned — before the measuring draw ever ran.
            cx.update(|window, cx| {
                story.update(cx, |story, cx| story.invalidate_panels(cx));
                window.draw(cx).clear(cx);
            });
        }
    }

    fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} should be reachable in the dock"))
    }

    /// Asserts `child` is inside `parent`, allowing for sub-pixel rounding.
    fn assert_within(child: Bounds<Pixels>, parent: Bounds<Pixels>, what: &str) {
        let slack = px(1.);
        assert!(
            child.origin.x >= parent.origin.x - slack
                && child.origin.y >= parent.origin.y - slack
                && child.origin.x + child.size.width <= parent.origin.x + parent.size.width + slack
                && child.origin.y + child.size.height
                    <= parent.origin.y + parent.size.height + slack,
            "{what} escaped its panel: child {child:?} parent {parent:?}",
        );
    }

    fn assert_every_panel_contains_its_content(cx: &mut VisualTestContext) {
        for (panel, content) in PANEL_CONTENT {
            let panel_bounds = bounds(cx, panel);
            assert!(
                panel_bounds.size.width > px(0.) && panel_bounds.size.height > px(0.),
                "{panel} should have a box",
            );
            assert_within(bounds(cx, content), panel_bounds, content);
        }
    }

    #[gpui::test]
    fn the_dock_story_mounts_every_panel_with_its_content_reachable(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (story, cx) = cx.add_window_view(DockCompositionStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(1100.), px(760.)));
        settle(cx, &story);

        assert_every_panel_contains_its_content(cx);
        assert_eq!(
            story.read_with(cx, |story, _| story.placement()),
            DockPlacement::Left,
        );
        story.read_with(cx, |story, cx| {
            let dock = story.dock().read(cx);
            assert!(dock.has_dock(DockPlacement::Left));
            assert!(dock.is_dock_open(DockPlacement::Left));
            // The center is the other region, so the composition really spans
            // two of upstream's regions and not one.
            assert!(!dock.is_empty(DockPlacement::Center, cx));
            // The seam is what makes the rest of this work, so the story is
            // pinned to it rather than to whatever the default happens to be.
            assert_eq!(
                story.nav(cx).read(cx).presentation(),
                SidebarNavPresentation::Embedded,
            );
        });
    }

    #[gpui::test]
    fn the_docked_navigation_fills_every_edge_upstream_offers(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (story, cx) = cx.add_window_view(DockCompositionStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(1100.), px(760.)));
        settle(cx, &story);

        let mut widths = Vec::new();
        for edge in DockCompositionStory::PLACEMENTS {
            cx.update(|window, cx| {
                story.update(cx, |story, cx| story.set_placement(edge, window, cx));
            });
            settle(cx, &story);

            let panel = bounds(cx, "dock-panel-dock-navigation");
            let nav = bounds(cx, "sidebar-nav-dock-nav");
            // The nav takes the panel's whole box at every edge; nothing about
            // the edge reached the component.
            assert_eq!(nav.size, panel.size, "the navigation should fill {edge:?}");
            assert_within(nav, panel, "the embedded navigation");
            widths.push((edge, nav.size.width));
        }

        let side = widths
            .iter()
            .find(|(edge, _)| *edge == DockPlacement::Left)
            .expect("left was exercised")
            .1;
        let bottom = widths
            .iter()
            .find(|(edge, _)| *edge == DockPlacement::Bottom)
            .expect("bottom was exercised")
            .1;
        // The finding in one assertion: docked along the bottom the navigation
        // spans the area instead of keeping a rail's width.
        assert!(
            bottom > side * 2.,
            "a bottom-docked navigation should fill the width: {bottom:?} vs a {side:?} rail",
        );
    }

    #[gpui::test]
    fn focus_moves_between_panels_without_leaking(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (story, cx) = cx.add_window_view(DockCompositionStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(1100.), px(760.)));
        settle(cx, &story);

        let handles = story.read_with(cx, |story, cx| story.panel_focus_handles(cx));
        assert_eq!(handles.len(), 4);

        for (name, handle) in &handles {
            cx.update(|window, cx| handle.focus(window, cx));
            settle(cx, &story);

            cx.update(|window, cx| {
                assert!(handle.is_focused(window), "{name} should take focus");
                let focused = window
                    .focused(cx)
                    .expect("focus should stay inside the dock");
                // Focus landed on a panel, not on nothing and not on a sibling
                // that happens to still be mounted.
                assert!(
                    handles.iter().any(|(_, other)| *other == focused),
                    "{name} handed focus outside every panel",
                );
                for (other_name, other) in &handles {
                    if other != handle {
                        assert!(
                            !other.is_focused(window),
                            "{other_name} kept focus while {name} took it",
                        );
                    }
                }
            });
        }
    }

    #[gpui::test]
    fn each_panel_scrolls_independently(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (story, cx) = cx.add_window_view(DockCompositionStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(1100.), px(760.)));
        settle(cx, &story);

        // Rows rather than panel centers: a panel's midpoint can land on its
        // header, and a wheel there would prove nothing about the list under
        // it.
        let nav_row = bounds(cx, "sidebar-nav-item-inventory");
        let thread_row = bounds(cx, "thread-supplier-pricing");

        cx.simulate_event(ScrollWheelEvent {
            position: nav_row.center(),
            delta: ScrollDelta::Pixels(gpui::point(px(0.), px(-40.))),
            ..Default::default()
        });
        settle(cx, &story);

        let nav_scrolled = bounds(cx, "sidebar-nav-item-inventory");
        assert!(
            nav_scrolled.origin.y < nav_row.origin.y,
            "the navigation should have taken the wheel: {nav_row:?} -> {nav_scrolled:?}",
        );
        assert_eq!(
            bounds(cx, "thread-supplier-pricing").origin,
            thread_row.origin,
            "the conversation list should not move when the navigation scrolls",
        );

        cx.simulate_event(ScrollWheelEvent {
            position: thread_row.center(),
            delta: ScrollDelta::Pixels(gpui::point(px(0.), px(-40.))),
            ..Default::default()
        });
        settle(cx, &story);

        assert!(
            bounds(cx, "thread-supplier-pricing").origin.y < thread_row.origin.y,
            "the conversation list should have taken its own wheel",
        );
        assert_eq!(
            bounds(cx, "sidebar-nav-item-inventory").origin,
            nav_scrolled.origin,
            "the navigation should not move when the conversation list scrolls",
        );
    }

    #[gpui::test]
    fn rem_zoom_keeps_every_panel_laid_out_and_reachable(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (story, cx) = cx.add_window_view(DockCompositionStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(1100.), px(760.)));
        settle(cx, &story);

        let base = cx.update(|_, cx| cx.theme().font_size);
        // 100%, 150%, 200%.
        for scale in [1., 1.5, 2.] {
            cx.update(|window, cx| {
                Theme::global_mut(cx).font_size = base * scale;
                window.set_rem_size(Theme::global(cx).font_size);
            });
            settle(cx, &story);

            assert_every_panel_contains_its_content(cx);
        }
    }

    #[gpui::test]
    fn resizing_relays_out_without_a_child_escaping_its_panel(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (story, cx) = cx.add_window_view(DockCompositionStory::new);
        let cx: &mut VisualTestContext = cx;

        for window_size in [
            size(px(1200.), px(800.)),
            size(px(760.), px(600.)),
            size(px(1000.), px(700.)),
        ] {
            cx.simulate_resize(window_size);
            settle(cx, &story);
            assert_every_panel_contains_its_content(cx);
        }

        // The bottom edge is the case a fixed-width rail would break, so it is
        // re-checked across a resize rather than only at one size.
        cx.update(|window, cx| {
            story.update(cx, |story, cx| {
                story.set_placement(DockPlacement::Bottom, window, cx)
            });
        });
        for window_size in [size(px(1200.), px(800.)), size(px(780.), px(620.))] {
            cx.simulate_resize(window_size);
            settle(cx, &story);
            assert_every_panel_contains_its_content(cx);

            let panel = bounds(cx, "dock-panel-dock-navigation");
            assert_eq!(bounds(cx, "sidebar-nav-dock-nav").size, panel.size);
        }
    }

    /// Documents an inherited constraint rather than endorsing it.
    ///
    /// A `ThreadList` row menu is a deferred popup opened from inside a
    /// virtualized list, and `gpui::list` paints under a content mask that
    /// deferred draws replay: a menu opened near a panel's bottom edge is laid
    /// out below the trigger, extends past the panel, and is clipped there
    /// instead of flipping above it. Not fixable in this composition — the
    /// mask belongs to the list and the placement to upstream's popover — so
    /// the story pins what actually happens. The trigger is also pointer-only,
    /// which is why this drives a click and not a keystroke.
    ///
    /// If this test starts failing because a menu now stays inside its panel,
    /// upstream fixed it and the note in the 0.2.1 ledger can be retired.
    #[gpui::test]
    fn row_menus_open_downward_and_clip_at_the_panel_edge(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (story, cx) = cx.add_window_view(DockCompositionStory::new);
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(1100.), px(760.)));
        settle(cx, &story);

        let panel = bounds(cx, "dock-panel-dock-threads");
        let panel_bottom = panel.origin.y + panel.size.height;

        // High in the panel: the menu has room and stays inside.
        let trigger = bounds(cx, "thread-more-supplier-pricing").center();
        cx.simulate_click(trigger, Modifiers::default());
        settle(cx, &story);
        let menu = bounds(cx, "thread-actions-menu");
        assert!(
            menu.origin.y >= panel.origin.y && menu.origin.y + menu.size.height <= panel_bottom,
            "a menu opened with room below it should fit vertically: menu {menu:?} against panel {panel:?}",
        );
        cx.simulate_keystrokes("escape");
        settle(cx, &story);

        // Near the bottom: the same menu is placed below the trigger anyway.
        let trigger = bounds(cx, "thread-more-margins").center();
        cx.simulate_click(trigger, Modifiers::default());
        settle(cx, &story);
        let menu = bounds(cx, "thread-actions-menu");
        assert!(
            menu.origin.y + menu.size.height > panel_bottom,
            "the row menu is expected to overhang the panel today: menu {menu:?} against a panel ending at {panel_bottom:?}",
        );
    }
}
