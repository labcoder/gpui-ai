//! Bounded, feature-oriented comparison table values and presentation.

use std::{collections::HashSet, sync::Arc, time::Duration};

use gpui::{
    Animation, AnimationExt as _, App, Context, EventEmitter, FocusHandle, Focusable, Hsla,
    InteractiveElement as _, IntoElement, ParentElement as _, Render, Role, ScrollHandle,
    ScrollWheelEvent, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    prelude::FluentBuilder as _, px,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, h_flex, scroll::ScrollableElement as _, text::TextView,
};

use crate::{
    control::outlined_control_with_label,
    records_table::escape_markdown_text,
    scrolling::ScrollRoom,
    stream::{ProgressState, Progressive},
    theme::SemanticStyledExt as _,
};

/// Maximum number of side-by-side items accepted by a comparison snapshot.
pub const MAX_COMPARISON_ITEMS: usize = 12;

/// Maximum number of feature rows accepted by a comparison snapshot.
pub const MAX_COMPARISON_FEATURES: usize = 128;

/// Presentation state attached to one comparison item.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonItemState {
    /// An ordinary comparison item.
    #[default]
    Default,
    /// An item the application wants to call out as recommended or notable.
    Highlighted,
    /// An item that cannot currently be selected or activated.
    Disabled,
}

/// One consumer-owned item displayed as a comparison column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonItem {
    id: SharedString,
    label: SharedString,
    description: Option<SharedString>,
    state: ComparisonItemState,
}

impl ComparisonItem {
    /// Creates an ordinary item with stable identity and a visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            state: ComparisonItemState::Default,
        }
    }

    /// Adds readable supporting copy for the item header.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the consumer-owned item presentation state.
    pub fn state(mut self, state: ComparisonItemState) -> Self {
        self.state = state;
        self
    }

    /// Returns the stable application ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the visible label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns optional readable supporting copy.
    pub fn description_text(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the current consumer-owned presentation state.
    pub fn item_state(&self) -> ComparisonItemState {
        self.state
    }
}

/// One readable value at the intersection of a feature and comparison item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonValue {
    item_id: SharedString,
    display: SharedString,
    included: Option<bool>,
}

impl ComparisonValue {
    /// Creates a free-form readable value for one stable item ID.
    pub fn new(item_id: impl Into<SharedString>, display: impl Into<SharedString>) -> Self {
        Self {
            item_id: item_id.into(),
            display: display.into(),
            included: None,
        }
    }

    /// Creates an explicit included/not-included value without relying on an icon or color.
    pub fn included(item_id: impl Into<SharedString>, included: bool) -> Self {
        Self {
            item_id: item_id.into(),
            display: if included { "Included" } else { "Not included" }.into(),
            included: Some(included),
        }
    }

    /// Returns the stable item ID this value belongs to.
    pub fn item_id(&self) -> &str {
        &self.item_id
    }

    /// Returns the readable displayed value.
    pub fn display(&self) -> &str {
        &self.display
    }

    /// Returns explicit support state when this is an included/not-included value.
    pub fn is_included(&self) -> Option<bool> {
        self.included
    }
}

/// One feature row with values keyed by stable comparison item ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonFeature {
    id: SharedString,
    label: SharedString,
    description: Option<SharedString>,
    values: Arc<[ComparisonValue]>,
}

impl ComparisonFeature {
    /// Creates an empty feature row with stable identity and a visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            description: None,
            values: Arc::from([]),
        }
    }

    /// Adds readable supporting copy for the feature label.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Replaces the immutable item-value snapshot.
    pub fn values(mut self, values: impl IntoIterator<Item = ComparisonValue>) -> Self {
        self.values = values.into_iter().collect::<Vec<_>>().into();
        self
    }

    /// Returns the stable application ID.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the visible label.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns optional readable supporting copy.
    pub fn description_text(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the value for a stable item ID.
    pub fn value(&self, item_id: &str) -> Option<&ComparisonValue> {
        self.values.iter().find(|value| value.item_id == item_id)
    }

    /// Returns the immutable values in consumer order.
    pub fn values_snapshot(&self) -> &[ComparisonValue] {
        &self.values
    }
}

/// Structural validation failure for a bounded comparison snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonSnapshotError {
    /// More side-by-side items were supplied than the bounded layout supports.
    TooManyItems {
        /// The largest accepted item count.
        maximum: usize,
    },
    /// More feature rows were supplied than the bounded layout supports.
    TooManyFeatures {
        /// The largest accepted feature count.
        maximum: usize,
    },
    /// Two items reused one stable ID.
    DuplicateItemId(SharedString),
    /// Two features reused one stable ID.
    DuplicateFeatureId(SharedString),
    /// A feature supplied more than one value for the same stable item ID.
    DuplicateValueItemId {
        /// Stable feature ID containing the duplicate.
        feature_id: SharedString,
        /// Reused stable item ID.
        item_id: SharedString,
    },
    /// A feature value referenced an item absent from the snapshot.
    UnknownItemId {
        /// Stable feature ID containing the value.
        feature_id: SharedString,
        /// Unknown stable item ID.
        item_id: SharedString,
    },
}

/// Validated immutable data for one intentionally bounded comparison surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparisonSnapshot {
    items: Arc<[ComparisonItem]>,
    features: Arc<[ComparisonFeature]>,
}

impl ComparisonSnapshot {
    /// Validates and creates a bounded comparison snapshot.
    pub fn try_new(
        items: impl IntoIterator<Item = ComparisonItem>,
        features: impl IntoIterator<Item = ComparisonFeature>,
    ) -> Result<Self, ComparisonSnapshotError> {
        let items = items.into_iter().collect::<Vec<_>>();
        if items.len() > MAX_COMPARISON_ITEMS {
            return Err(ComparisonSnapshotError::TooManyItems {
                maximum: MAX_COMPARISON_ITEMS,
            });
        }
        let features = features.into_iter().collect::<Vec<_>>();
        if features.len() > MAX_COMPARISON_FEATURES {
            return Err(ComparisonSnapshotError::TooManyFeatures {
                maximum: MAX_COMPARISON_FEATURES,
            });
        }

        let mut item_ids = HashSet::new();
        for item in &items {
            if !item_ids.insert(item.id.clone()) {
                return Err(ComparisonSnapshotError::DuplicateItemId(item.id.clone()));
            }
        }

        let mut feature_ids = HashSet::new();
        for feature in &features {
            if !feature_ids.insert(feature.id.clone()) {
                return Err(ComparisonSnapshotError::DuplicateFeatureId(
                    feature.id.clone(),
                ));
            }
            let mut value_item_ids = HashSet::new();
            for value in feature.values.iter() {
                if !value_item_ids.insert(value.item_id.clone()) {
                    return Err(ComparisonSnapshotError::DuplicateValueItemId {
                        feature_id: feature.id.clone(),
                        item_id: value.item_id.clone(),
                    });
                }
                if !item_ids.contains(&value.item_id) {
                    return Err(ComparisonSnapshotError::UnknownItemId {
                        feature_id: feature.id.clone(),
                        item_id: value.item_id.clone(),
                    });
                }
            }
        }

        Ok(Self {
            items: items.into(),
            features: features.into(),
        })
    }

    /// Returns the immutable items in consumer order.
    pub fn items(&self) -> &[ComparisonItem] {
        &self.items
    }

    /// Returns the immutable features in consumer order.
    pub fn features(&self) -> &[ComparisonFeature] {
        &self.features
    }

    /// Finds a feature by stable application ID.
    pub fn feature(&self, feature_id: &str) -> Option<&ComparisonFeature> {
        self.features
            .iter()
            .find(|feature| feature.id == feature_id)
    }
}

/// A controlled, intentionally bounded feature-comparison surface.
///
/// The application owns snapshot progress, highlighted/disabled item state,
/// and selected item identity. The entity retains only focus and overflow
/// presentation state.
pub struct ComparisonTable {
    id: SharedString,
    label: SharedString,
    snapshot: Progressive<ComparisonSnapshot>,
    selected_item_id: Option<SharedString>,
    focused_item_id: Option<SharedString>,
    focus_handle: FocusHandle,
    focus_engaged: bool,
    horizontal_scroll: ScrollHandle,
    feature_scroll: ScrollHandle,
    _focus_subscriptions: Vec<gpui::Subscription>,
}

impl ComparisonTable {
    /// Creates an empty controlled comparison table with stable identity.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let empty = ComparisonSnapshot {
            items: Arc::from([]),
            features: Arc::from([]),
        };
        let focus_handle = cx.focus_handle();
        let focus_subscriptions = vec![
            cx.on_focus(&focus_handle, window, |this, _, cx| {
                this.focus_engaged = true;
                this.reveal_focused_item(cx);
            }),
            cx.on_blur(&focus_handle, window, |this, _, _| {
                this.focus_engaged = false;
            }),
        ];
        Self {
            id: id.into(),
            label: label.into(),
            snapshot: Progressive::pending(empty),
            selected_item_id: None,
            focused_item_id: None,
            focus_handle,
            focus_engaged: false,
            horizontal_scroll: ScrollHandle::new(),
            feature_scroll: ScrollHandle::new(),
            _focus_subscriptions: focus_subscriptions,
        }
    }

    /// Replaces the controlled progressive comparison snapshot.
    pub fn set_snapshot(
        &mut self,
        snapshot: Progressive<ComparisonSnapshot>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.snapshot = snapshot;
        if self.selected_item_id.as_ref().is_some_and(|selected| {
            !self
                .snapshot
                .content()
                .items
                .iter()
                .any(|item| item.id == *selected && item.state != ComparisonItemState::Disabled)
        }) {
            self.selected_item_id = None;
        }
        if self.focused_item_id.as_ref().is_none_or(|focused| {
            !self
                .snapshot
                .content()
                .items
                .iter()
                .any(|item| item.id == *focused && item.state != ComparisonItemState::Disabled)
        }) {
            self.focused_item_id = self
                .snapshot
                .content()
                .items
                .iter()
                .find(|item| item.state != ComparisonItemState::Disabled)
                .map(|item| item.id.clone());
        }
        if self.focus_engaged {
            self.reveal_focused_item(cx);
        }
        cx.notify();
    }

    /// Replaces the controlled selected item when the stable item is enabled.
    pub fn set_selected_item(
        &mut self,
        item_id: impl Into<SharedString>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let item_id = item_id.into();
        if !self
            .snapshot
            .content()
            .items
            .iter()
            .any(|item| item.id == item_id && item.state != ComparisonItemState::Disabled)
        {
            return;
        }
        self.selected_item_id = Some(item_id);
        cx.notify();
    }

    /// Clears the current controlled item selection without changing focus.
    pub fn clear_selected_item(&mut self, cx: &mut Context<Self>) {
        self.selected_item_id = None;
        cx.notify();
    }

    /// Returns the current controlled selected stable item ID.
    pub fn selected_item_id(&self) -> Option<&str> {
        self.selected_item_id.as_deref()
    }

    /// Returns the current progressive comparison snapshot.
    pub fn snapshot(&self) -> &Progressive<ComparisonSnapshot> {
        &self.snapshot
    }

    /// Moves keyboard focus to an enabled stable item and reveals its column.
    pub fn focus_item(&mut self, item_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self
            .snapshot
            .content()
            .items
            .iter()
            .position(|item| item.id == item_id && item.state != ComparisonItemState::Disabled)
        else {
            return;
        };
        self.focused_item_id = Some(self.snapshot.content().items[index].id.clone());
        self.focus_engaged = true;
        self.reveal_item(index, cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn request_selection(&self, item_id: SharedString, cx: &mut Context<Self>) {
        if self
            .snapshot
            .content()
            .items
            .iter()
            .any(|item| item.id == item_id && item.state != ComparisonItemState::Disabled)
        {
            cx.emit(ComparisonTableEvent::SelectionRequested {
                id: self.id.clone(),
                item_id,
            });
        }
    }

    fn move_item_focus(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self
            .snapshot
            .content()
            .items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.state != ComparisonItemState::Disabled)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        if enabled.is_empty() {
            return;
        }
        let current = self
            .focused_item_id
            .as_ref()
            .and_then(|focused| {
                self.snapshot
                    .content()
                    .items
                    .iter()
                    .position(|item| item.id == *focused)
            })
            .and_then(|index| enabled.iter().position(|enabled| *enabled == index))
            .unwrap_or_default();
        let next = (current as isize + delta).rem_euclid(enabled.len() as isize) as usize;
        let index = enabled[next];
        self.focused_item_id = Some(self.snapshot.content().items[index].id.clone());
        self.focus_engaged = true;
        self.reveal_item(index, cx);
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn reveal_item(&self, index: usize, cx: &App) {
        let (feature_width, item_width, _) = comparison_layout(cx);
        let current = self.horizontal_scroll.offset();
        self.horizontal_scroll.set_offset(gpui::point(
            -(feature_width + item_width * index as f32),
            current.y,
        ));
    }

    fn reveal_focused_item(&self, cx: &App) {
        let Some(index) =
            self.focused_item_id.as_ref().and_then(|focused| {
                self.snapshot.content().items.iter().position(|item| {
                    item.id == *focused && item.state != ComparisonItemState::Disabled
                })
            })
        else {
            return;
        };
        self.reveal_item(index, cx);
    }

    /// Scrolls the stable feature row into the vertical viewport.
    pub fn scroll_to_feature(&mut self, feature_id: &str, cx: &mut Context<Self>) {
        let Some(index) = self
            .snapshot
            .content()
            .features
            .iter()
            .position(|feature| feature.id == feature_id)
        else {
            return;
        };
        // gpui-component's scrollbar layer and the comparison header precede feature rows.
        self.feature_scroll.scroll_to_item(index.saturating_add(2));
        cx.notify();
    }
}

/// Typed application intent emitted by a comparison table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComparisonTableEvent {
    /// An enabled item was chosen through pointer, keyboard, or accessibility activation.
    SelectionRequested {
        /// Stable comparison table ID.
        id: SharedString,
        /// Stable comparison item ID.
        item_id: SharedString,
    },
}

impl EventEmitter<ComparisonTableEvent> for ComparisonTable {}

impl Focusable for ComparisonTable {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

fn comparison_table_frame(
    table_id: &str,
    label: impl Into<SharedString>,
) -> gpui::Stateful<gpui::Div> {
    let identity: SharedString = format!("comparison-table-root:{table_id}").into();
    div()
        .id(identity.clone())
        .debug_selector(move || identity.to_string())
        .role(Role::Table)
        .aria_label(label.into())
}

fn comparison_layout(cx: &App) -> (gpui::Pixels, gpui::Pixels, gpui::Pixels) {
    let spacing = cx.theme().semantic_tokens().spacing;
    (
        spacing.xxl * 6. + spacing.xl,
        spacing.xxl * 5. + spacing.lg,
        spacing.xxl * 2. + spacing.sm,
    )
}

fn comparison_item_header_frame(
    table_id: &str,
    item: &ComparisonItem,
    selected: bool,
) -> gpui::Stateful<gpui::Div> {
    let state_label = match item.state {
        ComparisonItemState::Default => None,
        ComparisonItemState::Highlighted => Some("Recommended"),
        ComparisonItemState::Disabled => Some("Unavailable"),
    };
    let accessible_label: SharedString = state_label
        .map(|state| format!("{}; {state}", item.label))
        .unwrap_or_else(|| item.label.to_string())
        .into();
    div()
        .id(format!("comparison-item-header:{table_id}:{}", item.id))
        .role(Role::ColumnHeader)
        .aria_label(accessible_label)
        .aria_selected(selected)
        .when_some(item.description.clone(), |header, description| {
            header.aria_description(description)
        })
}

fn comparison_item_control(
    table_id: &str,
    item: &ComparisonItem,
    cx: &mut App,
) -> gpui_base::Button {
    let debug_id: SharedString = format!("comparison-item-control:{table_id}:{}", item.id).into();
    outlined_control_with_label(
        debug_id.clone(),
        format!("Select {}", item.label),
        item.label.clone(),
        cx,
    )
    .debug_selector(move || debug_id.to_string())
    .disabled(item.state == ComparisonItemState::Disabled)
    .when_some(item.description.clone(), |button, description| {
        button.aria_description(description)
    })
}

fn comparison_status_frame(
    table_id: &str,
    role: Role,
    label: SharedString,
) -> gpui::Stateful<gpui::Div> {
    let identity = format!("comparison-table-status:{table_id}");
    div()
        .id(identity.clone())
        .debug_selector(move || identity.clone())
        .role(role)
        .aria_label(label)
}
fn comparison_feature_row_frame(
    table_id: &str,
    feature: &ComparisonFeature,
) -> gpui::Stateful<gpui::Div> {
    let identity = format!("comparison-feature:{table_id}:{}", feature.id);
    div()
        .id(identity.clone())
        .debug_selector(move || identity.clone())
        .role(Role::Row)
        .aria_label(feature.label.clone())
        .when_some(feature.description.clone(), |row, description| {
            row.aria_description(description)
        })
}

fn comparison_feature_header_frame(
    table_id: &str,
    feature: &ComparisonFeature,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(format!(
            "comparison-feature-label:{table_id}:{}",
            feature.id
        ))
        .role(Role::RowHeader)
        .aria_label(feature.label.clone())
        .when_some(feature.description.clone(), |header, description| {
            header.aria_description(description)
        })
}

fn comparison_cell_frame(
    table_id: &str,
    feature_id: &str,
    item: &ComparisonItem,
    display: SharedString,
) -> gpui::Stateful<gpui::Div> {
    let identity = format!("comparison-cell:{table_id}:{feature_id}:{}", item.id);
    div()
        .id(identity.clone())
        .debug_selector(move || identity.clone())
        .role(Role::Cell)
        .aria_label(format!("{}: {display}", item.label))
        .aria_value(display)
}

impl Render for ComparisonTable {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let owner = cx.weak_entity();
        let navigation_owner = owner.clone();
        let items = self.snapshot.content().items.clone();
        let features = self.snapshot.content().features.clone();
        let (feature_width, item_width, header_height) = comparison_layout(cx);
        let status = match self.snapshot.state() {
            ProgressState::Pending => Some((Role::ProgressIndicator, "Loading comparison".into())),
            ProgressState::Running => Some((Role::ProgressIndicator, "Updating comparison".into())),
            ProgressState::Failed(reason) => Some((Role::Alert, reason.clone())),
            ProgressState::Complete if items.is_empty() || features.is_empty() => {
                Some((Role::Status, "No comparison data".into()))
            }
            ProgressState::Complete => None,
        };

        comparison_table_frame(&self.id, self.label.clone())
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            // Native scroll chaining: while either axis of the table has
            // room in the wheel's direction, the wheel belongs to the table.
            .on_scroll_wheel({
                let horizontal = self.horizontal_scroll.clone();
                let vertical = self.feature_scroll.clone();
                move |event: &ScrollWheelEvent, _, cx| {
                    let delta = event.delta.pixel_delta(px(20.));
                    let vertical_room = ScrollRoom::from_handle(&vertical).can_absorb(delta.y);
                    let horizontal_room = ScrollRoom::from_handle(&horizontal).can_absorb(delta.x);
                    if vertical_room || horizontal_room {
                        cx.stop_propagation();
                    }
                }
            })
            .overflow_x_scroll()
            .track_scroll(&self.horizontal_scroll)
            .horizontal_scrollbar(&self.horizontal_scroll)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(tokens.radius.md)
            .when_some(status, |surface, (role, label): (Role, SharedString)| {
                // Status states carry the same semantic color language as
                // Task Rows: info for in-flight work, danger for failures,
                // muted for empty. A spinner accompanies loading so the
                // state is visible, not just readable.
                fn status_visuals(role: Role, cx: &App) -> (Hsla, IconName) {
                    match role {
                        Role::ProgressIndicator => (cx.theme().info, IconName::LoaderCircle),
                        Role::Alert => (cx.theme().danger, IconName::CircleX),
                        _ => (cx.theme().muted_foreground, IconName::Dash),
                    }
                }
                let spinner = role == Role::ProgressIndicator;
                let (color, text) = (status_visuals(role, cx).0, label.clone());
                surface.child(
                    comparison_status_frame(&self.id, role, label.clone())
                        .p(tokens.spacing.md)
                        .child(
                            h_flex()
                                .gap(tokens.spacing.sm)
                                .items_center()
                                .when(spinner, |row| {
                                    // The animated element must be the direct
                                    // child; wrap the rotating icon in a
                                    // fixed-size slot so layout stays stable.
                                    let icon = status_visuals(role, cx).1;
                                    row.child(
                                        div().size_4().child(
                                            Icon::new(icon)
                                                .text_color(color)
                                                .with_animation(
                                                    "comparison-status-spinner",
                                                    Animation::new(Duration::from_millis(900))
                                                        .repeat(),
                                                    |this, delta| {
                                                        this.rotate(gpui::percentage(delta))
                                                    },
                                                )
                                                .into_any_element(),
                                        ),
                                    )
                                })
                                .when(!spinner, |row| {
                                    let icon = status_visuals(role, cx).1;
                                    row.child(Icon::new(icon).size_4().text_color(color))
                                })
                                .child(
                                    div()
                                        .text_token(tokens.typography.sm)
                                        .text_color(color)
                                        .child(text),
                                ),
                        ),
                )
            })
            .child(
                div()
                    .id(format!("comparison-table-rows:{}", self.id))
                    .w_full()
                    .flex_1()
                    .min_h_0()
                    .min_w(feature_width + item_width * items.len() as f32)
                    .overflow_y_scroll()
                    .track_scroll(&self.feature_scroll)
                    .vertical_scrollbar(&self.feature_scroll)
                    .role(Role::RowGroup)
                    .child(
                        div()
                            .id(format!("comparison-table-header-row:{}", self.id))
                            .flex()
                            .role(Role::Row)
                            .child(
                                div()
                                    .id(format!("comparison-feature-header:{}", self.id))
                                    .w(feature_width)
                                    .flex_none()
                                    .flex()
                                    .flex_col()
                                    .gap(tokens.spacing.xxs)
                                    .p(tokens.spacing.sm)
                                    .role(Role::ColumnHeader)
                                    .aria_label("Feature"),
                            )
                            .children(items.iter().map(|item| {
                                let item_id = item.id.clone();
                                let handler_owner = owner.clone();
                                let selected = self.selected_item_id.as_ref() == Some(&item.id);
                                let focused = self.focused_item_id.as_ref() == Some(&item.id);
                                let state_label = match item.state {
                                    ComparisonItemState::Default => None,
                                    ComparisonItemState::Highlighted => Some("Recommended"),
                                    ComparisonItemState::Disabled => Some("Unavailable"),
                                };
                                comparison_item_header_frame(&self.id, item, selected)
                                    .w(item_width)
                                    .min_h(header_height)
                                    .flex_none()
                                    .p(tokens.spacing.sm)
                                    .border_l_1()
                                    .border_color(cx.theme().border)
                                    .when(
                                        item.state == ComparisonItemState::Highlighted,
                                        |header| header.bg(cx.theme().accent),
                                    )
                                    .when(selected, |header| {
                                        header.border_2().border_color(cx.theme().ring)
                                    })
                                    .child(
                                        comparison_item_control(&self.id, item, cx)
                                            .w_full()
                                            .tab_stop(focused)
                                            .when(focused, |button| {
                                                button.track_focus(&self.focus_handle)
                                            })
                                            .on_click(move |_, window, cx| {
                                                let _ = handler_owner.update(cx, |table, cx| {
                                                    table.focus_item(&item_id, window, cx);
                                                    table.request_selection(item_id.clone(), cx);
                                                });
                                            }),
                                    )
                                    .when_some(item.description.clone(), |header, description| {
                                        header.child(
                                            TextView::markdown(
                                                format!(
                                                    "comparison-item-description:{}:{}",
                                                    self.id, item.id
                                                ),
                                                escape_markdown_text(&description),
                                            )
                                            .selectable(true),
                                        )
                                    })
                                    .when_some(state_label, |header, state| {
                                        header.child(
                                            div()
                                                .mt(tokens.spacing.xs)
                                                .text_token(tokens.typography.xs)
                                                .text_color(cx.theme().muted_foreground)
                                                .child(state),
                                        )
                                    })
                                    .when(selected, |header| {
                                        header.child(
                                            div()
                                                .mt(tokens.spacing.xs)
                                                .text_token(tokens.typography.xs)
                                                .child("Selected"),
                                        )
                                    })
                            }))
                            .on_key_down(move |event, window, cx| {
                                let delta = match event.keystroke.key.as_str() {
                                    "left" => -1,
                                    "right" => 1,
                                    _ => return,
                                };
                                let _ = navigation_owner.update(cx, |table, cx| {
                                    table.move_item_focus(delta, window, cx);
                                });
                                cx.stop_propagation();
                            }),
                    )
                    .children(features.iter().map(|feature| {
                        comparison_feature_row_frame(&self.id, feature)
                            .flex()
                            .border_t_1()
                            .border_color(cx.theme().border)
                            .child(
                                comparison_feature_header_frame(&self.id, feature)
                                    .w(feature_width)
                                    .flex_none()
                                    .p(tokens.spacing.sm)
                                    .child(
                                        TextView::markdown(
                                            format!(
                                                "comparison-feature-copy:{}:{}",
                                                self.id, feature.id
                                            ),
                                            escape_markdown_text(&feature.label),
                                        )
                                        .selectable(true),
                                    )
                                    .when_some(
                                        feature.description.clone(),
                                        |header, description| {
                                            header.child(
                                                TextView::markdown(
                                                    format!(
                                                        "comparison-feature-description:{}:{}",
                                                        self.id, feature.id
                                                    ),
                                                    escape_markdown_text(&description),
                                                )
                                                .selectable(true),
                                            )
                                        },
                                    ),
                            )
                            .children(items.iter().map(|item| {
                                let value = feature.value(&item.id);
                                let display: SharedString = value
                                    .map(|value| value.display.clone())
                                    .unwrap_or_else(|| "Not specified".into());
                                comparison_cell_frame(&self.id, &feature.id, item, display.clone())
                                    .w(item_width)
                                    .flex_none()
                                    .p(tokens.spacing.sm)
                                    .border_l_1()
                                    .border_color(cx.theme().border)
                                    .when(item.state == ComparisonItemState::Highlighted, |cell| {
                                        cell.bg(cx.theme().accent)
                                    })
                                    .child(
                                        TextView::markdown(
                                            format!(
                                                "comparison-cell-copy:{}:{}:{}",
                                                self.id, feature.id, item.id
                                            ),
                                            escape_markdown_text(&display),
                                        )
                                        .selectable(true),
                                    )
                            }))
                    })),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Element as _, RenderOnce as _, TestAppContext, accesskit, canvas};
    use std::sync::{Arc, Mutex};

    type CapturedNodes = Arc<Mutex<Option<[(Option<Role>, accesskit::Node); 9]>>>;

    struct SemanticsProbe {
        captured: CapturedNodes,
    }

    impl Render for SemanticsProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let root = comparison_table_frame("plans", "Plan comparison").into_element();
                    let business = ComparisonItem::new("business", "Business")
                        .description("Best for growing teams")
                        .state(ComparisonItemState::Highlighted);
                    let header =
                        comparison_item_header_frame("plans", &business, true).into_element();
                    let enabled_control = comparison_item_control("plans", &business, cx)
                        .on_click(|_, _, _| {})
                        .render(window, cx)
                        .into_element();
                    let control = comparison_item_control(
                        "plans",
                        &ComparisonItem::new("legacy", "Legacy")
                            .state(ComparisonItemState::Disabled),
                        cx,
                    )
                    .on_click(|_, _, _| {})
                    .render(window, cx)
                    .into_element();
                    let disabled_header = comparison_item_header_frame(
                        "plans",
                        &ComparisonItem::new("legacy", "Legacy")
                            .state(ComparisonItemState::Disabled),
                        false,
                    )
                    .into_element();
                    let feature = ComparisonFeature::new("support", "Priority support")
                        .description("Response within four hours");
                    let row = comparison_feature_row_frame("plans", &feature).into_element();
                    let row_header =
                        comparison_feature_header_frame("plans", &feature).into_element();
                    let cell =
                        comparison_cell_frame("plans", "support", &business, "Included".into())
                            .into_element();
                    let status = comparison_status_frame(
                        "plans",
                        Role::ProgressIndicator,
                        "Loading comparison".into(),
                    )
                    .into_element();
                    let mut root_node = accesskit::Node::new(Role::Unknown);
                    root.write_a11y_info(&mut root_node);
                    let mut header_node = accesskit::Node::new(Role::Unknown);
                    header.write_a11y_info(&mut header_node);
                    let mut control_node = accesskit::Node::new(Role::Unknown);
                    control.write_a11y_info(&mut control_node);
                    let mut disabled_header_node = accesskit::Node::new(Role::Unknown);
                    disabled_header.write_a11y_info(&mut disabled_header_node);
                    let mut enabled_control_node = accesskit::Node::new(Role::Unknown);
                    enabled_control.write_a11y_info(&mut enabled_control_node);
                    let mut row_node = accesskit::Node::new(Role::Unknown);
                    row.write_a11y_info(&mut row_node);
                    let mut row_header_node = accesskit::Node::new(Role::Unknown);
                    row_header.write_a11y_info(&mut row_header_node);
                    let mut cell_node = accesskit::Node::new(Role::Unknown);
                    cell.write_a11y_info(&mut cell_node);
                    let mut status_node = accesskit::Node::new(Role::Unknown);
                    status.write_a11y_info(&mut status_node);
                    *captured.lock().expect("capture mutex should be available") = Some([
                        (root.a11y_role(), root_node),
                        (header.a11y_role(), header_node),
                        (control.a11y_role(), control_node),
                        (disabled_header.a11y_role(), disabled_header_node),
                        (enabled_control.a11y_role(), enabled_control_node),
                        (row.a11y_role(), row_node),
                        (row_header.a11y_role(), row_header_node),
                        (cell.a11y_role(), cell_node),
                        (status.a11y_role(), status_node),
                    ]);
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn comparison_semantics_name_table_selected_highlight_and_disabled_action(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| SemanticsProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let [
            root,
            header,
            disabled,
            disabled_header,
            enabled,
            row,
            row_header,
            cell,
            status,
        ] = result
            .lock()
            .expect("capture mutex should be available")
            .take()
            .expect("semantic nodes should be captured");

        assert_eq!(root.0, Some(Role::Table));
        assert_eq!(root.1.label(), Some("Plan comparison"));
        assert_eq!(header.0, Some(Role::ColumnHeader));
        assert_eq!(header.1.label(), Some("Business; Recommended"));
        assert_eq!(header.1.description(), Some("Best for growing teams"));
        assert_eq!(header.1.is_selected(), Some(true));
        assert_eq!(disabled.0, Some(Role::Button));
        assert!(!disabled.1.supports_action(accesskit::Action::Click));
        assert_eq!(disabled_header.1.label(), Some("Legacy; Unavailable"));
        assert_eq!(enabled.0, Some(Role::Button));
        assert_eq!(enabled.1.label(), Some("Select Business"));
        assert_eq!(enabled.1.description(), Some("Best for growing teams"));
        assert!(enabled.1.supports_action(accesskit::Action::Click));
        assert_eq!(row.0, Some(Role::Row));
        assert_eq!(row.1.label(), Some("Priority support"));
        assert_eq!(row.1.description(), Some("Response within four hours"));
        assert_eq!(row_header.0, Some(Role::RowHeader));
        assert_eq!(row_header.1.label(), Some("Priority support"));
        assert_eq!(cell.0, Some(Role::Cell));
        assert_eq!(cell.1.label(), Some("Business: Included"));
        assert_eq!(cell.1.value(), Some("Included"));
        assert_eq!(status.0, Some(Role::ProgressIndicator));
        assert_eq!(status.1.label(), Some("Loading comparison"));
    }
}
