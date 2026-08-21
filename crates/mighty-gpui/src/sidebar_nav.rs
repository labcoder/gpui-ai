//! Stable-ID, filterable navigation composed over gpui-component's Sidebar primitives.

use std::{collections::HashSet, sync::Arc};

use gpui::{
    AnyElement, App, AppContext as _, Context, Div, ElementId, Entity, EventEmitter,
    Focusable as _, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity,
    Window, div, percentage, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Collapsible, Icon, IconName, h_flex,
    input::{Input, InputEvent, InputState},
    sidebar::{Sidebar, SidebarGroup, SidebarItem, SidebarMenuItem},
    tooltip::Tooltip,
    v_flex,
};

use crate::theme::SemanticStyledExt as _;

/// One application-owned recursive sidebar item.
///
/// IDs must be stable and globally unique inside a sidebar snapshot. Labels
/// may be duplicated because every interaction is routed by `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarNavItem {
    id: SharedString,
    label: SharedString,
    icon: Option<SharedString>,
    badge: Option<SharedString>,
    disabled: bool,
    children: Arc<[SidebarNavItem]>,
}

impl SidebarNavItem {
    /// Create an enabled item with stable application identity.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            badge: None,
            disabled: false,
            children: Arc::from([]),
        }
    }

    /// Set the optional leading icon.
    pub fn icon(mut self, icon: impl gpui_component::IconNamed) -> Self {
        self.icon = Some(icon.path());
        self
    }

    /// Set compact trailing badge text.
    pub fn badge(mut self, badge: impl Into<SharedString>) -> Self {
        self.badge = Some(badge.into());
        self
    }

    /// Set whether every activation path is unavailable.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Replace the recursive child snapshot.
    pub fn children<I>(mut self, children: I) -> Self
    where
        I: IntoIterator<Item = SidebarNavItem>,
    {
        self.children = children.into_iter().collect();
        self
    }

    /// Return the stable application identity.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Return the visible label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// Return the optional leading icon.
    pub fn icon_path(&self) -> Option<&SharedString> {
        self.icon.as_ref()
    }

    /// Return optional trailing badge text.
    pub fn badge_text(&self) -> Option<&SharedString> {
        self.badge.as_ref()
    }

    /// Return whether every activation path is unavailable.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    /// Return the recursive child snapshot.
    pub fn child_items(&self) -> &[SidebarNavItem] {
        &self.children
    }
}

/// One application-owned labeled section of sidebar items.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSection {
    id: SharedString,
    label: SharedString,
    items: Arc<[SidebarNavItem]>,
}

impl SidebarSection {
    /// Create an empty section with stable identity.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            items: Arc::from([]),
        }
    }

    /// Replace the section's recursive item snapshot.
    pub fn items<I>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = SidebarNavItem>,
    {
        self.items = items.into_iter().collect();
        self
    }

    /// Return the stable section identity.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Return the visible section label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// Return the recursive item snapshot.
    pub fn nav_items(&self) -> &[SidebarNavItem] {
        &self.items
    }
}

/// A typed application intent emitted by [`SidebarNav`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarNavEvent {
    /// An enabled item was activated by pointer, keyboard, or accessibility.
    ///
    /// Activating a parent intentionally toggles its transient expansion and
    /// emits `Selected` in the same interaction so applications may navigate
    /// to parent routes as well as expose their children.
    Selected {
        /// Sidebar component identity.
        id: SharedString,
        /// Stable application item identity.
        item_id: SharedString,
    },
    /// The sidebar's owned collapsed state changed.
    CollapsedChanged {
        /// Sidebar component identity.
        id: SharedString,
        /// Latest collapsed state.
        collapsed: bool,
    },
    /// The named new-task control was activated.
    NewTaskRequested {
        /// Sidebar component identity.
        id: SharedString,
    },
    /// The native quick-filter query changed.
    QueryChanged {
        /// Sidebar component identity.
        id: SharedString,
        /// Latest query text.
        query: SharedString,
    },
}

fn collect_item_ids(items: &[SidebarNavItem], ids: &mut HashSet<SharedString>) -> bool {
    items
        .iter()
        .all(|item| ids.insert(item.id.clone()) && collect_item_ids(&item.children, ids))
}

fn snapshot_ids_are_unique(sections: &[SidebarSection]) -> bool {
    let mut section_ids = HashSet::new();
    let mut item_ids = HashSet::new();
    sections.iter().all(|section| {
        section_ids.insert(section.id.clone()) && collect_item_ids(&section.items, &mut item_ids)
    })
}

fn collect_parent_ids(items: &[SidebarNavItem], ids: &mut HashSet<SharedString>) {
    for item in items {
        if !item.children.is_empty() {
            ids.insert(item.id.clone());
            collect_parent_ids(&item.children, ids);
        }
    }
}

fn item_matches(item: &SidebarNavItem, query: &str) -> bool {
    item.label.to_lowercase().contains(query)
        || item
            .badge
            .as_ref()
            .is_some_and(|badge| badge.to_lowercase().contains(query))
}

fn filter_item(item: &SidebarNavItem, query: &str) -> Option<SidebarNavItem> {
    if item_matches(item, query) {
        return Some(item.clone());
    }

    let children: Arc<[SidebarNavItem]> = item
        .children
        .iter()
        .filter_map(|child| filter_item(child, query))
        .collect();
    (!children.is_empty()).then(|| {
        let mut retained = item.clone();
        retained.children = children;
        retained
    })
}

fn filtered_sections(sections: &[SidebarSection], query: &str) -> Arc<[SidebarSection]> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Arc::from(sections);
    }

    sections
        .iter()
        .filter_map(|section| {
            if section.label.to_lowercase().contains(&query) {
                return Some(section.clone());
            }
            let items: Arc<[SidebarNavItem]> = section
                .items
                .iter()
                .filter_map(|item| filter_item(item, &query))
                .collect();
            (!items.is_empty()).then(|| {
                let mut retained = section.clone();
                retained.items = items;
                retained
            })
        })
        .collect()
}

fn nav_control(
    id: impl Into<ElementId>,
    label: SharedString,
    icon: IconName,
    show_label: bool,
    cx: &mut App,
) -> gpui_base::Button {
    let tokens = cx.theme().semantic_tokens();
    gpui_base::Button::new(id)
        .accessibility_label(label.clone())
        .flex()
        .items_center()
        .justify_center()
        .gap(tokens.spacing.xs)
        .px(tokens.spacing.sm)
        .py(tokens.spacing.xs)
        .rounded(tokens.radius.sm)
        .border_1()
        .border_color(cx.theme().sidebar_border)
        .bg(cx.theme().transparent)
        .text_color(cx.theme().sidebar_foreground)
        .hover(|style| style.bg(cx.theme().sidebar_accent))
        .active(|style| style.bg(cx.theme().button_active))
        .focus_visible(|style| style.border_color(cx.theme().ring))
        .child(Icon::new(icon).size_4())
        .when(show_label, |this| {
            this.flex_1()
                .justify_start()
                .child(div().text_token(tokens.typography.sm).child(label))
        })
}

fn sidebar_item_description(
    badge: Option<&SharedString>,
    disabled: bool,
    contains_active: bool,
) -> Option<SharedString> {
    let mut parts = Vec::new();
    if let Some(badge) = badge {
        parts.push(format!("Badge {badge}"));
    }
    if contains_active {
        parts.push("Contains selected item".to_owned());
    }
    if disabled {
        parts.push("Unavailable".to_owned());
    }
    (!parts.is_empty()).then(|| parts.join(". ").into())
}

#[allow(clippy::too_many_arguments)]
fn sidebar_item_control(
    id: impl Into<ElementId>,
    accessibility_id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    depth: usize,
    disabled: bool,
    active: bool,
    expanded: Option<bool>,
    description: Option<SharedString>,
    collapsed_icon: Option<SharedString>,
    collapsed: bool,
    cx: &mut App,
) -> gpui_base::Button {
    let tokens = cx.theme().semantic_tokens();
    let label = label.into();
    gpui_base::Button::new(id)
        .accessibility_id(accessibility_id)
        .accessibility_label(label.clone())
        .role(Role::TreeItem)
        .disabled(disabled)
        .selected(active)
        .aria_selected(active)
        .aria_level(depth + 1)
        .when_some(expanded, |this, expanded| this.aria_expanded(expanded))
        .when_some(description, |this, description| {
            this.aria_description(description)
        })
        .absolute()
        .inset_0()
        .size_full()
        .rounded(tokens.radius.sm)
        .border_1()
        .border_color(cx.theme().transparent)
        .bg(cx.theme().transparent)
        .block_mouse_except_scroll()
        .focus_visible(|style| style.border_color(cx.theme().ring))
        .when(collapsed, |this| {
            let tooltip_label = label.clone();
            this.when_some(collapsed_icon, |this, icon| {
                this.child(Icon::default().path(icon).size_4())
            })
            .tooltip(move |window, cx| Tooltip::new(tooltip_label.clone()).build(window, cx))
        })
        .styles(|styles| {
            styles
                .selected(|style| style.border_color(cx.theme().sidebar_border))
                .disabled(|style| style.text_color(cx.theme().muted_foreground))
        })
}

fn sidebar_tree_container(
    component_id: SharedString,
    section_id: SharedString,
    section_label: SharedString,
) -> Stateful<Div> {
    v_flex()
        .id((
            ElementId::from((ElementId::from(component_id.clone()), section_id.clone())),
            "tree",
        ))
        .accessibility_id(format!(
            "sidebar-nav.{component_id}.section.{section_id}.tree"
        ))
        .role(Role::Tree)
        .aria_label(format!("{section_label} navigation items"))
}

fn sidebar_section_container(
    component_id: SharedString,
    section_id: SharedString,
    section_label: SharedString,
    child: impl IntoElement,
) -> Stateful<Div> {
    div()
        .id((
            ElementId::from((ElementId::from(component_id.clone()), section_id.clone())),
            "group",
        ))
        .accessibility_id(format!("sidebar-nav.{component_id}.section.{section_id}"))
        .role(Role::Group)
        .aria_label(section_label)
        .child(child)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Element as _, RenderOnce as _, TestAppContext, accesskit, canvas};
    use std::sync::{Arc, Mutex};

    type CapturedSemanticNodes = Arc<Mutex<Vec<(Option<Role>, accesskit::Node)>>>;

    struct SemanticsProbe {
        captured: CapturedSemanticNodes,
    }

    impl Render for SemanticsProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let active = sidebar_item_control(
                        "active-item",
                        "sidebar-nav.test.item.active",
                        "Reports",
                        2,
                        false,
                        true,
                        Some(true),
                        sidebar_item_description(Some(&"12".into()), false, true),
                        None,
                        false,
                        cx,
                    )
                    .on_click(|_, _, _| {})
                    .render(window, cx)
                    .into_element();
                    let disabled = sidebar_item_control(
                        "disabled-item",
                        "sidebar-nav.test.item.disabled",
                        "Exports",
                        1,
                        true,
                        false,
                        None,
                        sidebar_item_description(None, true, false),
                        None,
                        false,
                        cx,
                    )
                    .on_click(|_, _, _| {})
                    .render(window, cx)
                    .into_element();
                    let workspace_group = sidebar_section_container(
                        "test".into(),
                        "workspace".into(),
                        "Workspace".into(),
                        div(),
                    )
                    .into_element();
                    let workspace_tree = sidebar_tree_container(
                        "test".into(),
                        "workspace".into(),
                        "Workspace".into(),
                    )
                    .into_element();
                    let reports_tree =
                        sidebar_tree_container("test".into(), "reports".into(), "Reports".into())
                            .into_element();
                    let mut active_node = accesskit::Node::new(Role::Unknown);
                    active.write_a11y_info(&mut active_node);
                    let mut disabled_node = accesskit::Node::new(Role::Unknown);
                    disabled.write_a11y_info(&mut disabled_node);
                    let mut workspace_group_node = accesskit::Node::new(Role::Unknown);
                    workspace_group.write_a11y_info(&mut workspace_group_node);
                    let mut workspace_tree_node = accesskit::Node::new(Role::Unknown);
                    workspace_tree.write_a11y_info(&mut workspace_tree_node);
                    let mut reports_tree_node = accesskit::Node::new(Role::Unknown);
                    reports_tree.write_a11y_info(&mut reports_tree_node);
                    *captured.lock().expect("capture mutex should be available") = vec![
                        (active.a11y_role(), active_node),
                        (disabled.a11y_role(), disabled_node),
                        (workspace_group.a11y_role(), workspace_group_node),
                        (workspace_tree.a11y_role(), workspace_tree_node),
                        (reports_tree.a11y_role(), reports_tree_node),
                    ];
                },
                |_, _, _, _| {},
            )
        }
    }

    #[test]
    fn recursive_filter_retains_only_the_matching_branch_and_its_ancestors() {
        let sections = [SidebarSection::new("workspace", "Workspace").items([
            SidebarNavItem::new("orders", "Orders").children([
                SidebarNavItem::new("history", "History"),
                SidebarNavItem::new("suppliers", "Suppliers")
                    .children([SidebarNavItem::new("risk", "Risk reports")]),
            ]),
        ])];

        let filtered = filtered_sections(&sections, "RISK");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].items[0].id, "orders");
        assert_eq!(filtered[0].items[0].children.len(), 1);
        assert_eq!(filtered[0].items[0].children[0].id, "suppliers");
        assert_eq!(filtered[0].items[0].children[0].children[0].id, "risk");
    }

    #[test]
    fn duplicate_labels_are_valid_but_duplicate_ids_are_rejected() {
        let duplicate_labels = [SidebarSection::new("reports", "Reports").items([
            SidebarNavItem::new("live", "Reports"),
            SidebarNavItem::new("archive", "Reports"),
        ])];
        assert!(snapshot_ids_are_unique(&duplicate_labels));

        let duplicate_ids = [SidebarSection::new("reports", "Reports").items([
            SidebarNavItem::new("same", "Live"),
            SidebarNavItem::new("same", "Archive"),
        ])];
        assert!(!snapshot_ids_are_unique(&duplicate_ids));
    }

    #[gpui::test]
    fn item_controls_expose_active_expanded_disabled_and_activation_semantics(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(Vec::new()));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| SemanticsProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let nodes = result
            .lock()
            .expect("capture mutex should be available")
            .drain(..)
            .collect::<Vec<_>>();

        let (active_role, active) = &nodes[0];
        assert_eq!(*active_role, Some(Role::TreeItem));
        assert_eq!(active.label(), Some("Reports"));
        assert_eq!(
            active.description(),
            Some("Badge 12. Contains selected item")
        );
        assert_eq!(active.level(), Some(3));
        assert_eq!(active.is_selected(), Some(true));
        assert_eq!(active.is_expanded(), Some(true));
        assert!(active.supports_action(accesskit::Action::Click));

        let (disabled_role, disabled) = &nodes[1];
        assert_eq!(*disabled_role, Some(Role::TreeItem));
        assert_eq!(disabled.label(), Some("Exports"));
        assert_eq!(disabled.description(), Some("Unavailable"));
        assert!(!disabled.supports_action(accesskit::Action::Click));

        let (group_role, group) = &nodes[2];
        assert_eq!(*group_role, Some(Role::Group));
        assert_eq!(group.label(), Some("Workspace"));

        let (workspace_tree_role, workspace_tree) = &nodes[3];
        assert_eq!(*workspace_tree_role, Some(Role::Tree));
        assert_eq!(workspace_tree.label(), Some("Workspace navigation items"));
        let (reports_tree_role, reports_tree) = &nodes[4];
        assert_eq!(*reports_tree_role, Some(Role::Tree));
        assert_eq!(reports_tree.label(), Some("Reports navigation items"));
    }
}

#[derive(Clone)]
struct StableMenuTree {
    component_id: SharedString,
    section_id: SharedString,
    section_label: SharedString,
    items: Arc<[SidebarNavItem]>,
    active_item: Option<SharedString>,
    expanded: Arc<HashSet<SharedString>>,
    owner: WeakEntity<SidebarNav>,
    collapsed: bool,
    filtering: bool,
}

impl Collapsible for StableMenuTree {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl StableMenuTree {
    fn render_items(
        &self,
        items: &[SidebarNavItem],
        depth: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Vec<AnyElement> {
        items
            .iter()
            .map(|item| self.render_item(item, depth, window, cx))
            .collect()
    }

    fn render_item(
        &self,
        item: &SidebarNavItem,
        depth: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let tokens = cx.theme().semantic_tokens();
        let exact_active = self.active_item.as_ref() == Some(&item.id);
        let contains_active = self.active_item.as_ref().is_some_and(|active_id| {
            item.children
                .iter()
                .any(|child| find_item(child, active_id).is_some())
        });
        // A compact sidebar cannot expose descendants, so its visible ancestor
        // carries selected state. Expanded navigation instead keeps the exact
        // controlled route reachable through transiently collapsed parents.
        let active = exact_active || (self.collapsed && contains_active);
        let has_children = !item.children.is_empty();
        let expanded =
            has_children && (self.filtering || self.expanded.contains(&item.id) || contains_active);
        let item_id = item.id.clone();
        let debug_id = item.id.clone();
        let active_debug_id = item.id.clone();
        let keyboard_owner = self.owner.clone();
        let keyboard_item_id = item.id.clone();
        let component_id = self.component_id.clone();
        let badge = item.badge.clone();
        let collapsed_icon = item.icon.clone();

        let menu_item = SidebarMenuItem::new(item.label.clone())
            .active(active)
            .collapsed(self.collapsed)
            .disable(item.disabled)
            .when(!self.collapsed, |this| {
                this.when_some(item.icon.clone(), |this, icon| {
                    this.icon(Icon::default().path(icon))
                })
            })
            .when(
                !self.collapsed && (badge.is_some() || active || has_children),
                |this| {
                    this.suffix(move |_, cx| {
                        h_flex()
                            .gap(cx.theme().semantic_tokens().spacing.xs)
                            .when_some(badge.clone(), |this, badge| {
                                this.child(
                                    div()
                                        .px(cx.theme().semantic_tokens().spacing.xs)
                                        .py(cx.theme().semantic_tokens().spacing.xxs)
                                        .rounded(cx.theme().semantic_tokens().radius.full)
                                        .border_1()
                                        .border_color(cx.theme().sidebar_border)
                                        .text_token(cx.theme().semantic_tokens().typography.xs)
                                        .child(badge),
                                )
                            })
                            .when(active, |this| {
                                this.child(Icon::new(IconName::Check).size_3())
                            })
                            .when(has_children, |this| {
                                this.child(
                                    Icon::new(IconName::ChevronRight)
                                        .size_3()
                                        .when(expanded, |this| this.rotate(percentage(90. / 360.))),
                                )
                            })
                    })
                },
            );

        let control = sidebar_item_control(
            (
                ElementId::from((ElementId::from(component_id.clone()), item_id.clone())),
                "control",
            ),
            format!("sidebar-nav.{component_id}.item.{}", item.id),
            item.label.clone(),
            depth,
            item.disabled,
            active,
            has_children.then_some(expanded),
            sidebar_item_description(
                item.badge.as_ref(),
                item.disabled,
                self.collapsed && contains_active,
            ),
            collapsed_icon,
            self.collapsed,
            cx,
        )
        .debug_selector(move || format!("sidebar-nav-item-{debug_id}"))
        .on_click(move |_, _, cx| {
            _ = keyboard_owner.update(cx, |nav, cx| {
                nav.activate_item(keyboard_item_id.clone(), cx)
            });
        });

        let row = div()
            .id((
                ElementId::from((ElementId::from(self.component_id.clone()), item.id.clone())),
                "row",
            ))
            .relative()
            .w_full()
            .min_w_0()
            .child(menu_item.render(
                format!("sidebar-nav-menu.{}.{}", self.component_id, item.id),
                // SidebarMenuItem owns the pinned presentation. The transparent
                // stable control blocks non-scroll pointer fallthrough and owns
                // tooltip, keyboard, and AccessKit activation as one handler.
                window,
                cx,
            ))
            .child(control);

        let children = if expanded {
            self.render_items(&item.children, depth + 1, window, cx)
        } else {
            Vec::new()
        };

        v_flex()
            .id((ElementId::from(self.component_id.clone()), item.id.clone()))
            .when(active, |this| {
                this.debug_selector(move || format!("sidebar-nav-active-{active_debug_id}"))
            })
            .w_full()
            .min_w_0()
            .child(row)
            .when(!children.is_empty() && !self.collapsed, |this| {
                this.child(
                    v_flex()
                        .ml(tokens.spacing.md)
                        .pl(tokens.spacing.sm)
                        .border_l_1()
                        .border_color(cx.theme().sidebar_border)
                        .gap(tokens.spacing.xxs)
                        .children(children),
                )
            })
            .into_any_element()
    }
}

impl SidebarItem for StableMenuTree {
    fn render(
        self,
        _id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        sidebar_tree_container(
            self.component_id.clone(),
            self.section_id.clone(),
            self.section_label.clone(),
        )
        .gap(cx.theme().semantic_tokens().spacing.xxs)
        .children(self.render_items(&self.items, 0, window, cx))
    }
}

#[derive(Clone)]
struct StableSection {
    section: SidebarSection,
    tree: StableMenuTree,
    collapsed: bool,
}

impl Collapsible for StableSection {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self.tree = self.tree.collapsed(collapsed);
        self
    }
}

impl SidebarItem for StableSection {
    fn render(
        self,
        _id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let section_id = self.section.id.clone();
        let component_id = self.tree.component_id.clone();
        let group = SidebarGroup::new(self.section.label.clone())
            .child(self.tree)
            .collapsed(self.collapsed)
            .render(
                (ElementId::from("sidebar-nav-section"), section_id),
                window,
                cx,
            );
        sidebar_section_container(
            component_id,
            self.section.id.clone(),
            self.section.label.clone(),
            group,
        )
    }
}

/// A hybrid-controlled, filterable application sidebar.
///
/// Applications own the immutable section/item snapshot and active item ID.
/// This entity retains one native [`InputState`] plus collapsed, query,
/// expansion, focus, and scroll interaction state. Selection is emitted as a
/// typed stable ID and never changes the consumer-controlled active item.
///
/// ```ignore
/// let nav = cx.new(|cx| SidebarNav::new("workspace-nav", window, cx));
/// nav.update(cx, |nav, cx| {
///     nav.set_sections([
///         SidebarSection::new("main", "Workspace").items([
///             SidebarNavItem::new("orders", "Orders")
///                 .icon(gpui_component::IconName::LayoutDashboard),
///         ]),
///     ], cx);
///     nav.set_active_item("orders", cx);
/// });
/// ```
pub struct SidebarNav {
    id: SharedString,
    sections: Arc<[SidebarSection]>,
    active_item: Option<SharedString>,
    collapsed: bool,
    query: SharedString,
    input: Entity<InputState>,
    expanded: HashSet<SharedString>,
    _input_subscription: Subscription,
}

impl SidebarNav {
    /// Create an empty navigation entity with one retained native filter input.
    pub fn new(id: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter navigation"));
        let subscription =
            cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    let query: SharedString = input.read(cx).value().to_string().into();
                    this.update_query(query, cx);
                }
            });
        Self {
            id: id.into(),
            sections: Arc::from([]),
            active_item: None,
            collapsed: false,
            query: "".into(),
            input,
            expanded: HashSet::new(),
            _input_subscription: subscription,
        }
    }

    /// Replace the controlled section snapshot.
    ///
    /// Malformed snapshots containing duplicate section or item IDs are
    /// ignored so focus, events, and accessibility identities cannot alias.
    pub fn set_sections(
        &mut self,
        sections: impl IntoIterator<Item = SidebarSection>,
        cx: &mut Context<Self>,
    ) {
        let sections: Arc<[SidebarSection]> = sections.into_iter().collect();
        if self.sections.as_ref() == sections.as_ref() || !snapshot_ids_are_unique(&sections) {
            return;
        }

        let mut old_parents = HashSet::new();
        let mut new_parents = HashSet::new();
        for section in self.sections.iter() {
            collect_parent_ids(&section.items, &mut old_parents);
        }
        for section in sections.iter() {
            collect_parent_ids(&section.items, &mut new_parents);
        }
        self.expanded.retain(|id| new_parents.contains(id));
        self.expanded
            .extend(new_parents.difference(&old_parents).cloned());

        self.sections = sections;
        cx.notify();
    }

    /// Replace the consumer-controlled active item ID.
    pub fn set_active_item(&mut self, item_id: impl Into<SharedString>, cx: &mut Context<Self>) {
        let item_id = item_id.into();
        if self.active_item.as_ref() != Some(&item_id) {
            self.active_item = Some(item_id);
            cx.notify();
        }
    }

    /// Replace the owned collapsed state and emit its stable component identity.
    pub fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.collapsed == collapsed {
            return;
        }
        self.collapsed = collapsed;
        cx.emit(SidebarNavEvent::CollapsedChanged {
            id: self.id.clone(),
            collapsed,
        });
        cx.notify();
    }

    /// Replace the native query and emit a change only when its text differs.
    pub fn set_query(
        &mut self,
        query: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = query.into();
        if self.query == query {
            return;
        }
        self.query = query.clone();
        self.input.update(cx, |input, cx| {
            input.set_value(query.to_string(), window, cx)
        });
        self.emit_query_changed(query, cx);
        // InputState intentionally suppresses Change for programmatic values,
        // and it may be unmounted while collapsed, so the owner must invalidate
        // its filtered snapshot directly.
        cx.notify();
    }

    /// Return whether the sidebar is currently collapsed.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    /// Return the latest quick-filter query.
    pub fn query(&self) -> &SharedString {
        &self.query
    }

    /// Move focus into the retained native filter input when it is mounted.
    ///
    /// Collapsed navigation keeps focus on its visible compact control instead
    /// of moving it into an unmounted input.
    pub fn focus_filter(&self, window: &mut Window, cx: &mut App) {
        if self.collapsed {
            return;
        }
        self.input.read(cx).focus_handle(cx).focus(window, cx);
    }

    fn update_query(&mut self, query: SharedString, cx: &mut Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query.clone();
        self.emit_query_changed(query, cx);
        cx.notify();
    }

    fn emit_query_changed(&self, query: SharedString, cx: &mut Context<Self>) {
        cx.emit(SidebarNavEvent::QueryChanged {
            id: self.id.clone(),
            query,
        });
    }

    fn activate_item(&mut self, item_id: SharedString, cx: &mut Context<Self>) {
        let Some(item) = self
            .sections
            .iter()
            .flat_map(|section| section.items.iter())
            .find_map(|item| find_item(item, &item_id))
        else {
            return;
        };
        if item.disabled {
            return;
        }
        let has_children = !item.children.is_empty();
        if has_children {
            if !self.expanded.remove(&item_id) {
                self.expanded.insert(item_id.clone());
            }
            cx.notify();
        }
        cx.emit(SidebarNavEvent::Selected {
            id: self.id.clone(),
            item_id,
        });
    }
}

fn find_item<'a>(item: &'a SidebarNavItem, id: &SharedString) -> Option<&'a SidebarNavItem> {
    (item.id == *id)
        .then_some(item)
        .or_else(|| item.children.iter().find_map(|child| find_item(child, id)))
}

impl EventEmitter<SidebarNavEvent> for SidebarNav {}

impl Render for SidebarNav {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let sections = filtered_sections(&self.sections, &self.query);
        let owner = cx.weak_entity();
        let expanded = Arc::new(self.expanded.clone());
        let filtering = !self.query.trim().is_empty();
        let stable_sections = sections
            .iter()
            .cloned()
            .map(|section| StableSection {
                tree: StableMenuTree {
                    component_id: self.id.clone(),
                    section_id: section.id.clone(),
                    section_label: section.label.clone(),
                    items: section.items.clone(),
                    active_item: self.active_item.clone(),
                    expanded: expanded.clone(),
                    owner: owner.clone(),
                    collapsed: self.collapsed,
                    filtering,
                },
                section,
                collapsed: self.collapsed,
            })
            .collect::<Vec<_>>();
        let new_task_owner = cx.weak_entity();
        let collapse_owner = cx.weak_entity();
        let collapsed = self.collapsed;
        let input = self.input.clone();
        let id = self.id.clone();
        let empty_message: SharedString = if self.sections.is_empty() {
            "No navigation items".into()
        } else {
            "No matching navigation items".into()
        };
        let empty_selector = self.sections.is_empty();

        let header_actions = if collapsed {
            h_flex().w_full().child(
                nav_control(
                    (ElementId::from(id.clone()), "collapse"),
                    "Expand navigation".into(),
                    IconName::PanelLeftOpen,
                    false,
                    cx,
                )
                .w_full()
                .debug_selector(|| "sidebar-nav-collapse".to_owned())
                .on_click(move |_, _, cx| {
                    _ = collapse_owner.update(cx, |nav, cx| nav.set_collapsed(false, cx));
                }),
            )
        } else {
            h_flex()
                .w_full()
                .gap(tokens.spacing.xs)
                .child(
                    nav_control(
                        (ElementId::from(id.clone()), "new-task"),
                        "New task".into(),
                        IconName::Plus,
                        true,
                        cx,
                    )
                    .debug_selector(|| "sidebar-nav-new-task".to_owned())
                    .on_click(move |_, _, cx| {
                        _ = new_task_owner.update(cx, |nav, cx| {
                            cx.emit(SidebarNavEvent::NewTaskRequested { id: nav.id.clone() });
                        });
                    }),
                )
                .child(
                    nav_control(
                        (ElementId::from(id.clone()), "collapse"),
                        "Collapse navigation".into(),
                        IconName::PanelLeftClose,
                        false,
                        cx,
                    )
                    .debug_selector(|| "sidebar-nav-collapse".to_owned())
                    .on_click(move |_, _, cx| {
                        _ = collapse_owner.update(cx, |nav, cx| nav.set_collapsed(true, cx));
                    }),
                )
        };

        let header = v_flex()
            .w_full()
            .gap(tokens.spacing.sm)
            .child(header_actions)
            .when(!collapsed, |this| {
                this.child(
                    div()
                        .id((ElementId::from(id.clone()), "filter"))
                        .debug_selector(|| "sidebar-nav-filter".to_owned())
                        .w_full()
                        .child(
                            Input::new(&input)
                                .accessibility_id(format!("sidebar-nav.{id}.filter"))
                                .aria_label("Filter navigation")
                                .prefix(IconName::Search)
                                .cleanable(true)
                                .w_full(),
                        ),
                )
            });

        div()
            .id((ElementId::from(self.id.clone()), "frame"))
            .debug_selector({
                let id = self.id.clone();
                move || format!("sidebar-nav-{id}")
            })
            .accessibility_id(format!("sidebar-nav.{}", self.id))
            .role(Role::Navigation)
            .aria_label("Workspace navigation")
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .child(
                Sidebar::new((ElementId::from(self.id.clone()), "sidebar"))
                    .collapsed(collapsed)
                    .w(tokens.spacing.xxl * 8.)
                    .header(header)
                    .children(stable_sections)
                    .when(sections.is_empty(), |this| {
                        this.child(StableSection {
                            section: SidebarSection::new("empty", ""),
                            tree: StableMenuTree {
                                component_id: self.id.clone(),
                                section_id: "empty".into(),
                                section_label: "Navigation status".into(),
                                items: Arc::from([]),
                                active_item: None,
                                expanded: Arc::new(HashSet::new()),
                                owner: cx.weak_entity(),
                                collapsed,
                                filtering,
                            },
                            collapsed,
                        })
                    })
                    .footer(
                        div()
                            .id((ElementId::from(self.id.clone()), "empty-status"))
                            .when(sections.is_empty() && !collapsed, |this| {
                                this.debug_selector(move || {
                                    if empty_selector {
                                        "sidebar-nav-empty".to_owned()
                                    } else {
                                        "sidebar-nav-no-results".to_owned()
                                    }
                                })
                                .role(Role::Status)
                                .aria_label(empty_message.clone())
                                .text_token(tokens.typography.sm)
                                .text_color(cx.theme().muted_foreground)
                                .child(empty_message)
                            }),
                    ),
            )
    }
}
