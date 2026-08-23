//! Controlled record-grid values and presentation.

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};

use gpui::{
    AnyElement, App, AppContext as _, Context, Div, EventEmitter, FocusHandle, Focusable,
    InteractiveElement as _, IntoElement as _, KeyBinding, ParentElement as _, Pixels, Render,
    Role, SharedString, Stateful, StatefulInteractiveElement as _, Styled as _, Subscription,
    WeakEntity, Window, div, prelude::FluentBuilder as _,
};
use gpui_base::motion::{Transition, transition};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, Size,
    spinner::Spinner,
    table::{Column, DataTable, TableDelegate, TableEvent, TableState},
    text::TextView,
};

use crate::{
    control::{composed_button, outlined_control_with_label},
    motion::Shimmer,
    stream::{ProgressState, Progressive},
    theme::SemanticStyledExt as _,
};

const RECORDS_TABLE_CONTEXT: &str = "GpuiAiRecordsTable";
gpui::actions!(
    gpui_ai_records_table,
    [
        /// Activates the consumer-controlled selected record.
        ActivateRecord
    ]
);

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", ActivateRecord, Some(RECORDS_TABLE_CONTEXT)),
        KeyBinding::new("space", ActivateRecord, Some(RECORDS_TABLE_CONTEXT)),
    ]);
}

/// Sort direction requested for a record column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordSortDirection {
    /// Sort values from low to high.
    Ascending,
    /// Sort values from high to low.
    Descending,
}

/// Horizontal alignment for one records column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordColumnAlignment {
    /// Align content to the leading edge.
    #[default]
    Left,
    /// Center content in the column.
    Center,
    /// Align content to the trailing edge.
    Right,
}

/// Visual structure of one record cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecordCellKind {
    /// Ordinary readable text.
    #[default]
    Text,
    /// A set of compact categorical tags.
    Tags,
    /// A named semantic status.
    Status,
}

/// Semantic emphasis for a status cell.
///
/// The readable status text always carries meaning; tone only reinforces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordStatusTone {
    /// Positive or healthy status.
    Positive,
    /// Informational status without positive or negative meaning.
    Neutral,
    /// Status that deserves attention.
    Caution,
    /// Critical or failed status.
    Critical,
}

/// Typed application intent emitted by [`RecordsTable`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecordsTableEvent {
    /// Requests that the application select the identified row.
    SelectionRequested {
        /// Stable records-table ID.
        id: SharedString,
        /// Stable application row ID.
        row_id: SharedString,
    },
    /// Requests the row's primary application action.
    ActivationRequested {
        /// Stable records-table ID.
        id: SharedString,
        /// Stable application row ID.
        row_id: SharedString,
    },
    /// Requests a controlled sort projection.
    SortRequested {
        /// Stable records-table ID.
        id: SharedString,
        /// Stable application column ID.
        column_id: SharedString,
        /// Requested direction, or `None` to clear sorting.
        direction: Option<RecordSortDirection>,
    },
}

/// One stable column in a [`RecordsTable`].
///
/// Visible labels need not be unique. Events and row lookup always use the
/// application-supplied column ID.
#[derive(Debug, Clone, PartialEq)]
pub struct RecordColumn {
    id: SharedString,
    label: SharedString,
    sortable: bool,
    width: Option<Pixels>,
    alignment: RecordColumnAlignment,
    fixed: bool,
    description: Option<SharedString>,
}

impl RecordColumn {
    /// Creates a column with a stable application ID and visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            sortable: false,
            width: None,
            alignment: RecordColumnAlignment::Left,
            fixed: false,
            description: None,
        }
    }

    /// Enables or disables sorting for the column.
    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    /// Sets the logical column width.
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = Some(width);
        self
    }

    /// Sets the column's horizontal content alignment.
    pub fn alignment(mut self, alignment: RecordColumnAlignment) -> Self {
        self.alignment = alignment;
        self
    }

    /// Pins or unpins the column at the leading edge.
    pub fn fixed(mut self, fixed: bool) -> Self {
        self.fixed = fixed;
        self
    }

    /// Adds supplementary accessible column guidance.
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Returns the stable application column ID.
    pub fn id(&self) -> &str {
        self.id.as_ref()
    }

    /// Returns the visible column label.
    pub fn label(&self) -> &str {
        self.label.as_ref()
    }

    /// Returns whether the column can request sorting.
    pub fn is_sortable(&self) -> bool {
        self.sortable
    }

    /// Returns the explicitly configured logical column width, if any.
    pub fn configured_width(&self) -> Option<Pixels> {
        self.width
    }

    /// Returns the horizontal content alignment.
    pub fn column_alignment(&self) -> RecordColumnAlignment {
        self.alignment
    }

    /// Returns whether the column is pinned to the leading edge.
    pub fn is_fixed(&self) -> bool {
        self.fixed
    }

    /// Returns supplementary accessible guidance, if supplied.
    pub fn accessible_description(&self) -> Option<&str> {
        self.description.as_deref()
    }
}

/// One readable record value associated with a stable column ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordCell {
    column_id: SharedString,
    value: SharedString,
    kind: RecordCellKind,
    tags: Arc<[SharedString]>,
    status_tone: Option<RecordStatusTone>,
}

impl RecordCell {
    /// Creates a cell for the identified column.
    pub fn new(column_id: impl Into<SharedString>, value: impl Into<SharedString>) -> Self {
        Self {
            column_id: column_id.into(),
            value: value.into(),
            kind: RecordCellKind::Text,
            tags: Arc::from([]),
            status_tone: None,
        }
    }

    /// Creates a categorical tag cell.
    pub fn tags(
        column_id: impl Into<SharedString>,
        tags: impl IntoIterator<Item = impl Into<SharedString>>,
    ) -> Self {
        let tags: Arc<[SharedString]> = tags.into_iter().map(Into::into).collect::<Vec<_>>().into();
        let value = tags
            .iter()
            .map(AsRef::<str>::as_ref)
            .collect::<Vec<_>>()
            .join(", ");
        Self {
            column_id: column_id.into(),
            value: value.into(),
            kind: RecordCellKind::Tags,
            tags,
            status_tone: None,
        }
    }

    /// Creates a readable semantic status cell.
    pub fn status(
        column_id: impl Into<SharedString>,
        value: impl Into<SharedString>,
        tone: RecordStatusTone,
    ) -> Self {
        Self {
            column_id: column_id.into(),
            value: value.into(),
            kind: RecordCellKind::Status,
            tags: Arc::from([]),
            status_tone: Some(tone),
        }
    }

    /// Returns the stable column ID associated with this value.
    pub fn column_id(&self) -> &str {
        self.column_id.as_ref()
    }

    /// Returns the readable cell value.
    pub fn value(&self) -> &str {
        self.value.as_ref()
    }

    /// Returns the cell's typed visual structure.
    pub fn kind(&self) -> RecordCellKind {
        self.kind
    }

    /// Returns categorical tags for a tag cell.
    pub fn tag_values(&self) -> &[SharedString] {
        &self.tags
    }

    /// Returns the semantic status tone for a status cell.
    pub fn status_tone(&self) -> Option<RecordStatusTone> {
        self.status_tone
    }
}

/// One immutable record row keyed by a stable application ID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordRow {
    id: SharedString,
    label: SharedString,
    cells: Vec<RecordCell>,
    disabled: bool,
}

impl RecordRow {
    /// Creates an empty row with a stable ID and accessible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            cells: Vec::new(),
            disabled: false,
        }
    }

    /// Replaces the row's immutable cell snapshot.
    pub fn cells(mut self, cells: impl IntoIterator<Item = RecordCell>) -> Self {
        self.cells = cells.into_iter().collect();
        self
    }

    /// Sets whether the row rejects selection and activation intent.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Returns the stable application row ID.
    pub fn id(&self) -> &str {
        self.id.as_ref()
    }

    /// Returns the row's accessible label.
    pub fn label(&self) -> &str {
        self.label.as_ref()
    }

    /// Returns the cell associated with `column_id`, if present.
    pub fn cell(&self, column_id: &str) -> Option<&RecordCell> {
        self.cells
            .iter()
            .find(|cell| cell.column_id.as_ref() == column_id)
    }

    /// Returns the immutable cell snapshot in consumer order.
    pub fn all_cells(&self) -> &[RecordCell] {
        &self.cells
    }

    /// Returns whether the row rejects selection and activation intent.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }
}

pub(crate) fn record_columns_have_unique_ids(columns: &[RecordColumn]) -> bool {
    let mut seen = HashSet::with_capacity(columns.len());
    columns.iter().all(|column| seen.insert(column.id()))
}

pub(crate) fn record_rows_have_unique_ids(rows: &[RecordRow]) -> bool {
    let mut row_ids = HashSet::with_capacity(rows.len());
    rows.iter().all(|row| {
        let mut cell_ids = HashSet::with_capacity(row.cells.len());
        row_ids.insert(row.id())
            && row
                .cells
                .iter()
                .all(|cell| cell_ids.insert(cell.column_id()))
    })
}

pub(crate) trait RecordCellProvider: Send + Sync {
    fn cell(&self, row_ix: usize, column_id: &str) -> Option<RecordCell>;
}

#[derive(Clone)]
struct RecordsDelegate {
    owner: WeakEntity<RecordsTable>,
    component_id: SharedString,
    columns: Arc<[RecordColumn]>,
    records: Progressive<Arc<[RecordRow]>>,
    cell_provider: Option<Arc<dyn RecordCellProvider>>,
    selected_row_id: Option<SharedString>,
    sort_column_id: Option<SharedString>,
    sort_direction: Option<RecordSortDirection>,
    activation_label: SharedString,
    row_reorder_duration: Option<Duration>,
    row_reorder_offsets: HashMap<SharedString, Pixels>,
    row_reorder_current_offsets: HashMap<SharedString, Pixels>,
    row_reorder_generation: usize,
    initialized_reorder_rows: HashSet<SharedString>,
    visible_row_ids: HashSet<SharedString>,
}

impl RecordsDelegate {
    fn empty(owner: WeakEntity<RecordsTable>, component_id: SharedString) -> Self {
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
            row_reorder_duration: None,
            row_reorder_offsets: HashMap::new(),
            row_reorder_current_offsets: HashMap::new(),
            row_reorder_generation: 0,
            initialized_reorder_rows: HashSet::new(),
            visible_row_ids: HashSet::new(),
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
                let width = column.width;
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
        if self.row_reorder_duration.is_none() {
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
        self.visible_row_ids.insert(row.id.clone());
        let owner = self.owner.clone();
        let pointer_row_id = row.id.clone();
        let row_frame = record_row_frame(
            &self.component_id,
            &row,
            self.selected_row_id.as_ref() == Some(&row.id),
        );
        let row_frame = if let (Some(duration), Some(initial_offset)) = (
            self.row_reorder_duration,
            self.row_reorder_offsets.get(&row.id).copied(),
        ) {
            let transition_id: SharedString = format!(
                "{}:{}",
                scoped_records_id("reorder", &self.component_id, &row.id),
                self.row_reorder_generation
            )
            .into();
            if self.initialized_reorder_rows.insert(row.id.clone()) {
                let _ = transition(
                    (transition_id.clone(), "position"),
                    initial_offset,
                    Transition::new(duration),
                    window,
                    cx,
                );
            }
            let offset = transition(
                (transition_id, "position"),
                // Reorder motion starts from rest; zero is the animation's
                // physical origin, not a spacing value.
                gpui::px(0.),
                Transition::new(duration),
                window,
                cx,
            );
            self.row_reorder_current_offsets
                .insert(row.id.clone(), offset);
            row_frame.relative().top(offset)
        } else {
            row_frame
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
        _: &mut Window,
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
            .map(|cell| record_cell_content(&scoped_identity, cell, cx))
            .unwrap_or_else(|| div().into_any_element());
        let content = if col_ix == 0 {
            if let Some(row) = row {
                let owner = self.owner.clone();
                let row_id = row.id.clone();
                let activation =
                    record_activation_button(&self.component_id, &self.activation_label, row, cx)
                        .on_click(move |_, _, cx| {
                            let _ = owner.update(cx, |table, cx| {
                                table.request_activation(row_id.clone(), cx);
                            });
                            cx.stop_propagation();
                        });
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(div().flex_1().min_w_0().overflow_hidden().child(content))
                    .child(activation)
                    .into_any_element()
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
        if self.row_reorder_duration.is_some() {
            self.visible_row_ids = self
                .records
                .content()
                .get(visible_range)
                .unwrap_or_default()
                .iter()
                .map(|row| row.id.clone())
                .collect();
        }
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

pub(crate) fn escape_markdown_text(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(
            character,
            '\\' | '`'
                | '*'
                | '_'
                | '{'
                | '}'
                | '['
                | ']'
                | '<'
                | '>'
                | '('
                | ')'
                | '#'
                | '+'
                | '-'
                | '.'
                | '!'
                | '|'
        ) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn records_table_frame(id: SharedString, label: SharedString) -> Stateful<Div> {
    let debug_id = id.clone();
    div()
        .id(format!("records-table-{id}"))
        .debug_selector(move || format!("records-table-{debug_id}"))
        .size_full()
        .role(Role::Table)
        .aria_label(label)
}

fn scoped_records_id(kind: &str, component_id: &str, local_id: &str) -> String {
    format!(
        "records-{kind}-{}:{component_id}{local_id}",
        component_id.len()
    )
}

fn record_sort_button(
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

fn record_activation_button(
    component_id: &str,
    activation_label: &str,
    row: &RecordRow,
    cx: &mut App,
) -> gpui_base::Button {
    let debug_id = scoped_records_id("activate", component_id, &row.id);
    let label = if row.disabled {
        format!("Unavailable: {activation_label} {}", row.label)
    } else {
        format!("{activation_label} {}", row.label)
    };
    outlined_control_with_label(debug_id.clone(), label, activation_label.to_owned(), cx)
        .debug_selector(move || debug_id.clone())
        .flex_none()
        .disabled(row.disabled)
}

fn records_state_frame(
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

/// A glyph that restates the state family without color: a spinner for
/// in-flight work, a cross for failures, a dash for nothing to show.
fn records_state_glyph(role: Role) -> AnyElement {
    match role {
        Role::ProgressIndicator => Spinner::new().xsmall().into_any_element(),
        Role::Alert => Icon::new(IconName::CircleX).xsmall().into_any_element(),
        _ => Icon::new(IconName::Dash).xsmall().into_any_element(),
    }
}

/// Loading labels shimmer; settled states stay selectable prose.
fn records_state_text(scoped_id: &str, role: Role, label: SharedString) -> AnyElement {
    if role == Role::ProgressIndicator {
        Shimmer::new(format!("{scoped_id}-shimmer"), label).into_any_element()
    } else {
        TextView::markdown(
            format!("{scoped_id}-text"),
            escape_markdown_text(label.as_ref()),
        )
        .selectable(true)
        .into_any_element()
    }
}

fn records_inline_state_frame(
    component_id: &str,
    id: &str,
    role: Role,
    label: SharedString,
    cx: &App,
) -> Stateful<Div> {
    let scoped_id = scoped_records_id("state", component_id, id);
    let tokens = cx.theme().semantic_tokens();
    div()
        .id(scoped_id.clone())
        .debug_selector({
            let scoped_id = scoped_id.clone();
            move || scoped_id.clone()
        })
        .w_full()
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .py(tokens.spacing.xxs)
        .gap(tokens.spacing.xs)
        .role(role)
        .aria_label(label.clone())
        .text_color(match role {
            Role::Alert => cx.theme().danger,
            _ => cx.theme().muted_foreground,
        })
        .child(records_state_glyph(role))
        .child(records_state_text(&scoped_id, role, label))
}

fn record_row_frame(component_id: &str, row: &RecordRow, selected: bool) -> Stateful<Div> {
    let debug_row_id = row.id.clone();
    let component_id = SharedString::from(component_id);
    div()
        .id(scoped_records_id("row", &component_id, &row.id))
        .debug_selector(move || scoped_records_id("row", &component_id, &debug_row_id))
        .role(Role::Row)
        .aria_label(row.label.clone())
        .aria_selected(selected)
        .when(row.disabled, |this| {
            this.aria_description("Unavailable record")
                .aria_value("Disabled")
        })
}

fn record_cell_frame(identity: impl Into<String>, value: SharedString) -> Stateful<Div> {
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

fn record_cell_accessible_value(
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

fn record_cell_content(identity: &str, cell: &RecordCell, cx: &mut App) -> gpui::AnyElement {
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
            div()
                .flex()
                .items_center()
                .gap(tokens.spacing.xs)
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

/// A controlled, virtualized records grid built on gpui-component's table.
///
/// Applications own columns, records, progress, sorting, and the selected row
/// ID. The entity retains only upstream focus and scrolling state.
pub struct RecordsTable {
    id: SharedString,
    label: SharedString,
    columns: Arc<[RecordColumn]>,
    records: Progressive<Arc<[RecordRow]>>,
    selected_row_id: Option<SharedString>,
    sort_column_id: Option<SharedString>,
    sort_direction: Option<RecordSortDirection>,
    activation_label: SharedString,
    row_reorder_duration: Option<Duration>,
    pending_suppressed_selection_events: usize,
    pending_pointer_row_id: Option<SharedString>,
    viewport_row_anchor_id: Option<SharedString>,
    viewport_column_anchor_id: Option<SharedString>,
    table: gpui::Entity<TableState<RecordsDelegate>>,
    _table_subscription: Subscription,
}

impl RecordsTable {
    /// Creates an empty records table with a stable ID and accessible label.
    pub fn new(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let id = id.into();
        let owner = cx.weak_entity();
        let delegate_id = id.clone();
        let table = cx.new(|cx| {
            TableState::new(RecordsDelegate::empty(owner, delegate_id), window, cx)
                .loop_selection(false)
                .col_selectable(false)
                .col_movable(false)
                .row_selectable(true)
                .sortable(false)
        });
        let table_subscription = cx.subscribe(&table, |this, _, event, cx| {
            this.handle_table_event(event, cx);
        });

        Self {
            id,
            label: label.into(),
            columns: Arc::from([]),
            records: Progressive::pending(Arc::from([])),
            selected_row_id: None,
            sort_column_id: None,
            sort_direction: None,
            activation_label: "Open".into(),
            row_reorder_duration: None,
            pending_suppressed_selection_events: 0,
            pending_pointer_row_id: None,
            viewport_row_anchor_id: None,
            viewport_column_anchor_id: None,
            table,
            _table_subscription: table_subscription,
        }
    }

    /// Replaces the visible and accessible verb used by row activation controls.
    pub fn set_activation_label(
        &mut self,
        activation_label: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.activation_label = activation_label.into();
        let activation_label = self.activation_label.clone();
        self.table.update(cx, |table, cx| {
            table.delegate_mut().activation_label = activation_label;
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn set_row_reorder_duration(
        &mut self,
        duration: Option<Duration>,
        cx: &mut Context<Self>,
    ) {
        self.row_reorder_duration = duration;
        self.table.update(cx, |table, cx| {
            table.delegate_mut().row_reorder_duration = duration;
            cx.notify();
        });
        cx.notify();
    }

    pub(crate) fn visible_row_count(&self, cx: &App) -> usize {
        self.table.read(cx).delegate().visible_row_ids.len()
    }

    pub(crate) fn animating_row_count(&self, cx: &App) -> usize {
        self.table.read(cx).delegate().row_reorder_offsets.len()
    }

    /// Replaces the controlled column snapshot without rebuilding table state.
    ///
    /// A snapshot containing duplicate stable column IDs is ignored atomically.
    pub fn set_columns(
        &mut self,
        columns: impl IntoIterator<Item = RecordColumn>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let columns = columns.into_iter().collect::<Vec<_>>();
        if !record_columns_have_unique_ids(&columns) {
            return;
        }
        let anchor_column_id = self.viewport_column_anchor_id.clone().or_else(|| {
            let fixed_columns = self.columns.iter().filter(|column| column.fixed).count();
            self.columns
                .get(
                    self.table
                        .read(cx)
                        .visible_range()
                        .cols()
                        .start
                        .saturating_add(fixed_columns),
                )
                .map(|column| column.id.clone())
        });
        self.columns = columns.into();
        if self.sort_column_id.as_ref().is_some_and(|sort_column_id| {
            !self
                .columns
                .iter()
                .any(|column| column.id == *sort_column_id && column.sortable)
        }) {
            self.sort_column_id = None;
            self.sort_direction = None;
        }
        let columns = self.columns.clone();
        let sort_column_id = self.sort_column_id.clone();
        let sort_direction = self.sort_direction;
        let anchor_column_ix = anchor_column_id
            .as_ref()
            .and_then(|anchor| self.columns.iter().position(|column| column.id == *anchor));
        self.table.update(cx, |table, cx| {
            table.delegate_mut().columns = columns;
            table.delegate_mut().sort_column_id = sort_column_id;
            table.delegate_mut().sort_direction = sort_direction;
            table.refresh(cx);
            if let Some(column_ix) = anchor_column_ix {
                table.scroll_to_col(column_ix, cx);
            }
            cx.notify();
        });
        cx.notify();
    }

    /// Replaces the controlled progressive record snapshot.
    ///
    /// A snapshot containing duplicate row IDs or duplicate cell column IDs
    /// within one row is ignored atomically.
    pub fn set_records(
        &mut self,
        records: Progressive<Arc<[RecordRow]>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.set_records_snapshot(records, cx);
    }

    pub(crate) fn set_records_snapshot(
        &mut self,
        records: Progressive<Arc<[RecordRow]>>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.set_records_snapshot_with_cell_provider(records, None, cx)
    }

    pub(crate) fn set_records_snapshot_with_cell_provider(
        &mut self,
        records: Progressive<Arc<[RecordRow]>>,
        cell_provider: Option<Arc<dyn RecordCellProvider>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !record_rows_have_unique_ids(records.content()) {
            return false;
        }
        let anchor_row_id = self.viewport_row_anchor_id.clone().or_else(|| {
            self.records
                .content()
                .get(self.table.read(cx).visible_range().rows().start)
                .map(|row| row.id.clone())
        });
        let old_visible_range = self.table.read(cx).visible_range().rows().clone();
        let visible_row_ids = if self.row_reorder_duration.is_some() {
            let rendered = self.table.read(cx).delegate().visible_row_ids.clone();
            if rendered.is_empty() {
                self.records
                    .content()
                    .get(old_visible_range.clone())
                    .unwrap_or_default()
                    .iter()
                    .map(|row| row.id.clone())
                    .collect::<Vec<_>>()
            } else {
                rendered.into_iter().collect()
            }
        } else {
            Vec::new()
        };
        let post_snapshot_anchor_ix = anchor_row_id
            .as_ref()
            .and_then(|anchor| records.content().iter().position(|row| row.id == *anchor))
            .or_else(|| {
                visible_row_ids
                    .iter()
                    .filter_map(|row_id| records.content().iter().position(|row| row.id == *row_id))
                    .min()
            });
        let reorder_offsets = if self.row_reorder_duration.is_some() && !cx.reduce_motion() {
            let current_offsets = self
                .table
                .read(cx)
                .delegate()
                .row_reorder_current_offsets
                .clone();
            let old_visible_start = old_visible_range.start;
            let visible_len = old_visible_range.len().max(visible_row_ids.len()).max(1);
            let new_visible_start = post_snapshot_anchor_ix
                .unwrap_or(old_visible_start.min(records.content().len().saturating_sub(1)));
            let new_visible_end = new_visible_start
                .saturating_add(visible_len)
                .min(records.content().len());
            visible_row_ids
                .iter()
                .filter_map(|row_id| {
                    let old_ix = self
                        .records
                        .content()
                        .iter()
                        .position(|row| row.id == *row_id)?;
                    let new_ix = records.content().iter().position(|row| row.id == *row_id)?;
                    if !(new_visible_start..new_visible_end).contains(&new_ix) {
                        return None;
                    }
                    // Motion offsets default to rest; zero is animation
                    // geometry, not a spacing value.
                    let prior_offset = current_offsets
                        .get(row_id)
                        .copied()
                        .unwrap_or_else(|| gpui::px(0.));
                    let old_position = old_ix.saturating_sub(old_visible_start);
                    let new_position = new_ix.saturating_sub(new_visible_start);
                    let offset = Size::Medium.table_row_height()
                        * (old_position as f32 - new_position as f32)
                        + prior_offset;
                    (offset != gpui::px(0.)).then(|| (row_id.clone(), offset))
                })
                .collect::<HashMap<_, _>>()
        } else {
            HashMap::new()
        };
        self.records = records;
        if self.selected_row_id.as_ref().is_some_and(|selected| {
            !self
                .records
                .content()
                .iter()
                .any(|row| row.id == *selected && !row.disabled)
        }) {
            self.selected_row_id = None;
        }

        let records = self.records.clone();
        let selected_row_id = self.selected_row_id.clone();
        let desired_row_ix = selected_row_id.as_ref().and_then(|selected| {
            self.records
                .content()
                .iter()
                .position(|row| row.id == *selected)
        });
        let anchor_row_ix = anchor_row_id
            .as_ref()
            .and_then(|anchor| {
                self.records
                    .content()
                    .iter()
                    .position(|row| row.id == *anchor)
            })
            .or(post_snapshot_anchor_ix);
        if desired_row_ix.is_some() {
            self.pending_suppressed_selection_events =
                self.pending_suppressed_selection_events.saturating_add(1);
        }
        self.table.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.records = records;
            delegate.cell_provider = cell_provider;
            delegate.selected_row_id = selected_row_id.clone();
            delegate.row_reorder_offsets = reorder_offsets;
            delegate.row_reorder_current_offsets.clear();
            delegate.row_reorder_generation = delegate.row_reorder_generation.wrapping_add(1);
            delegate.initialized_reorder_rows.clear();
            table.refresh(cx);
            if let Some(row_ix) = desired_row_ix {
                table.set_selected_row(row_ix, cx);
            } else {
                table.clear_selection(cx);
            }
            if let Some(row_ix) = anchor_row_ix {
                table.scroll_to_row(row_ix, cx);
            }
            cx.notify();
        });
        cx.notify();
        true
    }

    /// Replaces the controlled selected row when the ID exists.
    pub fn set_selected_row(
        &mut self,
        row_id: impl Into<SharedString>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let row_id = row_id.into();
        let Some(row_ix) = self
            .records
            .content()
            .iter()
            .position(|row| row.id == row_id && !row.disabled)
        else {
            return;
        };

        self.selected_row_id = Some(row_id);
        self.pending_suppressed_selection_events =
            self.pending_suppressed_selection_events.saturating_add(1);
        let selected_row_id = self.selected_row_id.clone();
        self.table.update(cx, |table, cx| {
            table.delegate_mut().selected_row_id = selected_row_id;
            table.set_selected_row(row_ix, cx);
        });
        cx.notify();
    }

    /// Clears the controlled selected-row snapshot.
    pub fn clear_selected_row(&mut self, cx: &mut Context<Self>) {
        self.selected_row_id = None;
        self.table.update(cx, |table, cx| {
            table.delegate_mut().selected_row_id = None;
            table.clear_selection(cx);
        });
        cx.notify();
    }

    /// Returns the controlled selected row ID.
    pub fn selected_row_id(&self) -> Option<&str> {
        self.selected_row_id.as_deref()
    }

    /// Replaces the controlled sort snapshot.
    ///
    /// Passing `None` clears sorting. A non-sortable or unknown column ID is
    /// ignored so stale application snapshots cannot corrupt table state.
    pub fn set_sort(
        &mut self,
        column_id: impl Into<SharedString>,
        direction: Option<RecordSortDirection>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let column_id = column_id.into();
        if direction.is_some()
            && !self
                .columns
                .iter()
                .any(|column| column.id == column_id && column.sortable)
        {
            return;
        }

        self.sort_column_id = direction.map(|_| column_id.clone());
        self.sort_direction = direction;
        let sort_column_id = self.sort_column_id.clone();
        self.table.update(cx, |table, cx| {
            table.delegate_mut().sort_column_id = sort_column_id;
            table.delegate_mut().sort_direction = direction;
            cx.notify();
        });
        cx.notify();
    }

    /// Returns the controlled sort column ID and direction.
    pub fn sort(&self) -> Option<(&str, RecordSortDirection)> {
        self.sort_column_id.as_deref().zip(self.sort_direction)
    }

    /// Scrolls the identified record into view when it exists.
    pub fn scroll_to_row(&mut self, row_id: &str, cx: &mut Context<Self>) {
        let Some(row_ix) = self
            .records
            .content()
            .iter()
            .position(|row| row.id() == row_id)
        else {
            return;
        };
        self.table
            .update(cx, |table, cx| table.scroll_to_row(row_ix, cx));
        self.viewport_row_anchor_id = Some(row_id.into());
    }

    /// Scrolls the identified column into view when it exists.
    pub fn scroll_to_column(&mut self, column_id: &str, cx: &mut Context<Self>) {
        let Some(col_ix) = self
            .columns
            .iter()
            .position(|column| column.id() == column_id)
        else {
            return;
        };
        self.table
            .update(cx, |table, cx| table.scroll_to_col(col_ix, cx));
        self.viewport_column_anchor_id = Some(column_id.into());
    }

    /// Moves keyboard focus to the records grid.
    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.table.focus_handle(cx).focus(window, cx);
    }

    fn handle_table_event(&mut self, event: &TableEvent, cx: &mut Context<Self>) {
        match event {
            TableEvent::SelectRow(_) if self.pending_suppressed_selection_events > 0 => {
                self.pending_suppressed_selection_events =
                    self.pending_suppressed_selection_events.saturating_sub(1);
            }
            TableEvent::SelectRow(row_ix) => {
                if let Some(row) = self.records.content().get(*row_ix) {
                    let from_pointer = self.pending_pointer_row_id.take().as_ref() == Some(&row.id);
                    if !row.disabled {
                        cx.emit(RecordsTableEvent::SelectionRequested {
                            id: self.id.clone(),
                            row_id: row.id.clone(),
                        });
                    } else if !from_pointer {
                        self.request_enabled_beyond(*row_ix, cx);
                    }
                }
                self.defer_controlled_selection_sync(cx);
            }
            TableEvent::DoubleClickedRow(row_ix) => {
                if let Some(row) = self.records.content().get(*row_ix)
                    && !row.disabled
                {
                    cx.emit(RecordsTableEvent::ActivationRequested {
                        id: self.id.clone(),
                        row_id: row.id.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    fn defer_controlled_selection_sync(&self, cx: &mut Context<Self>) {
        let owner = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = owner.update(cx, |table, cx| table.sync_controlled_selection(cx));
        });
    }

    fn sync_controlled_selection(&mut self, cx: &mut Context<Self>) {
        let desired_row_ix = self.selected_row_id.as_ref().and_then(|selected| {
            self.records
                .content()
                .iter()
                .position(|row| row.id == *selected)
        });
        let current_row_ix = self.table.read(cx).selected_row();
        if desired_row_ix == current_row_ix {
            return;
        }

        if desired_row_ix.is_some() {
            self.pending_suppressed_selection_events =
                self.pending_suppressed_selection_events.saturating_add(1);
        }
        self.table.update(cx, |table, cx| {
            if let Some(row_ix) = desired_row_ix {
                table.set_selected_row(row_ix, cx);
            } else if table.selected_row().is_some() {
                table.clear_selection(cx);
            }
        });
    }

    fn request_sort(&mut self, column_id: SharedString, cx: &mut Context<Self>) {
        let direction = if self.sort_column_id.as_ref() == Some(&column_id) {
            match self.sort_direction {
                None => Some(RecordSortDirection::Descending),
                Some(RecordSortDirection::Descending) => Some(RecordSortDirection::Ascending),
                Some(RecordSortDirection::Ascending) => None,
            }
        } else {
            Some(RecordSortDirection::Descending)
        };

        cx.emit(RecordsTableEvent::SortRequested {
            id: self.id.clone(),
            column_id,
            direction,
        });
    }

    fn request_activation(&mut self, row_id: SharedString, cx: &mut Context<Self>) {
        self.pending_pointer_row_id = None;
        if self
            .records
            .content()
            .iter()
            .any(|row| row.id == row_id && !row.disabled)
        {
            cx.emit(RecordsTableEvent::ActivationRequested {
                id: self.id.clone(),
                row_id,
            });
        }
    }

    fn activate_selected(&mut self, _: &ActivateRecord, _: &mut Window, cx: &mut Context<Self>) {
        let Some(row_id) = self.selected_row_id.clone() else {
            cx.propagate();
            return;
        };
        if self
            .records
            .content()
            .iter()
            .find(|row| row.id == row_id)
            .is_none_or(|row| row.disabled)
        {
            cx.propagate();
            return;
        }
        cx.emit(RecordsTableEvent::ActivationRequested {
            id: self.id.clone(),
            row_id,
        });
        cx.stop_propagation();
    }

    fn request_enabled_beyond(&self, disabled_index: usize, cx: &mut Context<Self>) {
        let current = self.selected_row_id.as_ref().and_then(|selected| {
            self.records
                .content()
                .iter()
                .position(|row| row.id == *selected)
        });
        let forward = current.is_none_or(|current| disabled_index > current);
        let next = if forward {
            self.records
                .content()
                .iter()
                .enumerate()
                .skip(disabled_index.saturating_add(1))
                .find(|(_, row)| !row.disabled)
        } else {
            self.records
                .content()
                .iter()
                .enumerate()
                .take(disabled_index)
                .rev()
                .find(|(_, row)| !row.disabled)
        };

        let Some((_, row)) = next else {
            return;
        };
        cx.emit(RecordsTableEvent::SelectionRequested {
            id: self.id.clone(),
            row_id: row.id.clone(),
        });
    }
}

impl EventEmitter<RecordsTableEvent> for RecordsTable {}

impl Focusable for RecordsTable {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.table.focus_handle(cx)
    }
}

impl Render for RecordsTable {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let inline_status = (!self.records.content().is_empty())
            .then(|| match self.records.state() {
                ProgressState::Pending | ProgressState::Running => {
                    Some(records_inline_state_frame(
                        &self.id,
                        "records-loading",
                        Role::ProgressIndicator,
                        "Loading records".into(),
                        cx,
                    ))
                }
                ProgressState::Failed(reason) => Some(records_inline_state_frame(
                    &self.id,
                    "records-error",
                    Role::Alert,
                    format!("Records unavailable: {reason}").into(),
                    cx,
                )),
                ProgressState::Complete => None,
            })
            .flatten();
        records_table_frame(self.id.clone(), self.label.clone())
            .flex()
            .flex_col()
            .min_h_0()
            .key_context(RECORDS_TABLE_CONTEXT)
            .border_1()
            .border_color(cx.theme().transparent)
            .track_focus(&self.table.focus_handle(cx))
            .focus_visible(|style| style.border_color(cx.theme().ring))
            .on_action(cx.listener(Self::activate_selected))
            .when_some(inline_status, |this, status| this.child(status))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(DataTable::new(&self.table).stripe(true).bordered(true)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{
        Element as _, RenderOnce as _, TestAppContext, VisualTestContext, accesskit, canvas, px,
        size,
    };
    use std::sync::{Arc, Mutex};

    #[test]
    fn table_row_and_cell_builders_expose_direct_accesskit_contracts() {
        let table =
            records_table_frame("suppliers".into(), "Supplier records".into()).into_element();
        let mut table_node = accesskit::Node::new(Role::Unknown);
        table.write_a11y_info(&mut table_node);
        assert_eq!(table.a11y_role(), Some(Role::Table));
        assert_eq!(table_node.label(), Some("Supplier records"));

        let row = RecordRow::new("aurora", "Aurora Scoops");
        let row = record_row_frame("suppliers", &row, true).into_element();
        let mut row_node = accesskit::Node::new(Role::Unknown);
        row.write_a11y_info(&mut row_node);
        assert_eq!(row.a11y_role(), Some(Role::Row));
        assert_eq!(row_node.label(), Some("Aurora Scoops"));
        assert_eq!(row_node.is_selected(), Some(true));

        let cell = record_cell_frame("aurora-strength", "Very strong".into()).into_element();
        let mut cell_node = accesskit::Node::new(Role::Unknown);
        cell.write_a11y_info(&mut cell_node);
        assert_eq!(cell.a11y_role(), Some(Role::Cell));
        assert_eq!(cell_node.label(), Some("Very strong"));
        assert_eq!(cell_node.value(), Some("Very strong"));
    }

    #[test]
    fn missing_first_column_value_names_the_synthetic_activation_cell() {
        let row = RecordRow::new("pistachio", "Pistachio proposal");
        let value = record_cell_accessible_value(None, Some(&row), 0, "Review");
        assert_eq!(value, "Review Pistachio proposal");

        let cell = record_cell_frame("pistachio-review", value).into_element();
        let mut cell_node = accesskit::Node::new(Role::Unknown);
        cell.write_a11y_info(&mut cell_node);
        assert_eq!(cell.a11y_role(), Some(Role::Cell));
        assert_eq!(cell_node.label(), Some("Review Pistachio proposal"));
        assert_eq!(cell_node.value(), Some("Review Pistachio proposal"));
    }

    #[gpui::test]
    fn visible_row_reorder_state_is_bounded_and_reduced_motion_snaps(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (records, cx) =
            cx.add_window_view(|window, cx| RecordsTable::new("motion", "Motion", window, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(640.), px(300.)));
        let make_rows = || {
            (0..1_000)
                .map(|index| {
                    RecordRow::new(format!("row-{index}"), format!("Row {index}"))
                        .cells([RecordCell::new("name", format!("Row {index}"))])
                })
                .collect::<Vec<_>>()
        };
        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_row_reorder_duration(Some(Duration::from_millis(180)), cx);
                records.set_columns([RecordColumn::new("name", "Name")], window, cx);
                records.set_records(Progressive::complete(make_rows().into()), window, cx);
            });
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let mut reordered = make_rows();
        reordered.swap(1, 2);
        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_records(Progressive::complete(reordered.into()), window, cx);
            });
        });
        let offsets = records.read_with(cx, |records, cx| {
            records
                .table
                .read(cx)
                .delegate()
                .row_reorder_offsets
                .clone()
        });
        assert!(!offsets.is_empty());
        assert!(
            offsets.len() < 64,
            "only visible stable rows should retain motion state, got {}",
            offsets.len()
        );

        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_records(Progressive::complete(make_rows().into()), window, cx);
                records.scroll_to_row("row-990", cx);
            });
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let filtered = make_rows()
            .into_iter()
            .enumerate()
            .filter_map(|(index, row)| (index % 3 == 2).then_some(row))
            .collect::<Vec<_>>();
        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_records(Progressive::complete(filtered.into()), window, cx);
            });
        });
        let scrolled_offsets = records.read_with(cx, |records, cx| {
            records
                .table
                .read(cx)
                .delegate()
                .row_reorder_offsets
                .clone()
        });
        assert!(
            !scrolled_offsets.is_empty(),
            "a filtered snapshot should animate retained rows in the post-filter anchored viewport"
        );
        assert!(scrolled_offsets.len() < 64);

        cx.update(|_, cx| cx.set_reduce_motion(true));
        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_records(Progressive::complete(make_rows().into()), window, cx);
            });
        });
        records.read_with(cx, |records, cx| {
            assert!(
                records
                    .table
                    .read(cx)
                    .delegate()
                    .row_reorder_offsets
                    .is_empty(),
                "reduced motion should retain no animated offsets"
            );
        });
    }

    #[test]
    fn progressive_state_builders_are_named_and_role_specific() {
        for (id, role, label) in [
            (
                "records-loading",
                Role::ProgressIndicator,
                "Loading records",
            ),
            ("records-empty", Role::Status, "No records"),
            (
                "records-error",
                Role::Alert,
                "Records unavailable: Network unavailable",
            ),
        ] {
            let element = records_state_frame("suppliers", id, role, label.into()).into_element();
            let mut node = accesskit::Node::new(Role::Unknown);
            element.write_a11y_info(&mut node);
            assert_eq!(element.a11y_role(), Some(role));
            assert_eq!(node.label(), Some(label));
        }
    }

    #[gpui::test]
    fn inline_progressive_states_keep_direct_roles_and_names(cx: &mut TestAppContext) {
        cx.update(crate::init);
        cx.update(|cx| {
            for (id, role, label) in [
                (
                    "records-loading",
                    Role::ProgressIndicator,
                    "Loading records",
                ),
                (
                    "records-error",
                    Role::Alert,
                    "Records unavailable: Refresh unavailable",
                ),
            ] {
                let element = records_inline_state_frame("suppliers", id, role, label.into(), cx)
                    .into_element();
                let mut node = accesskit::Node::new(Role::Unknown);
                element.write_a11y_info(&mut node);
                assert_eq!(element.a11y_role(), Some(role));
                assert_eq!(node.label(), Some(label));
            }
        });
    }

    #[gpui::test]
    fn nonempty_progressive_snapshots_keep_rows_and_lifecycle_status_visible(
        cx: &mut TestAppContext,
    ) {
        cx.update(crate::init);
        let (records, cx) =
            cx.add_window_view(|window, cx| RecordsTable::new("status", "Status", window, cx));
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(640.), px(300.)));
        let rows: Arc<[RecordRow]> = Arc::from([RecordRow::new("stale", "Stale record")
            .cells([RecordCell::new("name", "Stale record")])]);

        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_columns([RecordColumn::new("name", "Name")], window, cx);
                records.set_records(Progressive::running(rows.clone()), window, cx);
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("records-row-6:statusstale").is_some());
        assert!(
            cx.debug_bounds("records-state-6:statusrecords-loading")
                .is_some(),
            "running stale content must retain a named progress indicator"
        );

        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_records(Progressive::failed(rows, "Refresh unavailable"), window, cx);
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("records-row-6:statusstale").is_some());
        assert!(
            cx.debug_bounds("records-state-6:statusrecords-error")
                .is_some(),
            "failed stale content must retain a named alert"
        );
    }

    #[gpui::test]
    fn malformed_duplicate_identity_snapshots_are_rejected_atomically(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let (records, cx) =
            cx.add_window_view(|window, cx| RecordsTable::new("identity", "Identity", window, cx));
        let valid_columns: Arc<[RecordColumn]> = Arc::from([
            RecordColumn::new("name", "Name"),
            RecordColumn::new("status", "Status"),
        ]);
        let valid_rows: Arc<[RecordRow]> = Arc::from([
            RecordRow::new("first", "First").cells([
                RecordCell::new("name", "First"),
                RecordCell::new("status", "Ready"),
            ]),
            RecordRow::new("second", "Second").cells([
                RecordCell::new("name", "Second"),
                RecordCell::new("status", "Waiting"),
            ]),
        ]);

        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_columns(valid_columns.iter().cloned(), window, cx);
                records.set_records(Progressive::complete(valid_rows.clone()), window, cx);
            });
        });

        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_columns(
                    [
                        RecordColumn::new("duplicate", "First"),
                        RecordColumn::new("duplicate", "Second"),
                    ],
                    window,
                    cx,
                );
                records.set_records(
                    Progressive::complete(Arc::from([
                        RecordRow::new("duplicate-row", "First"),
                        RecordRow::new("duplicate-row", "Second"),
                    ])),
                    window,
                    cx,
                );
            });
        });
        records.read_with(cx, |records, _| {
            assert_eq!(records.columns, valid_columns);
            assert_eq!(records.records.content().as_ref(), valid_rows.as_ref());
        });

        cx.update(|window, cx| {
            records.update(cx, |records, cx| {
                records.set_records(
                    Progressive::complete(Arc::from([RecordRow::new("bad-cells", "Bad cells")
                        .cells([
                            RecordCell::new("duplicate-cell", "First"),
                            RecordCell::new("duplicate-cell", "Second"),
                        ])])),
                    window,
                    cx,
                );
            });
        });
        records.read_with(cx, |records, _| {
            assert_eq!(records.records.content().as_ref(), valid_rows.as_ref());
        });
    }

    type CapturedControls = Arc<Mutex<Vec<(Option<Role>, accesskit::Node)>>>;

    struct RecordsControlProbe {
        captured: CapturedControls,
    }

    impl Render for RecordsControlProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            let captured = self.captured.clone();
            canvas(
                move |_, window, cx| {
                    let enabled = RecordRow::new("aurora", "Aurora Scoops");
                    let disabled = RecordRow::new("offline", "Offline supplier").disabled(true);
                    let controls = [
                        record_sort_button(
                            "suppliers",
                            "strength",
                            "Status".into(),
                            ", descending",
                            "↓",
                            cx,
                        )
                        .on_click(|_, _, _| {})
                        .render(window, cx)
                        .into_element(),
                        record_activation_button("suppliers", "Open", &enabled, cx)
                            .on_click(|_, _, _| {})
                            .render(window, cx)
                            .into_element(),
                        record_activation_button("suppliers", "Open", &disabled, cx)
                            .on_click(|_, _, _| {})
                            .render(window, cx)
                            .into_element(),
                        record_activation_button("diff", "Review", &enabled, cx)
                            .on_click(|_, _, _| {})
                            .render(window, cx)
                            .into_element(),
                    ];
                    let nodes = controls
                        .into_iter()
                        .map(|control| {
                            let role = control.a11y_role();
                            let mut node = accesskit::Node::new(Role::Unknown);
                            control.write_a11y_info(&mut node);
                            (role, node)
                        })
                        .collect();
                    *captured.lock().expect("capture mutex should be available") = nodes;
                },
                |_, _, _, _| {},
            )
        }
    }

    #[gpui::test]
    fn sort_and_activation_controls_expose_direct_accesskit_contracts(cx: &mut TestAppContext) {
        cx.update(crate::init);
        let captured = CapturedControls::default();
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(|_, _| RecordsControlProbe { captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let nodes = result.lock().expect("capture mutex should be available");
        let (sort_role, sort) = &nodes[0];
        assert_eq!(*sort_role, Some(Role::Button));
        assert_eq!(sort.label(), Some("Sort by Status, descending"));
        assert!(sort.supports_action(accesskit::Action::Click));

        let (activation_role, activation) = &nodes[1];
        assert_eq!(*activation_role, Some(Role::Button));
        assert_eq!(activation.label(), Some("Open Aurora Scoops"));
        assert!(activation.supports_action(accesskit::Action::Click));

        let (disabled_role, disabled) = &nodes[2];
        assert_eq!(*disabled_role, Some(Role::Button));
        assert_eq!(disabled.label(), Some("Unavailable: Open Offline supplier"));
        assert!(!disabled.supports_action(accesskit::Action::Click));

        let (review_role, review) = &nodes[3];
        assert_eq!(*review_role, Some(Role::Button));
        assert_eq!(review.label(), Some("Review Aurora Scoops"));
        assert!(review.supports_action(accesskit::Action::Click));
    }
}
