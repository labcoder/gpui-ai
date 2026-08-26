//! Stable-ID, filterable navigation composed over gpui-component's Sidebar primitives.
//!
//! The entity's only rendering input is one flattened snapshot of visible
//! rows. Sections, items, filtering, and expansion resolve into a single
//! `[VisibleRow]` recomputed when the state it derives from changes and never
//! from `Render`, so virtualization spans rows rather than section roots: a
//! section holding ten thousand expanded descendants constructs the same
//! bounded window as a section holding ten.
//!
//! Flattening costs the tree relationships that element nesting used to
//! carry, so every row states them: level, parent, and position within its
//! visible sibling set. Those fields are what the ARIA tree keyboard model
//! walks and what the AccessKit nodes report, which is the sanctioned way to
//! describe a tree whose rows cannot contain one another.

use std::{cell::Cell, collections::HashSet, rc::Rc, sync::Arc};

use gpui::{
    AnyElement, App, AppContext as _, Context, Div, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable as _, InteractiveElement as _, IntoElement, ListAlignment, ListOffset, ListState,
    ParentElement as _, Pixels, Render, Role, SharedString, Stateful,
    StatefulInteractiveElement as _, Styled as _, Subscription, WeakEntity, Window, div, list,
    percentage, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    sidebar::{SidebarItem as _, SidebarMenuItem},
    tooltip::Tooltip,
    v_flex,
};

use crate::{
    resolved_layout::ResolvedLayoutKey, scrolling::list_scroll_mask, theme::SemanticStyledExt as _,
};

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

/// The sections and items one query retains.
///
/// Retention is recorded as identity rather than as a rebuilt tree: the
/// snapshot walk reads the application's own sections and skips what the
/// projection excludes, so filtering never clones a branch.
#[derive(Debug, Default)]
struct FilterProjection {
    sections: HashSet<SharedString>,
    items: HashSet<SharedString>,
}

/// Records `item` and its retained descendants, reporting whether it survives.
///
/// A matching item keeps its whole subtree â€” someone who searched for a branch
/// expects to see what is inside it â€” which `inherited` carries downward. Every
/// child is visited regardless of the running result, because retention is
/// being recorded, not merely answered.
fn retain_item(
    item: &SidebarNavItem,
    query: &str,
    inherited: bool,
    retained: &mut HashSet<SharedString>,
) -> bool {
    let matched = inherited || item_matches(item, query);
    let mut keep = matched;
    for child in item.children.iter() {
        keep |= retain_item(child, query, matched, retained);
    }
    if keep {
        retained.insert(item.id.clone());
    }
    keep
}

fn filter_projection(sections: &[SidebarSection], query: &str) -> FilterProjection {
    let mut projection = FilterProjection::default();
    for section in sections {
        let matched = section.label.to_lowercase().contains(query);
        let mut keep = matched;
        for item in section.items.iter() {
            keep |= retain_item(item, query, matched, &mut projection.items);
        }
        if keep {
            projection.sections.insert(section.id.clone());
        }
    }
    projection
}

/// Records the ancestors of `target`, reporting whether the walk found it.
fn collect_ancestors(
    items: &[SidebarNavItem],
    target: &SharedString,
    ancestors: &mut HashSet<SharedString>,
) -> bool {
    for item in items {
        if item.id == *target {
            return true;
        }
        if collect_ancestors(&item.children, target, ancestors) {
            ancestors.insert(item.id.clone());
            return true;
        }
    }
    false
}

fn find_item<'a>(item: &'a SidebarNavItem, id: &SharedString) -> Option<&'a SidebarNavItem> {
    (item.id == *id)
        .then_some(item)
        .or_else(|| item.children.iter().find_map(|child| find_item(child, id)))
}

/// One row of the flattened visible-row snapshot.
///
/// A row states its own place in the tree because the virtual list renders
/// rows as siblings: `level`, `parent`, `position`, and `set_size` are the
/// relationships that element nesting would otherwise carry, and both the
/// keyboard model and the AccessKit nodes read them from here.
#[derive(Clone, Debug, PartialEq, Eq)]
struct VisibleRow {
    /// Stable identity: the section ID for a header, the item ID otherwise.
    id: SharedString,
    /// Visible label.
    label: SharedString,
    /// Leading icon path, when the item declares one.
    icon: Option<SharedString>,
    /// Compact trailing badge text, when the item declares one.
    badge: Option<SharedString>,
    /// The row this one hangs from: a root item's parent is its section.
    parent: Option<SharedString>,
    /// One-based accessibility level; a section header is level 1.
    level: usize,
    /// Indentation steps below the section's root items.
    indent: usize,
    /// One-based position among the visible siblings under `parent`.
    position: usize,
    /// Number of visible siblings under `parent`.
    set_size: usize,
    /// Whether the row labels a section instead of an item.
    header: bool,
    /// Whether every activation path is unavailable.
    disabled: bool,
    /// Whether the row owns children that the projection retained.
    has_children: bool,
    /// Whether this row's children follow it in the snapshot.
    expanded: bool,
    /// Whether the row carries the active treatment.
    active: bool,
    /// Whether the controlled active item is a descendant.
    contains_active: bool,
}

/// The resolved inputs a snapshot walk reads for every row.
struct RowContext<'a> {
    projection: Option<&'a FilterProjection>,
    expanded: &'a HashSet<SharedString>,
    active: Option<&'a SharedString>,
    ancestors: &'a HashSet<SharedString>,
    filtering: bool,
    collapsed: bool,
}

/// The children of one item that survive the query, in application order.
fn visible_children<'a>(
    items: &'a [SidebarNavItem],
    projection: Option<&'a FilterProjection>,
) -> impl Iterator<Item = &'a SidebarNavItem> {
    items
        .iter()
        .filter(move |item| projection.is_none_or(|projection| projection.items.contains(&item.id)))
}

/// Flattens the controlled snapshot into the rows a frame may render.
///
/// This is the component's one projection: it applies the query, the owned
/// expansion set, the controlled active item, and the collapsed rail in a
/// single pass, so no later stage has to re-derive any of them.
fn visible_rows(
    sections: &[SidebarSection],
    query: &str,
    expanded: &HashSet<SharedString>,
    active: Option<&SharedString>,
    collapsed: bool,
) -> Arc<[VisibleRow]> {
    let query = query.trim().to_lowercase();
    let filtering = !query.is_empty();
    let projection = filtering.then(|| filter_projection(sections, &query));
    let projection = projection.as_ref();

    // An active item the query excluded cannot be contained by anything on
    // screen, so its ancestors keep their own presentation.
    let active =
        active.filter(|id| projection.is_none_or(|projection| projection.items.contains(*id)));
    let mut ancestors = HashSet::new();
    if let Some(active) = active {
        for section in sections {
            if collect_ancestors(&section.items, active, &mut ancestors) {
                break;
            }
        }
    }

    let context = RowContext {
        projection,
        expanded,
        active,
        ancestors: &ancestors,
        filtering,
        collapsed,
    };

    let visible: Vec<&SidebarSection> = sections
        .iter()
        .filter(|section| {
            projection.is_none_or(|projection| projection.sections.contains(&section.id))
        })
        .collect();
    let section_count = visible.len();
    // The compact rail shows one column of root items, so it has no section
    // headers and its roots are the tree's top level.
    let root_level = if collapsed { 1 } else { 2 };

    let mut rows = Vec::new();
    for (index, section) in visible.into_iter().enumerate() {
        if !collapsed {
            let children = visible_children(&section.items, context.projection)
                .next()
                .is_some();
            rows.push(VisibleRow {
                id: section.id.clone(),
                label: section.label.clone(),
                icon: None,
                badge: None,
                parent: None,
                level: 1,
                indent: 0,
                position: index + 1,
                set_size: section_count,
                header: true,
                disabled: false,
                has_children: children,
                expanded: children,
                active: false,
                contains_active: false,
            });
        }
        push_rows(
            &section.items,
            &section.id,
            root_level,
            0,
            &context,
            &mut rows,
        );
    }
    Arc::from(rows)
}

/// Appends one visible sibling set, and the subtrees the reader can see.
fn push_rows(
    items: &[SidebarNavItem],
    parent: &SharedString,
    level: usize,
    indent: usize,
    context: &RowContext,
    rows: &mut Vec<VisibleRow>,
) {
    let set_size = visible_children(items, context.projection).count();
    for (index, item) in visible_children(items, context.projection).enumerate() {
        let contains_active = context.ancestors.contains(&item.id);
        let has_children = visible_children(&item.children, context.projection)
            .next()
            .is_some();
        // A query exposes the ancestry it matched inside without recording
        // that reveal as expansion the reader chose, and a controlled active
        // descendant stays reachable through a parent the reader collapsed.
        let expanded = has_children
            && (context.filtering || context.expanded.contains(&item.id) || contains_active);
        // A compact sidebar cannot expose descendants, so its visible ancestor
        // carries selected state instead.
        let active = context.active == Some(&item.id) || (context.collapsed && contains_active);
        rows.push(VisibleRow {
            id: item.id.clone(),
            label: item.label.clone(),
            icon: item.icon.clone(),
            badge: item.badge.clone(),
            parent: Some(parent.clone()),
            level,
            indent,
            position: index + 1,
            set_size,
            header: false,
            disabled: item.disabled,
            has_children,
            expanded,
            active,
            contains_active,
        });
        if expanded && !context.collapsed {
            push_rows(
                &item.children,
                &item.id,
                level + 1,
                indent + 1,
                context,
                rows,
            );
        }
    }
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

/// The stable, accessible control layered over one item row's presentation.
///
/// Rows are never focus targets: the tree owns one focus handle and points at
/// the active descendant, which is the only way a virtualized tree can retain
/// focus while the row that holds it unmounts.
fn sidebar_item_control(
    component_id: &SharedString,
    row: &VisibleRow,
    focused: bool,
    collapsed: bool,
    cx: &mut App,
) -> gpui_base::Button {
    let tokens = cx.theme().semantic_tokens();
    let label = row.label.clone();
    let ring = if focused {
        cx.theme().ring
    } else {
        cx.theme().transparent
    };
    let selected_border = if focused {
        cx.theme().ring
    } else {
        cx.theme().sidebar_border
    };
    gpui_base::Button::new((
        ElementId::from((ElementId::from(component_id.clone()), row.id.clone())),
        "control",
    ))
    .accessibility_id(format!("sidebar-nav.{component_id}.item.{}", row.id))
    .accessibility_label(label.clone())
    .role(Role::TreeItem)
    .disabled(row.disabled)
    .focusable(false)
    .selected(row.active)
    .aria_selected(row.active)
    .aria_level(row.level)
    .aria_position_in_set(row.position)
    .aria_size_of_set(row.set_size)
    .when(row.has_children, |this| {
        // A collapsed rail unmounts every descendant, so its parents report
        // what a reader can actually reach.
        this.aria_expanded(row.expanded && !collapsed)
    })
    .when(focused, |this| this.aria_active_descendant())
    .when_some(
        sidebar_item_description(
            row.badge.as_ref(),
            row.disabled,
            collapsed && row.contains_active,
        ),
        |this, description| this.aria_description(description),
    )
    .absolute()
    .inset_0()
    .size_full()
    .rounded(tokens.radius.sm)
    .border_1()
    .border_color(ring)
    .bg(cx.theme().transparent)
    .block_mouse_except_scroll()
    .when(collapsed, |this| {
        let tooltip_label = label.clone();
        this.when_some(row.icon.clone(), |this, icon| {
            this.child(Icon::default().path(icon).size_4())
        })
        .tooltip(move |window, cx| Tooltip::new(tooltip_label.clone()).build(window, cx))
    })
    .styles(|styles| {
        styles
            .selected(|style| {
                style
                    .bg(cx.theme().sidebar_accent)
                    .border_color(selected_border)
            })
            .disabled(|style| style.text_color(cx.theme().muted_foreground))
    })
}

/// The accessible control for one section header row.
///
/// A section is a real parent in a flattened tree â€” its items are levels below
/// it â€” so it is a tree node rather than loose text. It carries no application
/// intent, so activating it does nothing; the reader's arrow keys still walk
/// through it into the items it names.
fn sidebar_section_control(
    component_id: &SharedString,
    row: &VisibleRow,
    focused: bool,
    cx: &mut App,
) -> gpui_base::Button {
    let tokens = cx.theme().semantic_tokens();
    let label = row.label.clone();
    let debug_id = row.id.clone();
    gpui_base::Button::new((
        ElementId::from((ElementId::from(component_id.clone()), row.id.clone())),
        "section",
    ))
    .accessibility_id(format!("sidebar-nav.{component_id}.section.{}", row.id))
    .accessibility_label(label.clone())
    .role(Role::TreeItem)
    .focusable(false)
    .aria_level(row.level)
    .aria_position_in_set(row.position)
    .aria_size_of_set(row.set_size)
    .when(row.has_children, |this| this.aria_expanded(row.expanded))
    .when(focused, |this| this.aria_active_descendant())
    .debug_selector(move || format!("sidebar-nav-section-{debug_id}"))
    .w_full()
    .justify_start()
    .px(tokens.spacing.sm)
    .py(tokens.spacing.xs)
    .rounded(tokens.radius.sm)
    .border_1()
    .border_color(if focused {
        cx.theme().ring
    } else {
        cx.theme().transparent
    })
    .text_token(tokens.typography.xs)
    .text_color(cx.theme().sidebar_foreground.opacity(0.7))
    .child(label)
}

fn sidebar_tree_container(component_id: &SharedString) -> Stateful<Div> {
    div()
        .id((ElementId::from(component_id.clone()), "tree"))
        .accessibility_id(format!("sidebar-nav.{component_id}.tree"))
        .role(Role::Tree)
        .aria_label("Navigation items")
}

/// Renders one flattened row: a section header or an item.
fn render_row(
    row: &VisibleRow,
    component_id: &SharedString,
    collapsed: bool,
    focused: bool,
    owner: &WeakEntity<SidebarNav>,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    if row.header {
        return sidebar_section_control(component_id, row, focused, cx).into_any_element();
    }

    let tokens = cx.theme().semantic_tokens();
    let badge = row.badge.clone();
    let active = row.active;
    let has_children = row.has_children;
    let expanded = row.expanded;
    let hover_group: SharedString =
        format!("sidebar-nav-hover-group.{component_id}.{}", row.id).into();
    let hover_element_id = (
        ElementId::from((ElementId::from(component_id.clone()), row.id.clone())),
        "hover",
    );
    let hover_debug_id = row.id.clone();
    let item_debug_id = row.id.clone();
    let active_debug_id = row.id.clone();
    let activate_id = row.id.clone();
    let activate_owner = owner.clone();

    let menu_item = SidebarMenuItem::new(row.label.clone())
        .active(active)
        .collapsed(collapsed)
        .disable(row.disabled)
        .when(!collapsed, |this| {
            this.when_some(row.icon.clone(), |this, icon| {
                this.icon(Icon::default().path(icon))
            })
        })
        .when(
            !collapsed && (badge.is_some() || active || has_children),
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

    let control = sidebar_item_control(component_id, row, focused, collapsed, cx)
        .debug_selector(move || format!("sidebar-nav-item-{item_debug_id}"))
        .when(!collapsed && !row.disabled && !active, |this| {
            this.group(hover_group.clone()).child(
                div()
                    .id(hover_element_id)
                    .debug_selector(move || format!("sidebar-nav-hover-{hover_debug_id}"))
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .w_0()
                    .rounded(tokens.radius.sm)
                    .border_1()
                    .border_color(cx.theme().transparent)
                    .group_hover(hover_group, |style| {
                        style
                            .w_full()
                            .border_color(cx.theme().sidebar_accent_foreground)
                    }),
            )
        })
        .on_click(move |_, window, cx| {
            _ = activate_owner.update(cx, |nav, cx| {
                nav.activate_item(activate_id.clone(), window, cx)
            });
        });

    div()
        .id((ElementId::from(component_id.clone()), row.id.clone()))
        .when(active, |this| {
            this.debug_selector(move || format!("sidebar-nav-active-{active_debug_id}"))
        })
        .flex()
        .flex_row()
        .w_full()
        .min_w_0()
        // A virtual list places rows itself, so the rhythm between them has to
        // belong to the row. Padding rather than margin, so the measured row
        // height contains it and neighbours cannot overlap.
        .pb(tokens.spacing.xxs)
        // Nesting is a per-row guide now that rows are siblings: one stretched
        // rule per ancestor level, at the offset that level would have owned.
        .children((0..row.indent).map(|_| {
            div()
                .flex_none()
                .w(tokens.spacing.md)
                .border_l_1()
                .border_color(cx.theme().sidebar_border)
        }))
        .child(
            div()
                .relative()
                .flex_1()
                .min_w_0()
                .child(menu_item.render(
                    format!("sidebar-nav-menu.{component_id}.{}", row.id),
                    // SidebarMenuItem owns the pinned presentation. The
                    // transparent stable control blocks non-scroll pointer
                    // fallthrough and owns tooltip and AccessKit activation as
                    // one handler.
                    window,
                    cx,
                ))
                .child(control),
        )
        .into_any_element()
}

/// How the navigation draws its outer shell.
///
/// The shell is all this changes. Header, rows, filtering, the keyboard model,
/// and every accessibility identity are the same in both modes, so a docked
/// navigation differs from a standalone one by its box alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SidebarNavPresentation {
    /// A standalone rail that owns its own size: a fixed collapsed or expanded
    /// width, no flex growth, and a border along its trailing edge.
    #[default]
    Standalone,
    /// A shell that fills whatever box its host gives it, with no forced
    /// width, no rail sizing, and no edge border.
    ///
    /// For hosts that already own placement and size — a dock panel, a split,
    /// a resizable pane. Placement stays with the host and is deliberately not
    /// described here: a navigation docked along the bottom fills that
    /// region's width exactly as one docked left fills its column, because the
    /// shell contributes no width of its own either way.
    Embedded,
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
    /// Flattened visible rows, recomputed only where state changes.
    rows: Arc<[VisibleRow]>,
    /// Stable ID of the tree's roving row, retained across every projection.
    focused_row: Option<SharedString>,
    /// The tree's single focus handle; rows are active descendants of it.
    tree_focus: FocusHandle,
    /// Virtualized row list and the nav body's sole scroll owner.
    row_list: ListState,
    /// Rem size the retained row heights were measured against.
    resolved_layout: ResolvedLayoutKey,
    /// Rows the virtual list has constructed, which is what bounded
    /// construction is asserted against.
    constructed_rows: Rc<Cell<usize>>,
    /// How the shell is drawn. Read only by `render_shell`.
    presentation: SidebarNavPresentation,
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
            rows: Arc::from([]),
            focused_row: None,
            tree_focus: cx.focus_handle(),
            row_list: ListState::new(0, ListAlignment::Top, Pixels::ZERO),
            resolved_layout: ResolvedLayoutKey::default(),
            constructed_rows: Rc::new(Cell::new(0)),
            presentation: SidebarNavPresentation::default(),
            _input_subscription: subscription,
        }
    }

    /// Set how the shell is drawn (default:
    /// [`SidebarNavPresentation::Standalone`]).
    ///
    /// ```ignore
    /// let nav = cx.new(|cx| {
    ///     SidebarNav::new("docked-nav", window, cx)
    ///         .presentation(SidebarNavPresentation::Embedded)
    /// });
    /// ```
    pub fn presentation(mut self, presentation: SidebarNavPresentation) -> Self {
        self.presentation = presentation;
        self
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
        self.rebuild_rows(cx);
    }

    /// Replace the consumer-controlled active item ID.
    pub fn set_active_item(&mut self, item_id: impl Into<SharedString>, cx: &mut Context<Self>) {
        let item_id = item_id.into();
        if self.active_item.as_ref() != Some(&item_id) {
            self.active_item = Some(item_id);
            self.rebuild_rows(cx);
        }
    }

    /// Replace the owned collapsed state and emit its stable component identity.
    pub fn set_collapsed(&mut self, collapsed: bool, cx: &mut Context<Self>) {
        if self.collapsed == collapsed {
            return;
        }
        self.collapsed = collapsed;
        self.rebuild_rows(cx);
        cx.emit(SidebarNavEvent::CollapsedChanged {
            id: self.id.clone(),
            collapsed,
        });
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
        // InputState intentionally suppresses Change for programmatic values,
        // and it may be unmounted while collapsed, so the owner must rebuild
        // its own projection here.
        self.rebuild_rows(cx);
        self.emit_query_changed(query, cx);
    }

    /// Return whether the sidebar is currently collapsed.
    pub fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    /// Return the latest quick-filter query.
    pub fn query(&self) -> &SharedString {
        &self.query
    }

    /// Return how the shell is drawn.
    pub fn nav_presentation(&self) -> SidebarNavPresentation {
        self.presentation
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
        self.rebuild_rows(cx);
        self.emit_query_changed(query, cx);
    }

    /// Recomputes the flattened snapshot and notifies once.
    ///
    /// Every path that changes sections, query, expansion, active item, or the
    /// collapsed rail ends here, and `Render` ends nowhere near it.
    fn rebuild_rows(&mut self, cx: &mut Context<Self>) {
        let anchor = self.scroll_anchor();
        self.rows = visible_rows(
            &self.sections,
            &self.query,
            &self.expanded,
            self.active_item.as_ref(),
            self.collapsed,
        );
        self.row_list.reset(self.rows.len());
        self.restore_anchor(anchor);
        cx.notify();
    }

    /// The stable ID of the row currently at the top of the viewport.
    fn scroll_anchor(&self) -> Option<(SharedString, Pixels)> {
        let offset = self.row_list.logical_scroll_top();
        self.rows
            .get(offset.item_ix)
            .map(|row| (row.id.clone(), offset.offset_in_item))
    }

    /// Puts the anchored row back where it was, when it is still visible.
    fn restore_anchor(&self, anchor: Option<(SharedString, Pixels)>) {
        let Some((id, offset_in_item)) = anchor else {
            return;
        };
        let Some(item_ix) = self.rows.iter().position(|row| row.id == id) else {
            return;
        };
        self.row_list.scroll_to(ListOffset {
            item_ix,
            offset_in_item,
        });
    }

    /// Re-measures the row list after the window's rem size changed.
    ///
    /// Row heights cache text laid out at the previous rem, and neither a
    /// snapshot nor a collapse reports a zoom change. The row that was first
    /// on screen stays first.
    fn resolve_layout(&mut self, rem_size: Pixels, cx: &mut Context<Self>) {
        if !self.resolved_layout.observe(rem_size) {
            return;
        }
        let anchor = self.scroll_anchor();
        self.row_list.remeasure();
        self.restore_anchor(anchor);
        cx.notify();
    }

    fn emit_query_changed(&self, query: SharedString, cx: &mut Context<Self>) {
        cx.emit(SidebarNavEvent::QueryChanged {
            id: self.id.clone(),
            query,
        });
    }

    /// Index of the retained roving row, when its ID is still visible.
    fn focused_row_index(&self) -> Option<usize> {
        let focused = self.focused_row.as_ref()?;
        self.rows.iter().position(|row| row.id == *focused)
    }

    /// The row the tree treats as current.
    ///
    /// A tree has exactly one entry point. Falling back to the first visible
    /// row rather than to row zero keeps that entry point on screen after the
    /// reader scrolls somewhere else.
    fn roving_row_index(&self) -> Option<usize> {
        if self.rows.is_empty() {
            return None;
        }
        self.focused_row_index().or_else(|| {
            let first = self.row_list.logical_scroll_top().item_ix;
            (first < self.rows.len()).then_some(first)
        })
    }

    fn roving_row_id(&self) -> Option<SharedString> {
        self.roving_row_index()
            .and_then(|index| self.rows.get(index))
            .map(|row| row.id.clone())
    }

    /// Moves the roving row, revealing it and keeping focus on the tree.
    fn focus_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        self.focused_row = Some(row.id.clone());
        self.row_list.scroll_to_reveal_item(index);
        self.tree_focus.focus(window, cx);
        cx.notify();
        true
    }

    /// The next row in `direction` that a reader can land on.
    fn step(&self, from: usize, forward: bool) -> Option<usize> {
        let mut index = from;
        loop {
            index = if forward {
                index.checked_add(1)?
            } else {
                index.checked_sub(1)?
            };
            if !self.rows.get(index)?.disabled {
                return Some(index);
            }
        }
    }

    /// The first or last row a reader can land on.
    fn bound(&self, last: bool) -> Option<usize> {
        let mut candidates = (0..self.rows.len())
            .filter(|index| self.rows.get(*index).is_some_and(|row| !row.disabled));
        if last {
            candidates.next_back()
        } else {
            candidates.next()
        }
    }

    /// The first landable child of the row at `index`.
    fn first_child(&self, index: usize) -> Option<usize> {
        let row = self.rows.get(index)?;
        self.rows
            .iter()
            .enumerate()
            .skip(index + 1)
            .take_while(|(_, candidate)| candidate.level > row.level)
            .find(|(_, candidate)| {
                candidate.parent.as_ref() == Some(&row.id) && !candidate.disabled
            })
            .map(|(child, _)| child)
    }

    /// The row that owns the row at `index`, when a reader can land on it.
    fn parent_row(&self, index: usize) -> Option<usize> {
        let parent = self.rows.get(index)?.parent.as_ref()?;
        self.rows
            .iter()
            .position(|row| row.id == *parent && !row.disabled)
    }

    /// Records expansion the reader chose and reprojects the rows.
    fn set_expanded(&mut self, id: SharedString, expanded: bool, cx: &mut Context<Self>) {
        let changed = if expanded {
            self.expanded.insert(id)
        } else {
            self.expanded.remove(&id)
        };
        if changed {
            self.rebuild_rows(cx);
        }
    }

    /// The ARIA tree keyboard model, resolved against the flattened rows.
    ///
    /// Returns whether the tree consumed the key, so an unhandled one still
    /// reaches the application.
    fn navigate(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(current) = self.roving_row_index() else {
            return false;
        };
        let Some(row) = self.rows.get(current).cloned() else {
            return false;
        };
        match key {
            "up" => self
                .step(current, false)
                .is_some_and(|index| self.focus_row(index, window, cx)),
            "down" => self
                .step(current, true)
                .is_some_and(|index| self.focus_row(index, window, cx)),
            "home" => self
                .bound(false)
                .is_some_and(|index| self.focus_row(index, window, cx)),
            "end" => self
                .bound(true)
                .is_some_and(|index| self.focus_row(index, window, cx)),
            "right" => {
                if row.has_children && !row.expanded && !row.header {
                    self.set_expanded(row.id.clone(), true, cx);
                    true
                } else {
                    self.first_child(current)
                        .is_some_and(|index| self.focus_row(index, window, cx))
                }
            }
            "left" => {
                // Only expansion the reader chose collapses again: a branch
                // held open by the query or by the controlled active item
                // would not close, so Left walks to the parent instead.
                if !row.header && row.expanded && self.expanded.contains(&row.id) {
                    self.set_expanded(row.id.clone(), false, cx);
                    true
                } else {
                    self.parent_row(current)
                        .is_some_and(|index| self.focus_row(index, window, cx))
                }
            }
            "enter" | "space" => {
                if row.header {
                    return false;
                }
                self.activate_item(row.id.clone(), window, cx);
                true
            }
            _ => false,
        }
    }

    fn activate_item(
        &mut self,
        item_id: SharedString,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        // Pointer and keyboard activation share one contract, so a click also
        // moves the tree's roving row onto what it activated.
        self.focused_row = Some(item_id.clone());
        self.tree_focus.focus(window, cx);
        if has_children && !self.expanded.remove(&item_id) {
            self.expanded.insert(item_id.clone());
        }
        self.rebuild_rows(cx);
        cx.emit(SidebarNavEvent::Selected {
            id: self.id.clone(),
            item_id,
        });
    }
}

impl EventEmitter<SidebarNavEvent> for SidebarNav {}

impl Render for SidebarNav {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Measured row heights are only valid for the rem they were laid out
        // at. Reading it here mutates nothing; the reaction is deferred so
        // that render never notifies.
        let rem_size = window.rem_size();
        if !self.resolved_layout.matches(rem_size) {
            cx.defer_in(window, move |nav, _, cx| {
                nav.resolve_layout(rem_size, cx);
            });
        }

        let content = self.render_content(window, cx);
        self.render_shell(cx).children(content)
    }
}

impl SidebarNav {
    /// The outer shell: stable identity, navigation semantics, and the box the
    /// host sees.
    ///
    /// This is the only rendering [`SidebarNavPresentation`] reaches, which is
    /// what keeps the setting additive: the content below draws the same rows
    /// either way.
    fn render_shell(&self, cx: &mut App) -> Stateful<Div> {
        let tokens = cx.theme().semantic_tokens();
        let collapsed = self.collapsed;
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
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(cx.theme().sidebar)
            .text_color(cx.theme().sidebar_foreground)
            .map(|shell| match self.presentation {
                // A standalone rail sizes itself and draws the edge between
                // itself and whatever sits beside it.
                SidebarNavPresentation::Standalone => shell
                    .w(if collapsed {
                        tokens.spacing.xxl * 1.5
                    } else {
                        tokens.spacing.xxl * 8.
                    })
                    .flex_none()
                    .border_r_1()
                    .border_color(cx.theme().sidebar_border),
                // The host owns placement and size, so the shell contributes
                // neither a width nor an edge: docked along the bottom it
                // fills that region's width instead of keeping a rail's, and
                // the host's own divider is the only border drawn.
                SidebarNavPresentation::Embedded => shell.w_full().min_w_0(),
            })
    }

    /// The shell's children, in order: header, virtualized row tree, and the
    /// empty status.
    ///
    /// Nothing here reads [`SidebarNavPresentation`].
    fn render_content(&self, window: &mut Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let tokens = cx.theme().semantic_tokens();
        let new_task_owner = cx.weak_entity();
        let collapse_owner = cx.weak_entity();
        let row_owner = cx.weak_entity();
        let key_owner = cx.weak_entity();
        let collapsed = self.collapsed;
        let input = self.input.clone();
        let id = self.id.clone();
        let rows = self.rows.clone();
        let row_component_id = self.id.clone();
        let constructed_rows = self.constructed_rows.clone();
        let roving_row = self.roving_row_id();
        let tree_focused = self.tree_focus.is_focused(window);
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
        let row_list = self.row_list.clone();
        let rows_are_empty = self.rows.is_empty();

        let mut content = vec![
            div()
                .w_full()
                .flex_none()
                .p(tokens.spacing.sm)
                .child(header)
                .into_any_element(),
            div()
                .relative()
                .w_full()
                .flex_1()
                .min_h_0()
                .child(
                    sidebar_tree_container(&self.id)
                        // The tree is the single tab stop and holds focus
                        // for every row, so a row that unmounts while
                        // scrolled away cannot take the reader's focus
                        // out of the navigation with it.
                        .track_focus(&self.tree_focus.clone().tab_index(0).tab_stop(true))
                        .size_full()
                        .flex()
                        .flex_col()
                        .px(tokens.spacing.sm)
                        .gap(tokens.spacing.xxs)
                        .child(
                            list(row_list.clone(), move |index, window, cx| {
                                constructed_rows.set(constructed_rows.get() + 1);
                                rows.get(index)
                                    .map(|row| {
                                        render_row(
                                            row,
                                            &row_component_id,
                                            collapsed,
                                            tree_focused && roving_row.as_ref() == Some(&row.id),
                                            &row_owner,
                                            window,
                                            cx,
                                        )
                                    })
                                    .unwrap_or_else(|| div().hidden().into_any_element())
                            })
                            .size_full(),
                        )
                        .vertical_scrollbar(&row_list)
                        .on_key_down(move |event, window, cx| {
                            if event.keystroke.modifiers.modified() {
                                return;
                            }
                            let key = event.keystroke.key.clone();
                            let handled = key_owner
                                .update(cx, |nav, cx| nav.navigate(&key, window, cx))
                                .unwrap_or(false);
                            if handled {
                                cx.stop_propagation();
                            }
                        }),
                )
                // Capture-phase containment wins over an ancestor catalog
                // list and releases the wheel at either nav edge.
                .child(list_scroll_mask(&self.row_list))
                .into_any_element(),
        ];
        if rows_are_empty && !collapsed {
            content.push(
                div()
                    .id((ElementId::from(self.id.clone()), "empty-status"))
                    .debug_selector(move || {
                        if empty_selector {
                            "sidebar-nav-empty".to_owned()
                        } else {
                            "sidebar-nav-no-results".to_owned()
                        }
                    })
                    .flex_none()
                    .p(tokens.spacing.sm)
                    .role(Role::Status)
                    .aria_label(empty_message.clone())
                    .text_token(tokens.typography.sm)
                    .text_color(cx.theme().muted_foreground)
                    .child(empty_message)
                    .into_any_element(),
            );
        }
        content
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Bounds, Element as _, ListAlignment, ListState, RenderOnce as _, ScrollDelta,
        ScrollWheelEvent, TestAppContext, VisualTestContext, accesskit, canvas, list, px, size,
    };
    use gpui_component::theme::Theme;
    use std::sync::{Arc, Mutex};

    type CapturedSemanticNodes = Arc<Mutex<Vec<(Option<Role>, accesskit::Node)>>>;

    struct SemanticsProbe {
        rows: Arc<[VisibleRow]>,
        captured: CapturedSemanticNodes,
    }

    struct SidebarInCatalogProbe {
        nav: Entity<SidebarNav>,
        catalog: ListState,
    }

    fn probe_sections() -> Vec<SidebarSection> {
        vec![
            SidebarSection::new("workspace", "Workspace").items([
                SidebarNavItem::new("overview", "Overview"),
                SidebarNavItem::new("orders", "Orders")
                    .badge("12")
                    .children([
                        SidebarNavItem::new("history", "History"),
                        SidebarNavItem::new("suppliers", "Suppliers")
                            .children([SidebarNavItem::new("risk", "Risk reports")]),
                        SidebarNavItem::new("exports", "Exports").disabled(true),
                    ]),
            ]),
            SidebarSection::new("reports", "Reports")
                .items([SidebarNavItem::new("live-report", "Reports")]),
        ]
    }

    /// Every parent expanded, which is what `set_sections` does for a snapshot
    /// whose parents are all new.
    fn every_parent(sections: &[SidebarSection]) -> HashSet<SharedString> {
        let mut parents = HashSet::new();
        for section in sections {
            collect_parent_ids(&section.items, &mut parents);
        }
        parents
    }

    fn row_ids(rows: &[VisibleRow]) -> Vec<&str> {
        rows.iter().map(|row| row.id.as_ref()).collect()
    }

    fn row<'a>(rows: &'a [VisibleRow], id: &str) -> &'a VisibleRow {
        rows.iter()
            .find(|row| row.id == id)
            .expect("the row should be in the snapshot")
    }

    impl SidebarInCatalogProbe {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let nav = cx.new(|cx| {
                let mut nav = SidebarNav::new("nested-nav", window, cx);
                nav.set_sections(
                    [
                        SidebarSection::new("workspace", "Workspace").items((0..30).map(|index| {
                            SidebarNavItem::new(
                                format!("destination-{index}"),
                                format!("Destination {index}"),
                            )
                        })),
                    ],
                    cx,
                );
                nav
            });
            Self {
                nav,
                catalog: ListState::new(8, ListAlignment::Top, px(0.)),
            }
        }
    }

    /// A nav in a box too short for its rows, so the row list scrolls.
    struct ShortSidebarProbe {
        nav: Entity<SidebarNav>,
    }

    impl ShortSidebarProbe {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let nav = cx.new(|cx| {
                let mut nav = SidebarNav::new("zoom-nav", window, cx);
                nav.set_sections(
                    (0..4).map(|section| {
                        SidebarSection::new(
                            format!("section-{section}"),
                            format!("Section {section}"),
                        )
                        .items((0..8).map(|item| {
                            SidebarNavItem::new(
                                format!("s{section}-item-{item}"),
                                format!("Destination {section}.{item}"),
                            )
                        }))
                    }),
                    cx,
                );
                nav
            });
            Self { nav }
        }
    }

    impl Render for ShortSidebarProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(320.)).h(px(240.)).child(self.nav.clone())
        }
    }

    /// One section holding ten thousand descendants, all expanded.
    struct HugeSidebarProbe {
        nav: Entity<SidebarNav>,
    }

    impl HugeSidebarProbe {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let nav = cx.new(|cx| {
                let mut nav = SidebarNav::new("huge-nav", window, cx);
                nav.set_sections(
                    [
                        SidebarSection::new("workspace", "Workspace").items((0..100).map(
                            |parent| {
                                SidebarNavItem::new(
                                    format!("parent-{parent}"),
                                    format!("Parent {parent}"),
                                )
                                .children((0..99).map(
                                    move |child| {
                                        SidebarNavItem::new(
                                            format!("item-{parent}-{child}"),
                                            format!("Item {parent}.{child}"),
                                        )
                                    },
                                ))
                            },
                        )),
                    ],
                    cx,
                );
                nav
            });
            Self { nav }
        }
    }

    impl Render for HugeSidebarProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(320.)).h(px(240.)).child(self.nav.clone())
        }
    }

    impl Render for SidebarInCatalogProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let nav = self.nav.clone();
            div().w(px(320.)).h(px(260.)).child(
                list(self.catalog.clone(), move |index, _, _| {
                    if index == 0 {
                        div()
                            .w(px(280.))
                            .h(px(180.))
                            .child(nav.clone())
                            .into_any_element()
                    } else {
                        div().w(px(280.)).h(px(100.)).into_any_element()
                    }
                })
                .size_full(),
            )
        }
    }

    impl Render for SemanticsProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            let rows = self.rows.clone();
            canvas(
                move |_, window, cx| {
                    let component_id: SharedString = "test".into();
                    let collapsed = false;
                    let nodes = rows
                        .iter()
                        .map(|row| {
                            let control = if row.header {
                                sidebar_section_control(&component_id, row, false, cx)
                            } else {
                                sidebar_item_control(&component_id, row, false, collapsed, cx)
                                    .on_click(|_, _, _| {})
                            }
                            .render(window, cx)
                            .into_element();
                            let mut node = accesskit::Node::new(Role::Unknown);
                            control.write_a11y_info(&mut node);
                            (control.a11y_role(), node)
                        })
                        .collect();
                    *captured.lock().expect("capture mutex should be available") = nodes;
                },
                |_, _, _, _| {},
            )
        }
    }

    /// Captures the AccessKit node of every row in `rows`, in row order.
    fn capture_semantics(
        cx: &mut TestAppContext,
        rows: Arc<[VisibleRow]>,
    ) -> Vec<(Option<Role>, accesskit::Node)> {
        cx.update(crate::init);
        let captured: CapturedSemanticNodes = Arc::new(Mutex::new(Vec::new()));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| SemanticsProbe { rows, captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        result
            .lock()
            .expect("capture mutex should be available")
            .drain(..)
            .collect()
    }

    #[test]
    fn the_snapshot_flattens_expanded_branches_and_states_their_relationships() {
        let sections = probe_sections();
        let rows = visible_rows(&sections, "", &every_parent(&sections), None, false);

        assert_eq!(
            row_ids(&rows),
            [
                "workspace",
                "overview",
                "orders",
                "history",
                "suppliers",
                "risk",
                "exports",
                "reports",
                "live-report",
            ]
        );

        let workspace = row(&rows, "workspace");
        assert!(workspace.header);
        assert_eq!(
            (workspace.level, workspace.position, workspace.set_size),
            (1, 1, 2)
        );

        let orders = row(&rows, "orders");
        assert_eq!(orders.parent.as_deref(), Some("workspace"));
        assert_eq!((orders.level, orders.position, orders.set_size), (2, 2, 2));
        assert!(orders.has_children && orders.expanded);

        let suppliers = row(&rows, "suppliers");
        assert_eq!(suppliers.parent.as_deref(), Some("orders"));
        assert_eq!(
            (suppliers.level, suppliers.position, suppliers.set_size),
            (3, 2, 3)
        );

        let risk = row(&rows, "risk");
        assert_eq!(risk.parent.as_deref(), Some("suppliers"));
        assert_eq!((risk.level, risk.indent), (4, 2));
        assert!(!risk.has_children);
    }

    #[test]
    fn a_collapsed_rail_flattens_root_items_without_headers() {
        let sections = probe_sections();
        let rows = visible_rows(&sections, "", &every_parent(&sections), None, true);

        assert_eq!(row_ids(&rows), ["overview", "orders", "live-report"]);
        assert_eq!(row(&rows, "orders").level, 1);
    }

    #[test]
    fn a_collapsed_rail_marks_the_ancestor_of_the_controlled_active_item() {
        let sections = probe_sections();
        let expanded = every_parent(&sections);
        let active: SharedString = "risk".into();

        let expanded_rows = visible_rows(&sections, "", &expanded, Some(&active), false);
        assert!(row(&expanded_rows, "risk").active);
        assert!(!row(&expanded_rows, "orders").active);
        assert!(row(&expanded_rows, "orders").contains_active);

        let rail = visible_rows(&sections, "", &expanded, Some(&active), true);
        assert!(row(&rail, "orders").active);
    }

    #[test]
    fn a_controlled_active_descendant_stays_visible_through_a_collapsed_parent() {
        let sections = probe_sections();
        let active: SharedString = "risk".into();
        // Nothing the reader expanded: only the controlled active ancestry
        // opens the path to it.
        let rows = visible_rows(&sections, "", &HashSet::new(), Some(&active), false);

        assert_eq!(
            row_ids(&rows),
            [
                "workspace",
                "overview",
                "orders",
                "history",
                "suppliers",
                "risk",
                "exports",
                "reports",
                "live-report"
            ]
        );
    }

    #[test]
    fn the_projection_retains_only_the_matching_branch_and_its_ancestors() {
        let sections = probe_sections();
        let rows = visible_rows(&sections, "RISK", &HashSet::new(), None, false);

        assert_eq!(row_ids(&rows), ["workspace", "orders", "suppliers", "risk"]);
        // A query reveals matched ancestry without the reader having expanded
        // anything, and position metadata describes the visible set.
        let suppliers = row(&rows, "suppliers");
        assert!(suppliers.expanded);
        assert_eq!((suppliers.position, suppliers.set_size), (1, 1));
    }

    #[test]
    fn a_matching_section_label_keeps_the_whole_section() {
        let sections = probe_sections();
        let rows = visible_rows(&sections, "workspace", &HashSet::new(), None, false);

        assert!(row_ids(&rows).contains(&"exports"));
        assert!(!row_ids(&rows).contains(&"reports"));
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
    fn tree_rows_expose_level_position_expansion_and_activation_semantics(cx: &mut TestAppContext) {
        let sections = probe_sections();
        let active: SharedString = "risk".into();
        let rows = visible_rows(
            &sections,
            "",
            &every_parent(&sections),
            Some(&active),
            false,
        );
        let ids = row_ids(&rows)
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let nodes = capture_semantics(cx, rows);
        let node = |id: &str| {
            let index = ids
                .iter()
                .position(|candidate| candidate == id)
                .expect("the row should be in the snapshot");
            &nodes[index]
        };

        // A section is the parent of its items, so it is level 1 and its root
        // items are level 2: parent and child relationships a flattened tree
        // can only state, never nest.
        let (workspace_role, workspace) = node("workspace");
        assert_eq!(*workspace_role, Some(Role::TreeItem));
        assert_eq!(workspace.label(), Some("Workspace"));
        assert_eq!(workspace.level(), Some(1));
        assert_eq!(workspace.position_in_set(), Some(1));
        assert_eq!(workspace.size_of_set(), Some(2));
        assert_eq!(workspace.is_expanded(), Some(true));

        let (orders_role, orders) = node("orders");
        assert_eq!(*orders_role, Some(Role::TreeItem));
        assert_eq!(orders.level(), Some(2));
        assert_eq!(orders.position_in_set(), Some(2));
        assert_eq!(orders.size_of_set(), Some(2));
        assert_eq!(orders.is_expanded(), Some(true));
        assert_eq!(orders.description(), Some("Badge 12"));
        assert!(orders.supports_action(accesskit::Action::Click));

        let (_, risk) = node("risk");
        assert_eq!(risk.label(), Some("Risk reports"));
        assert_eq!(risk.level(), Some(4));
        assert_eq!(risk.is_selected(), Some(true));
        assert_eq!(risk.is_expanded(), None, "a leaf has nothing to expand");

        let (disabled_role, disabled) = node("exports");
        assert_eq!(*disabled_role, Some(Role::TreeItem));
        assert_eq!(disabled.description(), Some("Unavailable"));
        assert_eq!(disabled.position_in_set(), Some(3));
        assert_eq!(disabled.size_of_set(), Some(3));
        assert!(!disabled.supports_action(accesskit::Action::Click));
    }

    #[gpui::test]
    fn a_collapsed_parent_reports_unmounted_children_as_not_expanded(cx: &mut TestAppContext) {
        let sections = probe_sections();
        let active: SharedString = "risk".into();
        let rail = visible_rows(&sections, "", &every_parent(&sections), Some(&active), true);
        let ids = row_ids(&rail)
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();

        // The rail renders through the same control with `collapsed` set, so
        // capture it the way the row list does.
        cx.update(crate::init);
        let captured: CapturedSemanticNodes = Arc::new(Mutex::new(Vec::new()));
        let result = captured.clone();
        let rows = rail.clone();
        let (_, cx) = cx.add_window_view(move |_, _| RailSemanticsProbe { rows, captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let nodes = result
            .lock()
            .expect("capture mutex should be available")
            .drain(..)
            .collect::<Vec<_>>();

        let index = ids
            .iter()
            .position(|candidate| candidate == "orders")
            .expect("the rail should render root items");
        let (role, parent) = &nodes[index];
        assert_eq!(*role, Some(Role::TreeItem));
        assert_eq!(parent.label(), Some("Orders"));
        assert_eq!(parent.is_selected(), Some(true));
        assert_eq!(
            parent.description(),
            Some("Badge 12. Contains selected item")
        );
        assert_eq!(parent.is_expanded(), Some(false));
    }

    struct RailSemanticsProbe {
        rows: Arc<[VisibleRow]>,
        captured: CapturedSemanticNodes,
    }

    impl Render for RailSemanticsProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            let rows = self.rows.clone();
            canvas(
                move |_, window, cx| {
                    let component_id: SharedString = "test".into();
                    let nodes = rows
                        .iter()
                        .map(|row| {
                            let control = sidebar_item_control(&component_id, row, false, true, cx)
                                .on_click(|_, _, _| {})
                                .render(window, cx)
                                .into_element();
                            let mut node = accesskit::Node::new(Role::Unknown);
                            control.write_a11y_info(&mut node);
                            (control.a11y_role(), node)
                        })
                        .collect();
                    *captured.lock().expect("capture mutex should be available") = nodes;
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn the_tree_container_is_the_navigation_tree(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured: CapturedSemanticNodes = Arc::new(Mutex::new(Vec::new()));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| TreeContainerProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let nodes = result
            .lock()
            .expect("capture mutex should be available")
            .drain(..)
            .collect::<Vec<_>>();

        let (role, tree) = &nodes[0];
        assert_eq!(*role, Some(Role::Tree));
        assert_eq!(tree.label(), Some("Navigation items"));
    }

    struct TreeContainerProbe {
        captured: CapturedSemanticNodes,
    }

    impl Render for TreeContainerProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, _, _| {
                    let tree = sidebar_tree_container(&"test".into()).into_element();
                    let mut node = accesskit::Node::new(Role::Unknown);
                    tree.write_a11y_info(&mut node);
                    *captured.lock().expect("capture mutex should be available") =
                        vec![(tree.a11y_role(), node)];
                },
                |_, _, _, _| {},
            )
        }
    }

    /// Zooms the way the shell does: the theme carries the base type size and
    /// `Root` hands it to the window every frame.
    ///
    /// Two draws, because the nav notices the new rem while rendering and
    /// reacts afterwards. Nothing here calls `remeasure`; that the nav does it
    /// unprompted is the property under test.
    fn zoom_to(cx: &mut VisualTestContext, font_size: f32) {
        cx.update(|window, cx| {
            Theme::global_mut(cx).font_size = px(font_size);
            window.set_rem_size(Theme::global(cx).font_size);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }

    fn top_row(nav: &SidebarNav) -> Option<SharedString> {
        let offset = nav.row_list.logical_scroll_top();
        nav.rows.get(offset.item_ix).map(|row| row.id.clone())
    }

    #[gpui::test]
    fn zooming_re_measures_rows_and_keeps_the_first_visible_row(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(ShortSidebarProbe::new);
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let nav = probe.read_with(cx, |probe, _| probe.nav.clone());

        // Rest on a row other than the first, so preserving the anchor is a
        // claim about the row and not about the top of the list.
        nav.read_with(cx, |nav, _| {
            nav.row_list.scroll_to(ListOffset {
                item_ix: 4,
                offset_in_item: px(0.),
            })
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let anchor = nav.read_with(cx, |nav, _| top_row(nav));
        assert_eq!(anchor.as_deref(), Some("s0-item-3"));

        // 100%, 150%, 200% of the 16px base.
        for font_size in [16., 24., 32.] {
            zoom_to(cx, font_size);

            nav.read_with(cx, |nav, _| {
                assert!(
                    nav.resolved_layout.matches(px(font_size)),
                    "the nav must notice {font_size}px type from its own render"
                );
                assert_eq!(
                    top_row(nav),
                    anchor,
                    "the row that was first on screen stays first at {font_size}px type"
                );
            });
            assert!(
                cx.debug_bounds("sidebar-nav-item-s0-item-3").is_some(),
                "the anchored row stays reachable at {font_size}px type"
            );
        }
    }

    #[gpui::test]
    fn one_section_of_ten_thousand_descendants_constructs_a_bounded_window(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(HugeSidebarProbe::new);
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        let nav = probe.read_with(cx, |probe, _| probe.nav.clone());
        nav.update(cx, |nav, _| nav.constructed_rows.set(0));
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let (rows, first_draw) =
            nav.read_with(cx, |nav, _| (nav.rows.len(), nav.constructed_rows.get()));
        assert_eq!(rows, 10_001, "one header over ten thousand descendants");
        assert!(
            (1..=64).contains(&first_draw),
            "a 240px viewport constructed {first_draw} of {rows} rows"
        );

        // One page further down is still one window of work, not a walk from
        // the top of the section.
        nav.update(cx, |nav, _| {
            nav.row_list.scroll_to(ListOffset {
                item_ix: 5_000,
                offset_in_item: px(0.),
            });
            nav.constructed_rows.set(0);
        });
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let scrolled = nav.read_with(cx, |nav, _| nav.constructed_rows.get());
        assert!(
            (1..=64).contains(&scrolled),
            "scrolled draw constructed {scrolled} of {rows} rows"
        );
        // Row 5,000 is the last child of parent 49, five thousand rows past
        // anything the first draw touched.
        assert!(cx.debug_bounds("sidebar-nav-item-item-49-98").is_some());
    }

    #[gpui::test]
    fn wheel_over_scrollable_sidebar_moves_sidebar_before_catalog(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (probe, cx) = cx.add_window_view(SidebarInCatalogProbe::new);
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let nav_bounds = cx
            .debug_bounds("sidebar-nav-nested-nav")
            .expect("the nested sidebar should render");
        let (nav_list, catalog) = probe.read_with(cx, |probe, cx| {
            (probe.nav.read(cx).row_list.clone(), probe.catalog.clone())
        });
        assert!(nav_list.max_offset_for_scrollbar().y > px(0.));

        cx.simulate_event(ScrollWheelEvent {
            position: nav_bounds.center(),
            delta: ScrollDelta::Pixels(gpui::point(px(0.), px(-40.))),
            ..Default::default()
        });

        assert_eq!(nav_list.scroll_px_offset_for_scrollbar().y, px(-40.));
        let catalog_top = catalog.logical_scroll_top();
        assert_eq!(
            (catalog_top.item_ix, catalog_top.offset_in_item),
            (0, px(0.))
        );

        // Rows are measured one window at a time, so the end of the list moves
        // further down as more of them are measured. Settle there before
        // asserting that the wheel is released: releasing early would hand the
        // catalog a wheel the nav could still use. (Virtualizing rows rather
        // than section roots is what made this take more than one pass.)
        let mut bottom = nav_list.scroll_px_offset_for_scrollbar().y;
        for _ in 0..8 {
            nav_list.scroll_by(nav_list.max_offset_for_scrollbar().y);
            cx.update(|window, cx| window.draw(cx).clear(cx));
            let settled = nav_list.scroll_px_offset_for_scrollbar().y;
            if settled == bottom {
                break;
            }
            bottom = settled;
        }
        cx.simulate_event(ScrollWheelEvent {
            position: nav_bounds.center(),
            delta: ScrollDelta::Pixels(gpui::point(px(0.), px(-40.))),
            ..Default::default()
        });

        assert_eq!(nav_list.scroll_px_offset_for_scrollbar().y, bottom);
        let catalog_top = catalog.logical_scroll_top();
        assert_ne!(
            (catalog_top.item_ix, catalog_top.offset_in_item),
            (0, px(0.))
        );
    }

    /// A nav inside a host box of an arbitrary size, in either presentation.
    struct HostedNavProbe {
        nav: Entity<SidebarNav>,
        size: gpui::Size<Pixels>,
    }

    impl HostedNavProbe {
        fn new(
            presentation: SidebarNavPresentation,
            host: gpui::Size<Pixels>,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) -> Self {
            let nav = cx.new(|cx| {
                let mut nav = SidebarNav::new("hosted-nav", window, cx).presentation(presentation);
                nav.set_sections(probe_sections(), cx);
                nav
            });
            Self { nav, size: host }
        }
    }

    impl Render for HostedNavProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .id("hosted-nav-host")
                .debug_selector(|| "hosted-nav-host".to_owned())
                .w(self.size.width)
                .h(self.size.height)
                .flex()
                .child(self.nav.clone())
        }
    }

    /// Draws one hosted nav and returns the host's and the nav's bounds.
    fn hosted_nav_bounds(
        cx: &mut TestAppContext,
        presentation: SidebarNavPresentation,
        host: gpui::Size<Pixels>,
    ) -> (Bounds<Pixels>, Bounds<Pixels>) {
        cx.update(crate::init);
        let (_, cx) = cx
            .add_window_view(move |window, cx| HostedNavProbe::new(presentation, host, window, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(900.), px(700.)));
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        (
            cx.debug_bounds("hosted-nav-host")
                .expect("the host should render"),
            cx.debug_bounds("sidebar-nav-hosted-nav")
                .expect("the navigation should render"),
        )
    }

    #[gpui::test]
    fn an_embedded_shell_fills_a_wide_short_host(cx: &mut TestAppContext) {
        let (host, nav) = hosted_nav_bounds(
            cx,
            SidebarNavPresentation::Embedded,
            size(px(620.), px(160.)),
        );

        // The bottom-dock case: an embedded nav takes the width it is given
        // rather than a rail's, which is the whole point of the mode.
        assert_eq!(nav.size, host.size);
        assert_eq!(nav.origin, host.origin);
    }

    #[gpui::test]
    fn an_embedded_shell_fills_a_narrow_tall_host(cx: &mut TestAppContext) {
        let (host, nav) = hosted_nav_bounds(
            cx,
            SidebarNavPresentation::Embedded,
            size(px(196.), px(460.)),
        );

        // Narrower than the expanded rail: the shell shrinks with the host
        // instead of overflowing it.
        assert_eq!(nav.size, host.size);
        assert_eq!(nav.origin, host.origin);
    }

    #[gpui::test]
    fn the_default_shell_keeps_a_rail_width_inside_a_wide_host(cx: &mut TestAppContext) {
        let host_size = size(px(620.), px(360.));
        let (host, nav) = hosted_nav_bounds(cx, SidebarNavPresentation::Standalone, host_size);

        assert_eq!(nav.size.height, host.size.height);
        assert!(
            nav.size.width < host.size.width,
            "a standalone rail sizes itself: {:?} in a {:?} host",
            nav.size,
            host.size,
        );
    }

    #[gpui::test]
    fn the_shells_differ_only_by_width_growth_and_trailing_edge(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (nav, cx) = cx.add_window_view(|window, cx| SidebarNav::new("shell", window, cx));
        let cx: &mut VisualTestContext = cx;
        cx.run_until_parked();

        let (standalone, embedded, rail) = cx.update(|_, cx| {
            let rail = cx.theme().semantic_tokens().spacing.xxl * 8.;
            let standalone = nav.update(cx, |nav, cx| nav.render_shell(cx).style().clone());
            let embedded = nav.update(cx, |nav, cx| {
                nav.presentation = SidebarNavPresentation::Embedded;
                let style = nav.render_shell(cx).style().clone();
                nav.presentation = SidebarNavPresentation::Standalone;
                style
            });
            (standalone, embedded, rail)
        });

        let expected_rail = div().w(rail).flex_none().border_r_1().style().clone();
        assert_eq!(standalone.size.width, expected_rail.size.width);
        assert_eq!(standalone.flex_grow, expected_rail.flex_grow);
        assert_eq!(standalone.flex_shrink, expected_rail.flex_shrink);
        assert_eq!(standalone.border_widths, expected_rail.border_widths);

        // Embedded keeps neither the rail's width nor its edge; the host draws
        // whatever divider the composition needs.
        let expected_embedded = div().w_full().style().clone();
        assert_eq!(embedded.size.width, expected_embedded.size.width);
        assert_eq!(embedded.flex_grow, expected_embedded.flex_grow);
        assert_eq!(embedded.flex_shrink, expected_embedded.flex_shrink);
        assert_eq!(embedded.border_widths, expected_embedded.border_widths);

        // Everything the shell is not asked to change is shared, so the seam
        // cannot quietly become a second skin.
        assert_eq!(standalone.size.height, embedded.size.height);
        assert_eq!(standalone.background, embedded.background);
        assert_eq!(standalone.text.color, embedded.text.color);
    }
}
