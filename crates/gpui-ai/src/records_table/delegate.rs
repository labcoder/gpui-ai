//! The table delegate: one accepted snapshot rendered as headers, rows, cells,
//! and progressive states.
//!
//! Upstream's table drives everything here — it asks for counts, for a column,
//! for the element at a coordinate, and it owns virtualization. Each answer is
//! read from the snapshot the owning entity handed over; nothing in this file
//! decides controlled state. Application intent goes back to the entity, which
//! is the only thing that may change it.

use std::{collections::HashSet, sync::Arc};

use gpui::{
    App, Context, Div, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels, Role,
    SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Size,
    table::{Column, TableDelegate, TableState},
    text::TextView,
};

use crate::{
    control::{composed_button, outlined_control_with_label},
    stream::{ProgressState, Progressive},
    theme::SemanticStyledExt as _,
};

use super::{
    RecordCell, RecordCellKind, RecordCellProvider, RecordColumn, RecordColumnAlignment, RecordRow,
    RecordSortDirection, RecordStatusTone, RecordsTable, RowActionPlacement, RowActionVisibility,
    escape_markdown_text, records_state_glyph, records_state_text, reorder::RowReorderState,
    scoped_records_id,
};

#[derive(Clone)]
pub(super) struct RecordsDelegate {
    owner: WeakEntity<RecordsTable>,
    component_id: SharedString,
    pub(super) columns: Arc<[RecordColumn]>,
    pub(super) records: Progressive<Arc<[RecordRow]>>,
    pub(super) cell_provider: Option<Arc<dyn RecordCellProvider>>,
    pub(super) selected_row_id: Option<SharedString>,
    pub(super) sort_column_id: Option<SharedString>,
    pub(super) sort_direction: Option<RecordSortDirection>,
    pub(super) activation_label: SharedString,
    pub(super) row_reorder: RowReorderState,
    pub(super) row_action_visibility: RowActionVisibility,
    pub(super) row_action_placement: RowActionPlacement,
    /// The rem the owning table last resolved, so a rem-scaled column width
    /// lands in the same type scale as the text inside it. Upstream caches
    /// column widths in pixels, so this only reaches layout through a refresh.
    pub(super) rem_size: Pixels,
}

impl RecordsDelegate {
    pub(super) fn empty(
        owner: WeakEntity<RecordsTable>,
        component_id: SharedString,
        rem_size: Pixels,
    ) -> Self {
        Self {
            owner,
            component_id,
            columns: Arc::from([]),
            records: Progressive::pending(Arc::from([])),
            cell_provider: None,
            selected_row_id: None,
            sort_column_id: None,
            sort_direction: None,
            activation_label: "Open".into(),
            row_reorder: RowReorderState::default(),
            row_action_visibility: RowActionVisibility::default(),
            row_action_placement: RowActionPlacement::default(),
            rem_size,
        }
    }

    fn row(&self, row_ix: usize) -> Option<&RecordRow> {
        self.records.content().get(row_ix)
    }

    fn record_column(&self, col_ix: usize) -> Option<&RecordColumn> {
        self.columns.get(col_ix)
    }
}

impl TableDelegate for RecordsDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.records.content().len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.record_column(col_ix)
            .map(|column| {
                let alignment = column.alignment;
                let fixed = column.fixed;
                let width = column.width.resolve(self.rem_size);
                let column = Column::new(column.id.clone(), column.label.clone());
                let column = column.when_some(width, |column, width| column.width(width));
                let column = if fixed { column.fixed_left() } else { column };
                match alignment {
                    RecordColumnAlignment::Left => column,
                    RecordColumnAlignment::Center => column.text_center(),
                    RecordColumnAlignment::Right => column.text_right(),
                }
            })
            .unwrap_or_default()
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        if let Some(row) = self.row(row_ix) {
            self.row_reorder.note_visible(row.id.clone());
        }
        if !self.row_reorder.is_enabled() {
            let Some(row) = self.row(row_ix) else {
                return div().id(("records-placeholder-row", row_ix));
            };
            let owner = self.owner.clone();
            let pointer_row_id = row.id.clone();
            return record_row_frame(
                &self.component_id,
                row,
                self.selected_row_id.as_ref() == Some(&row.id),
            )
            .on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
                let _ = owner.update(cx, |table, _| {
                    table.pending_pointer_row_id = Some(pointer_row_id.clone());
                });
            });
        }
        let Some(row) = self.row(row_ix).cloned() else {
            return div().id(("records-placeholder-row", row_ix));
        };
        let owner = self.owner.clone();
        let pointer_row_id = row.id.clone();
        let row_frame = record_row_frame(
            &self.component_id,
            &row,
            self.selected_row_id.as_ref() == Some(&row.id),
        );
        let row_frame = match self
            .row_reorder
            .sample(&self.component_id, &row.id, window, cx)
        {
            Some(offset) => row_frame.relative().top(offset),
            None => row_frame,
        };
        row_frame.on_mouse_down(gpui::MouseButton::Left, move |_, _, cx| {
            let _ = owner.update(cx, |table, _| {
                table.pending_pointer_row_id = Some(pointer_row_id.clone());
            });
        })
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl gpui::IntoElement {
        let column = self.record_column(col_ix);
        let label = column
            .map(|column| column.label.clone())
            .unwrap_or_default();
        let id = column
            .map(|column| column.id.clone())
            .unwrap_or_else(|| SharedString::from(format!("missing-{col_ix}")));

        let sortable = column.is_some_and(|column| column.sortable);
        let sort_description = if self.sort_column_id.as_ref() == Some(&id) {
            match self.sort_direction {
                Some(RecordSortDirection::Ascending) => ", ascending",
                Some(RecordSortDirection::Descending) => ", descending",
                None => ", unsorted",
            }
        } else {
            ", unsorted"
        };
        let sort_marker = if self.sort_column_id.as_ref() == Some(&id) {
            match self.sort_direction {
                Some(RecordSortDirection::Ascending) => "↑",
                Some(RecordSortDirection::Descending) => "↓",
                None => "↕",
            }
        } else {
            "↕"
        };
        let owner = self.owner.clone();
        let sort_column_id = id.clone();
        let content = if sortable {
            record_sort_button(
                &self.component_id,
                &id,
                label.clone(),
                sort_description,
                sort_marker,
                cx,
            )
            .on_click(move |_, _, cx| {
                let _ = owner.update(cx, |table, cx| {
                    table.request_sort(sort_column_id.clone(), cx);
                });
            })
            .into_any_element()
        } else {
            div().size_full().child(label.clone()).into_any_element()
        };

        let debug_column_id = id.clone();
        div()
            .id(scoped_records_id("column", &self.component_id, &id))
            .debug_selector({
                let component_id = self.component_id.clone();
                move || scoped_records_id("column", &component_id, &debug_column_id)
            })
            .size_full()
            .role(Role::ColumnHeader)
            .aria_label(label.clone())
            .when_some(
                column.and_then(|column| column.description.clone()),
                |this, description| this.aria_description(description),
            )
            .child(content)
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl gpui::IntoElement {
        let row = self.row(row_ix);
        let column = self.record_column(col_ix);
        let cell = row
            .zip(column)
            .and_then(|(row, column)| row.cell(column.id()))
            .cloned()
            .or_else(|| {
                column.and_then(|column| {
                    self.cell_provider
                        .as_ref()
                        .and_then(|provider| provider.cell(row_ix, column.id()))
                })
            });
        let value = record_cell_accessible_value(
            cell.as_ref(),
            row,
            col_ix,
            self.activation_label.as_ref(),
        );
        let row_id = row
            .map(|row| row.id.clone())
            .unwrap_or_else(|| SharedString::from(format!("missing-{row_ix}")));
        let column_id = column
            .map(|column| column.id.clone())
            .unwrap_or_else(|| SharedString::from(format!("missing-{col_ix}")));

        let identity = format!("{}:{row_id}{column_id}", row_id.len());
        let scoped_identity = scoped_records_id("cell", &self.component_id, &identity);

        let content = cell
            .as_ref()
            .map(|cell| record_cell_content(&scoped_identity, cell, window, cx))
            .unwrap_or_else(|| div().into_any_element());
        let content = if col_ix == 0 {
            if let Some(row) = row {
                let owner = self.owner.clone();
                let row_id = row.id.clone();
                let activation = record_activation_button(
                    &self.component_id,
                    &self.activation_label,
                    row,
                    window,
                    cx,
                )
                .on_click(move |_, _, cx| {
                    let _ = owner.update(cx, |table, cx| {
                        table.request_activation(row_id.clone(), cx);
                    });
                    cx.stop_propagation();
                });
                let activation = match self.row_action_visibility {
                    RowActionVisibility::Always => activation,
                    // Revealed by the row's hover group, and by the
                    // control's own keyboard focus, so tab reaches a
                    // visible control regardless of the pointer.
                    RowActionVisibility::OnHover => activation
                        .opacity(0.)
                        .group_hover(row_hover_group(&self.component_id, &row.id), |style| {
                            style.opacity(1.)
                        })
                        .focus_visible(|style| style.opacity(1.)),
                };
                match self.row_action_placement {
                    RowActionPlacement::End => div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(div().flex_1().min_w_0().overflow_hidden().child(content))
                        .child(activation)
                        .into_any_element(),
                    RowActionPlacement::Inline => div()
                        .size_full()
                        .flex()
                        .items_center()
                        .gap(cx.theme().semantic_tokens().spacing.sm)
                        .child(div().min_w_0().overflow_hidden().child(content))
                        .child(activation)
                        .into_any_element(),
                }
            } else {
                content
            }
        } else {
            content
        };
        record_cell_frame(scoped_identity, value).child(content)
    }

    fn render_empty(
        &mut self,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl gpui::IntoElement {
        let (id, role, label) = match self.records.state() {
            ProgressState::Failed(reason) => (
                "records-error",
                Role::Alert,
                SharedString::from(format!("Records unavailable: {reason}")),
            ),
            _ => (
                "records-empty",
                Role::Status,
                SharedString::from("No records"),
            ),
        };

        records_state_frame(&self.component_id, id, role, label).text_color(match role {
            Role::Alert => cx.theme().danger,
            _ => cx.theme().muted_foreground,
        })
    }

    fn render_loading(
        &mut self,
        _: Size,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl gpui::IntoElement {
        records_state_frame(
            &self.component_id,
            "records-loading",
            Role::ProgressIndicator,
            "Loading records".into(),
        )
        .text_color(cx.theme().muted_foreground)
    }

    fn loading(&self, _: &App) -> bool {
        self.records.content().is_empty()
            && matches!(
                self.records.state(),
                ProgressState::Pending | ProgressState::Running
            )
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        let Some(column) = self.record_column(col_ix) else {
            return String::new();
        };
        self.row(row_ix)
            .and_then(|row| row.cell(column.id()))
            .map(|cell| cell.value().to_owned())
            .or_else(|| {
                self.cell_provider
                    .as_ref()
                    .and_then(|provider| provider.cell(row_ix, column.id()))
                    .map(|cell| cell.value().to_owned())
            })
            .unwrap_or_default()
    }

    fn visible_rows_changed(
        &mut self,
        visible_range: std::ops::Range<usize>,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let anchor = self.row(visible_range.start).map(|row| row.id.clone());
        let visible_row_ids: HashSet<_> = self
            .records
            .content()
            .get(visible_range)
            .unwrap_or_default()
            .iter()
            .map(|row| row.id.clone())
            .collect();
        // A virtualized row that leaves the rendered window will no longer be
        // sampled, so it cannot settle itself. The reorder lifecycle owns the
        // same membership whether animation is enabled or not.
        self.row_reorder.set_visible(visible_row_ids);
        let _ = self.owner.update(cx, |table, _| {
            table.viewport_row_anchor_id = anchor;
        });
    }

    fn visible_columns_changed(
        &mut self,
        visible_range: std::ops::Range<usize>,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let fixed_columns = self.columns.iter().filter(|column| column.fixed).count();
        let anchor = self
            .record_column(visible_range.start.saturating_add(fixed_columns))
            .map(|column| column.id.clone());
        let _ = self.owner.update(cx, |table, _| {
            table.viewport_column_anchor_id = anchor;
        });
    }
}

pub(super) fn record_sort_button(
    component_id: &str,
    column_id: &str,
    label: SharedString,
    sort_description: &str,
    sort_marker: &'static str,
    cx: &mut App,
) -> gpui_base::Button {
    let debug_id = scoped_records_id("sort", component_id, column_id);
    composed_button(
        debug_id.clone(),
        format!("Sort by {label}{sort_description}"),
    )
    .size_full()
    .justify_between()
    .border_1()
    .border_color(cx.theme().transparent)
    .focus_visible(|style| style.border_color(cx.theme().ring))
    .debug_selector(move || debug_id.clone())
    .child(label)
    .child(sort_marker)
}

pub(super) fn record_activation_button(
    component_id: &str,
    activation_label: &str,
    row: &RecordRow,
    window: &mut Window,
    cx: &mut App,
) -> gpui_base::Button {
    let debug_id = scoped_records_id("activate", component_id, &row.id);
    let label = if row.disabled {
        format!("Unavailable: {activation_label} {}", row.label)
    } else {
        format!("{activation_label} {}", row.label)
    };
    outlined_control_with_label(
        debug_id.clone(),
        label,
        activation_label.to_owned(),
        window,
        cx,
    )
    .debug_selector(move || debug_id.clone())
    .flex_none()
    .disabled(row.disabled)
}

pub(super) fn records_state_frame(
    component_id: &str,
    id: &str,
    role: Role,
    label: SharedString,
) -> Stateful<Div> {
    let scoped_id = scoped_records_id("state", component_id, id);
    div()
        .id(scoped_id.clone())
        .debug_selector({
            let scoped_id = scoped_id.clone();
            move || scoped_id.clone()
        })
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(gpui::rems(0.5))
        .role(role)
        .aria_label(label.clone())
        .child(records_state_glyph(role))
        .child(records_state_text(&scoped_id, role, label))
}

pub(super) fn row_hover_group(component_id: &str, row_id: &str) -> SharedString {
    SharedString::from(format!("records-row-group:{component_id}:{row_id}"))
}

pub(super) fn record_row_frame(
    component_id: &str,
    row: &RecordRow,
    selected: bool,
) -> Stateful<Div> {
    let debug_row_id = row.id.clone();
    let component_id = SharedString::from(component_id);
    let hover_group = row_hover_group(&component_id, &row.id);
    div()
        .id(scoped_records_id("row", &component_id, &row.id))
        .debug_selector(move || scoped_records_id("row", &component_id, &debug_row_id))
        .group(hover_group)
        .role(Role::Row)
        .aria_label(row.label.clone())
        .aria_selected(selected)
        .when(row.disabled, |this| {
            this.aria_description("Unavailable record")
                .aria_value("Disabled")
        })
}

pub(super) fn record_cell_frame(identity: impl Into<String>, value: SharedString) -> Stateful<Div> {
    let identity = identity.into();
    let debug_identity = identity.clone();
    div()
        .id(identity)
        .debug_selector(move || debug_identity.clone())
        .size_full()
        .role(Role::Cell)
        .aria_label(value.clone())
        .aria_value(value.clone())
}

pub(super) fn record_cell_accessible_value(
    cell: Option<&RecordCell>,
    row: Option<&RecordRow>,
    col_ix: usize,
    activation_label: &str,
) -> SharedString {
    cell.map(|cell| cell.value.clone())
        .or_else(|| {
            (col_ix == 0).then(|| {
                row.map(|row| format!("{activation_label} {}", row.label))
                    .unwrap_or_default()
                    .into()
            })
        })
        .unwrap_or_default()
}

fn record_cell_content(
    identity: &str,
    cell: &RecordCell,
    window: &mut Window,
    cx: &mut App,
) -> gpui::AnyElement {
    let tokens = cx.theme().semantic_tokens();
    match cell.kind {
        RecordCellKind::Text => TextView::markdown(
            format!("records-cell-text-{identity}"),
            escape_markdown_text(cell.value()),
        )
        .selectable(true)
        .into_any_element(),
        RecordCellKind::Tags => div()
            .flex()
            .flex_wrap()
            .gap(tokens.spacing.xxs)
            .children(cell.tags.iter().enumerate().map(|(index, tag)| {
                div()
                    .px(tokens.spacing.xs)
                    .py(tokens.spacing.xxs)
                    .rounded(tokens.radius.full)
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().secondary)
                    .text_token(tokens.typography.xs)
                    .child(
                        TextView::markdown(
                            format!("records-tag-{identity}-{index}"),
                            escape_markdown_text(tag.as_ref()),
                        )
                        .selectable(true),
                    )
            }))
            .into_any_element(),
        RecordCellKind::Status => {
            let tone = match cell.status_tone.unwrap_or(RecordStatusTone::Neutral) {
                RecordStatusTone::Positive => cx.theme().success,
                RecordStatusTone::Neutral => cx.theme().muted_foreground,
                RecordStatusTone::Caution => cx.theme().warning,
                RecordStatusTone::Critical => cx.theme().danger,
            };
            // The status a cell is constructed with is presented settled; a
            // status changed in place — a proposal decided, a run finishing
            // — fades its new face in once at the quick tempo. The ordinal
            // is a hash of the value, so any change plays exactly once and
            // a colliding pair merely skips its acknowledgment.
            let ordinal = cell
                .value()
                .bytes()
                .fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
                    (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
                });
            let acknowledged = crate::motion::acknowledged_state(
                gpui::ElementId::Name(format!("records-status-ack-{identity}").into()),
                ordinal,
                window,
                cx,
            );
            div()
                .flex()
                .items_center()
                .gap(tokens.spacing.xs)
                .opacity(acknowledged)
                .child(
                    div()
                        .size(tokens.spacing.xs)
                        .rounded(tokens.radius.full)
                        .bg(tone),
                )
                .child(
                    TextView::markdown(
                        format!("records-status-{identity}"),
                        escape_markdown_text(cell.value()),
                    )
                    .selectable(true),
                )
                .into_any_element()
        }
    }
}
