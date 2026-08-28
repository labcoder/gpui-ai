//! Presentation of the navigation's controls: the header buttons and one row.
//!
//! Every accessible identity a reader meets is built here, layered over the
//! pinned `SidebarMenuItem` presentation as one transparent stable control, so
//! a row's semantics and its pixels cannot drift apart.

use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window,
    div, percentage, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, h_flex,
    sidebar::{SidebarItem as _, SidebarMenuItem},
    tooltip::Tooltip,
};

use crate::theme::SemanticStyledExt as _;

use super::{SidebarNav, rows::VisibleRow};

pub(super) fn nav_control(
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
        // Header controls rail with the filter input beside them, so they
        // take the input's own height tier from the size policy.
        .h(crate::sizing::SizeTokens::read(cx).control_lg())
        .px(tokens.spacing.sm)
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
pub(super) fn sidebar_item_control(
    component_id: &SharedString,
    row: &VisibleRow,
    focused: bool,
    collapsed: bool,
    cx: &mut App,
) -> gpui_base::Button {
    let label = row.label.clone();
    let ring = if focused {
        cx.theme().ring
    } else {
        cx.theme().transparent
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
    .rounded(cx.theme().radius)
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
    .styles(|styles| styles.disabled(|style| style.text_color(cx.theme().muted_foreground)))
}

/// The accessible control for one section header row.
///
/// A section is a real parent in a flattened tree — its items are levels below
/// it — so it is a tree node rather than loose text. It carries no application
/// intent, so activating it does nothing; the reader's arrow keys still walk
/// through it into the items it names.
pub(super) fn sidebar_section_control(
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

pub(super) fn sidebar_tree_container(component_id: &SharedString) -> Stateful<Div> {
    div()
        .id((ElementId::from(component_id.clone()), "tree"))
        .accessibility_id(format!("sidebar-nav.{component_id}.tree"))
        .role(Role::Tree)
        .aria_label("Navigation items")
}

/// Renders one flattened row: a section header or an item.
pub(super) fn render_row(
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
    let item_debug_id = row.id.clone();
    let active_debug_id = row.id.clone();
    let activate_id = row.id.clone();
    let activate_owner = owner.clone();

    let menu_item = SidebarMenuItem::new(row.label.clone())
        .active(active)
        .collapsed(collapsed)
        .disable(row.disabled)
        .when(!collapsed, |this| {
            // Row icons draw at 16px beside the 14px labels — the icon
            // scale the size principles set — instead of inheriting the
            // label's own font size.
            this.when_some(row.icon.clone(), |this, icon| {
                this.icon(Icon::default().path(icon).size_4())
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
                                crate::status::chip_frame(
                                    cx.theme().muted_foreground,
                                    crate::status::ChipStrength::Neutral,
                                    cx,
                                )
                                .px(cx.theme().semantic_tokens().spacing.xs)
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
                .border_color(cx.theme().sidebar_border.opacity(0.6))
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
