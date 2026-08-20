//! Stable-ID command search composed over gpui-component's native command palette.

use std::sync::Arc;

use gpui::{
    AccessibleAction, App, AppContext as _, Context, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    SharedString, StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div,
    prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Disableable as _, ElementExt as _, IndexPath,
    command::{Command, CommandItem, CommandState},
    h_flex,
    label::Label,
    v_flex,
};

use crate::theme::SemanticStyledExt as _;

type RowActivation = std::rc::Rc<dyn for<'a, 'b> Fn(&'a mut Window, &'b mut App)>;

/// One application-owned command-search item.
///
/// IDs must be stable and unique inside a snapshot. Labels may be duplicated:
/// selection and emitted events always use `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSearchItem {
    id: SharedString,
    title: SharedString,
    subtitle: Option<SharedString>,
    keywords: Arc<[SharedString]>,
    shortcut: Option<SharedString>,
    disabled: bool,
}

impl CommandSearchItem {
    /// Create an enabled item with stable identity and a visible title.
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            keywords: Arc::from([]),
            shortcut: None,
            disabled: false,
        }
    }

    /// Set secondary descriptive text, which also participates in filtering.
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Replace the extra case-insensitive search terms.
    pub fn keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<SharedString>,
    {
        self.keywords = keywords.into_iter().map(Into::into).collect();
        self
    }

    /// Set the visible shortcut hint.
    pub fn shortcut(mut self, shortcut: impl Into<SharedString>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Set whether the item is visible but unavailable to every activation path.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Return the stable application identity.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Return the visible title.
    pub fn title(&self) -> &SharedString {
        &self.title
    }

    /// Return the optional secondary description.
    pub fn subtitle_text(&self) -> Option<&SharedString> {
        self.subtitle.as_ref()
    }

    /// Return the extra search terms.
    pub fn keyword_terms(&self) -> &[SharedString] {
        &self.keywords
    }

    /// Return the optional shortcut hint.
    pub fn shortcut_text(&self) -> Option<&SharedString> {
        self.shortcut.as_ref()
    }

    /// Return whether activation is disabled.
    pub fn is_disabled(&self) -> bool {
        self.disabled
    }

    fn accessibility_label(&self) -> SharedString {
        match (&self.subtitle, &self.shortcut) {
            (Some(subtitle), Some(shortcut)) => {
                format!("{}, {}, shortcut {}", self.title, subtitle, shortcut).into()
            }
            (Some(subtitle), None) => format!("{}, {}", self.title, subtitle).into(),
            (None, Some(shortcut)) => format!("{}, shortcut {}", self.title, shortcut).into(),
            (None, None) => self.title.clone(),
        }
    }
}

/// A typed application intent emitted by [`CommandSearch`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommandSearchEvent {
    /// The native command query changed.
    QueryChanged {
        /// Command-search component identity.
        id: SharedString,
        /// Latest query text.
        query: SharedString,
    },
    /// An enabled item was confirmed by pointer, keyboard, or accessibility.
    Selected {
        /// Command-search component identity.
        id: SharedString,
        /// Stable application item identity.
        item_id: SharedString,
    },
    /// Escape requested that the surrounding surface dismiss the search.
    Dismissed {
        /// Command-search component identity.
        id: SharedString,
    },
}

fn retained_selection(
    previous: Option<&SharedString>,
    items: &[CommandSearchItem],
) -> Option<SharedString> {
    previous
        .and_then(|id| items.iter().find(|item| item.id == *id && !item.disabled))
        .or_else(|| items.iter().find(|item| !item.disabled))
        .map(|item| item.id.clone())
}

fn command_search_frame(id: &SharedString) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id.clone())
        .accessibility_id(format!("command-search.{id}"))
        .debug_selector({
            let id = id.clone();
            move || format!("command-search-{id}")
        })
        .role(Role::Search)
        .aria_label("Command search")
        .w_full()
        .min_w_0()
        .overflow_hidden()
}

fn row_content(
    search_id: SharedString,
    item: CommandSearchItem,
    activation: Option<RowActivation>,
    cx: &mut App,
) -> gpui::Stateful<gpui::Div> {
    let tokens = cx.theme().semantic_tokens();
    let accessibility_id = format!("command-search.{search_id}.item.{}", item.id);
    let debug_item_id = item.id.clone();
    let content_id = ElementId::from((
        ElementId::from((ElementId::from(search_id), item.id.clone())),
        "content",
    ));
    let label = item.accessibility_label();

    h_flex()
        .id(content_id)
        .accessibility_id(accessibility_id)
        .debug_selector(move || format!("command-search-item-{debug_item_id}"))
        .role(Role::ListBoxOption)
        .aria_label(label)
        .w_full()
        .min_w_0()
        .gap(tokens.spacing.sm)
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .gap(tokens.spacing.xxs)
                .child(Label::new(item.title.clone()).text_token(tokens.typography.sm))
                .when_some(item.subtitle.clone(), |this, subtitle| {
                    this.child(
                        Label::new(subtitle)
                            .text_token(tokens.typography.xs)
                            .text_color(cx.theme().muted_foreground),
                    )
                }),
        )
        .when_some(item.shortcut.clone(), |this, shortcut| {
            this.child(
                div()
                    .flex_none()
                    .px(tokens.spacing.xs)
                    .py(tokens.spacing.xxs)
                    .rounded(tokens.radius.sm)
                    .border_1()
                    .border_color(cx.theme().border)
                    .text_token(tokens.typography.xs)
                    .text_color(cx.theme().muted_foreground)
                    .child(Label::new(shortcut)),
            )
        })
        .when_some(activation, |this, activate| {
            this.on_a11y_action(AccessibleAction::Click, move |_, window, cx| {
                activate(window, cx);
            })
        })
}

fn upstream_item(
    search_id: &SharedString,
    item: &CommandSearchItem,
    owner: WeakEntity<CommandSearch>,
) -> CommandItem {
    let row_item = item.clone();
    let row_search_id = search_id.clone();
    let item_id = item.id.clone();
    let row_activation: Option<RowActivation> = (!item.disabled).then(|| {
        std::rc::Rc::new(move |_: &mut Window, cx: &mut App| {
            _ = owner.update(cx, |search, cx| search.confirm_id(item_id.clone(), cx));
        }) as RowActivation
    });
    let mut search_terms =
        Vec::with_capacity(item.keywords.len() + usize::from(item.subtitle.is_some()));
    if let Some(subtitle) = &item.subtitle {
        search_terms.push(subtitle.clone());
    }
    search_terms.extend(item.keywords.iter().cloned());

    CommandItem::new()
        .label(item.title.clone())
        .keywords(search_terms)
        .child(move |_, cx| {
            row_content(
                row_search_id.clone(),
                row_item.clone(),
                row_activation.clone(),
                cx,
            )
        })
        .disabled(item.disabled)
}

/// A controlled stable-ID adapter around gpui-component's native command palette.
///
/// The application replaces immutable item snapshots and handles typed events.
/// This entity owns one upstream [`CommandState`], which remains the sole owner
/// of native text editing, focus, filtering, navigation, and virtual-list state.
///
/// ```ignore
/// let search = cx.new(|cx| CommandSearch::new("workspace-search", window, cx));
/// search.update(cx, |search, cx| {
///     search.set_items(
///         [CommandSearchItem::new("calendar", "Open calendar")
///             .shortcut("Ctrl+K")],
///         window,
///         cx,
///     );
/// });
/// ```
pub struct CommandSearch {
    id: SharedString,
    items: Arc<[CommandSearchItem]>,
    upstream_items: Arc<[CommandItem]>,
    state: Entity<CommandState>,
    selected_id: Option<SharedString>,
    items_revision: u64,
    applied_selection_revision: u64,
}

impl CommandSearch {
    /// Create an empty command search with native input and focus state.
    pub fn new(id: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            id: id.into(),
            items: Arc::from([]),
            upstream_items: Arc::from([]),
            state: cx.new(|cx| CommandState::new(window, cx)),
            selected_id: None,
            items_revision: 0,
            applied_selection_revision: 0,
        }
    }

    /// Replace the controlled item snapshot and retain selection by stable ID.
    pub fn set_items(
        &mut self,
        items: impl IntoIterator<Item = CommandSearchItem>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let items: Arc<[CommandSearchItem]> = items.into_iter().collect();
        if self.items == items {
            return;
        }

        self.selected_id = retained_selection(self.selected_id.as_ref(), &items);
        let owner = cx.weak_entity();
        self.upstream_items = items
            .iter()
            .map(|item| upstream_item(&self.id, item, owner.clone()))
            .collect();
        self.items = items;
        self.items_revision = self.items_revision.wrapping_add(1);
        cx.notify();
    }

    /// Replace the native query and emit [`CommandSearchEvent::QueryChanged`]
    /// when it actually changes.
    pub fn set_query(
        &mut self,
        query: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state
            .update(cx, |state, cx| state.set_query(query, window, cx));
    }

    /// Return the native input's current query.
    pub fn query(&self, cx: &App) -> SharedString {
        self.state.read(cx).query(cx)
    }

    /// Move focus to the native command input.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.state.update(cx, |state, cx| state.focus(window, cx));
    }

    fn item_id_at(&self, index: IndexPath) -> Option<SharedString> {
        (index.section == 0)
            .then(|| self.items.get(index.row))
            .flatten()
            .map(|item| item.id.clone())
    }

    fn index_for_id(&self, id: &SharedString) -> Option<IndexPath> {
        self.items
            .iter()
            .position(|item| item.id == *id && !item.disabled)
            .map(IndexPath::new)
    }

    fn on_select(&mut self, index: IndexPath) {
        if let Some(item_id) = self.item_id_at(index) {
            self.selected_id = Some(item_id);
        }
    }

    fn on_confirm(&mut self, index: IndexPath, cx: &mut Context<Self>) {
        let Some(item_id) = self.item_id_at(index) else {
            return;
        };
        self.confirm_id(item_id, cx);
    }

    fn confirm_id(&mut self, item_id: SharedString, cx: &mut Context<Self>) {
        let Some(item) = self
            .items
            .iter()
            .find(|item| item.id == item_id && !item.disabled)
        else {
            return;
        };

        self.selected_id = Some(item.id.clone());
        cx.emit(CommandSearchEvent::Selected {
            id: self.id.clone(),
            item_id: item.id.clone(),
        });
    }

    fn on_query(&mut self, query: &str, cx: &mut Context<Self>) {
        cx.emit(CommandSearchEvent::QueryChanged {
            id: self.id.clone(),
            query: query.to_owned().into(),
        });
    }

    fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(CommandSearchEvent::Dismissed {
            id: self.id.clone(),
        });
    }
}

impl EventEmitter<CommandSearchEvent> for CommandSearch {}

impl Focusable for CommandSearch {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state.focus_handle(cx)
    }
}

impl Render for CommandSearch {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let id = self.id.clone();
        let tokens = cx.theme().semantic_tokens();
        let query_owner = cx.weak_entity();
        let select_owner = cx.weak_entity();
        let confirm_owner = cx.weak_entity();
        let cancel_owner = cx.weak_entity();
        let selection_owner = cx.weak_entity();
        let revision = self.items_revision;
        let should_restore_selection = self.applied_selection_revision != revision;
        let command = Command::new(&self.state)
            .items(self.upstream_items.iter().cloned())
            .placeholder("Search commands")
            .max_h(tokens.spacing.xxl * 10.)
            .empty({
                let id = id.clone();
                move |state, _, cx| {
                    let (message, selector): (SharedString, &'static str) =
                        if state.query(cx).is_empty() {
                            (
                                "No commands available".into(),
                                "command-search-empty-catalog",
                            )
                        } else {
                            ("No matching commands".into(), "command-search-no-results")
                        };
                    div()
                        .id(ElementId::from((ElementId::from(id.clone()), "empty")))
                        .debug_selector(move || selector.to_owned())
                        .role(Role::Status)
                        .aria_label(message.clone())
                        .py(cx.theme().semantic_tokens().spacing.lg)
                        .text_color(cx.theme().muted_foreground)
                        .child(Label::new(message))
                }
            })
            .on_query(move |query, _, cx| {
                _ = query_owner.update(cx, |search, cx| search.on_query(query, cx));
            })
            .on_select(move |index, _, cx| {
                _ = select_owner.update(cx, |search, _| search.on_select(index));
            })
            .on_confirm(move |index, _, cx| {
                _ = confirm_owner.update(cx, |search, cx| search.on_confirm(index, cx));
            })
            .on_cancel(move |_, cx| {
                _ = cancel_owner.update(cx, |search, cx| search.dismiss(cx));
            });

        command_search_frame(&self.id)
            .on_prepaint(move |_, window, cx| {
                if !should_restore_selection {
                    return;
                }
                _ = selection_owner.update(cx, |search, cx| {
                    if search.applied_selection_revision == revision {
                        return;
                    }
                    search.applied_selection_revision = revision;
                    let selected_index = search
                        .selected_id
                        .as_ref()
                        .and_then(|id| search.index_for_id(id));
                    search.state.update(cx, |state, cx| {
                        state.set_selected_index(selected_index, window, cx);
                    });
                });
            })
            .child(command)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, Mutex},
    };

    use gpui::{
        AppContext as _, Context, Element as _, Entity, IntoElement as _, KeyDownEvent, KeyUpEvent,
        Keystroke, Modifiers, Render, Role, Subscription, TestAppContext, VisualTestContext,
        Window, accesskit, canvas, px, size,
    };
    use gpui_component::Root;

    use super::{
        CommandSearch, CommandSearchEvent, CommandSearchItem, command_search_frame,
        retained_selection, row_content,
    };

    #[test]
    fn selection_is_retained_by_id_after_replacement_and_reorder() {
        let replacement = vec![
            CommandSearchItem::new("third", "Third"),
            CommandSearchItem::new("selected", "Selected"),
            CommandSearchItem::new("first", "First"),
        ];

        assert_eq!(
            retained_selection(Some(&"selected".into()), &replacement),
            Some("selected".into())
        );
        assert_eq!(
            retained_selection(Some(&"removed".into()), &replacement),
            Some("third".into())
        );
    }

    #[test]
    fn disabled_items_are_skipped_when_choosing_a_replacement_selection() {
        let replacement = vec![
            CommandSearchItem::new("disabled", "Disabled").disabled(true),
            CommandSearchItem::new("enabled", "Enabled"),
        ];

        assert_eq!(
            retained_selection(None, &replacement),
            Some("enabled".into())
        );
    }

    struct CapturedRow {
        role: Option<Role>,
        node: accesskit::Node,
    }

    struct RowA11yProbe {
        disabled: bool,
        captured: Arc<Mutex<Option<CapturedRow>>>,
    }

    impl Render for RowA11yProbe {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            let disabled = self.disabled;
            let captured = self.captured.clone();
            canvas(
                move |_, _, cx| {
                    let row = row_content(
                        "probe".into(),
                        CommandSearchItem::new("report", "Open report")
                            .subtitle("Supplier pricing")
                            .shortcut("Ctrl+R")
                            .disabled(disabled),
                        (!disabled).then(|| {
                            Rc::new(|_: &mut Window, _: &mut gpui::App| {}) as super::RowActivation
                        }),
                        cx,
                    );
                    let role = row.a11y_role();
                    let mut node = accesskit::Node::new(Role::Unknown);
                    row.write_a11y_info(&mut node);
                    *captured.lock().expect("row capture should be available") =
                        Some(CapturedRow { role, node });
                },
                |_, _, _, _| {},
            )
        }
    }

    fn capture_row(disabled: bool, cx: &mut TestAppContext) -> CapturedRow {
        cx.update(crate::init);
        let captured = Arc::new(Mutex::new(None));
        let result = captured.clone();
        let (_, cx) = cx.add_window_view(move |_, _| RowA11yProbe { disabled, captured });
        cx.update(|window, cx| window.draw(cx).clear(cx));
        result
            .lock()
            .expect("row capture should be available")
            .take()
            .expect("row should be captured")
    }

    #[gpui::test]
    fn production_rows_expose_option_name_and_only_enabled_activation(cx: &mut TestAppContext) {
        let enabled = capture_row(false, cx);
        assert_eq!(enabled.role, Some(Role::ListBoxOption));
        assert_eq!(
            enabled.node.label(),
            Some("Open report, Supplier pricing, shortcut Ctrl+R")
        );
        assert!(enabled.node.supports_action(accesskit::Action::Click));

        let disabled = capture_row(true, cx);
        assert_eq!(disabled.role, Some(Role::ListBoxOption));
        assert_eq!(
            disabled.node.label(),
            Some("Open report, Supplier pricing, shortcut Ctrl+R")
        );
        assert!(!disabled.node.supports_action(accesskit::Action::Click));
    }

    #[test]
    fn production_frame_exposes_a_named_search_landmark() {
        let frame = command_search_frame(&"palette".into()).into_element();
        let mut node = accesskit::Node::new(Role::Unknown);
        frame.write_a11y_info(&mut node);

        assert_eq!(frame.a11y_role(), Some(Role::Search));
        assert_eq!(node.label(), Some("Command search"));
    }

    struct Harness {
        search: Entity<CommandSearch>,
        events: Rc<RefCell<Vec<CommandSearchEvent>>>,
        _subscription: Subscription,
    }

    impl Harness {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let search = cx.new(|cx| CommandSearch::new("test-search", window, cx));
            let events = Rc::new(RefCell::new(Vec::new()));
            let captured = events.clone();
            let subscription = cx.subscribe(&search, move |_, _, event, _| {
                captured.borrow_mut().push(event.clone());
            });
            Self {
                search,
                events,
                _subscription: subscription,
            }
        }
    }

    impl Render for Harness {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl gpui::IntoElement {
            self.search.clone()
        }
    }

    fn interactive_catalog() -> Vec<CommandSearchItem> {
        vec![
            CommandSearchItem::new("first", "Open report").shortcut("Ctrl+1"),
            CommandSearchItem::new("disabled", "Unavailable report").disabled(true),
            CommandSearchItem::new("second", "Open report")
                .subtitle("Supplier pricing")
                .keywords(["margin"])
                .shortcut("Ctrl+2"),
        ]
    }

    fn harness(cx: &mut TestAppContext) -> (Entity<Harness>, &mut VisualTestContext) {
        cx.update(crate::init);
        let (root, cx) = cx.add_window_view(|window, cx| {
            let content = cx.new(|cx| Harness::new(window, cx));
            Root::new(content, window, cx)
        });
        let harness = root.read_with(cx, |root, _| {
            root.view()
                .clone()
                .downcast::<Harness>()
                .expect("command-search harness should remain the root view")
        });
        let cx: &mut VisualTestContext = cx;
        cx.simulate_resize(size(px(520.), px(360.)));
        let search = harness.read_with(cx, |harness, _| harness.search.clone());
        cx.update(|window, cx| {
            search.update(cx, |search, cx| {
                search.set_items(interactive_catalog(), window, cx);
                search.focus(window, cx);
            });
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        (harness, cx)
    }

    fn activate_key(cx: &mut VisualTestContext, key: &str) {
        let keystroke = Keystroke::parse(key).expect("test key should parse");
        cx.simulate_event(KeyDownEvent {
            keystroke: keystroke.clone(),
            is_held: false,
            prefer_character_input: false,
        });
        cx.simulate_event(KeyUpEvent { keystroke });
        cx.run_until_parked();
    }

    #[gpui::test]
    fn upstream_filter_matches_title_subtitle_and_keywords_case_insensitively(
        cx: &mut TestAppContext,
    ) {
        let (harness, cx) = harness(cx);
        let search = harness.read_with(cx, |harness, _| harness.search.clone());
        cx.update(|window, cx| {
            search.update(cx, |search, cx| {
                search.set_items(
                    [
                        CommandSearchItem::new("calendar", "Open Calendar")
                            .subtitle("Review supplier delivery dates")
                            .keywords(["schedule", "dates"]),
                        CommandSearchItem::new("pricing", "Open report")
                            .subtitle("Supplier pricing")
                            .keywords(["margin", "cost"]),
                        CommandSearchItem::new("archive", "Archive workspace")
                            .keywords(["close", "store"]),
                    ],
                    window,
                    cx,
                );
            });
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();

        for (query, expected_id) in [
            ("CALENDAR", "calendar"),
            ("DELIVERY", "calendar"),
            ("MARGIN", "pricing"),
        ] {
            cx.update(|window, cx| {
                search.update(cx, |search, cx| search.set_query(query, window, cx));
            });
            cx.run_until_parked();
            search.read_with(cx, |search, cx| {
                assert_eq!(search.state.read(cx).matched_count(), 1, "query {query}");
                assert_eq!(
                    search.selected_id.as_deref(),
                    Some(expected_id),
                    "query {query}"
                );
            });
        }
    }

    #[gpui::test]
    fn programmatic_query_emits_the_latest_stable_component_id(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        let search = harness.read_with(cx, |harness, _| harness.search.clone());

        cx.update(|window, cx| {
            search.update(cx, |search, cx| search.set_query("MARGIN", window, cx));
        });
        cx.run_until_parked();

        assert_eq!(
            harness.read_with(cx, |harness, _| harness.events.borrow().clone()),
            [CommandSearchEvent::QueryChanged {
                id: "test-search".into(),
                query: "MARGIN".into(),
            }]
        );
    }

    #[gpui::test]
    fn empty_catalog_and_no_results_render_distinct_named_statuses(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        let search = harness.read_with(cx, |harness, _| harness.search.clone());

        cx.update(|window, cx| {
            search.update(cx, |search, cx| search.set_items([], window, cx));
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("command-search-empty-catalog").is_some());
        assert!(cx.debug_bounds("command-search-no-results").is_none());

        cx.update(|window, cx| {
            search.update(cx, |search, cx| {
                search.set_items(
                    [CommandSearchItem::new("present", "Present command")],
                    window,
                    cx,
                );
                search.set_query("absent", window, cx);
            });
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("command-search-empty-catalog").is_none());
        assert!(cx.debug_bounds("command-search-no-results").is_some());
    }

    #[gpui::test]
    fn keyboard_navigation_skips_disabled_and_confirms_duplicate_label_by_id(
        cx: &mut TestAppContext,
    ) {
        let (harness, cx) = harness(cx);

        activate_key(cx, "down");
        activate_key(cx, "enter");

        assert_eq!(
            harness.read_with(cx, |harness, _| harness.events.borrow().clone()),
            [CommandSearchEvent::Selected {
                id: "test-search".into(),
                item_id: "second".into(),
            }]
        );
    }

    #[gpui::test]
    fn pointer_and_keyboard_activation_emit_the_same_stable_event(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        let first = cx
            .debug_bounds("command-search-item-first")
            .expect("the first visible row should expose a stable test selector");

        cx.simulate_click(first.center(), Modifiers::default());
        cx.run_until_parked();
        activate_key(cx, "enter");

        assert_eq!(
            harness.read_with(cx, |harness, _| harness.events.borrow().clone()),
            [
                CommandSearchEvent::Selected {
                    id: "test-search".into(),
                    item_id: "first".into(),
                },
                CommandSearchEvent::Selected {
                    id: "test-search".into(),
                    item_id: "first".into(),
                },
            ]
        );
    }

    #[gpui::test]
    fn escape_clears_a_query_before_emitting_dismissal(cx: &mut TestAppContext) {
        let (harness, cx) = harness(cx);
        let search = harness.read_with(cx, |harness, _| harness.search.clone());
        cx.update(|window, cx| {
            search.update(cx, |search, cx| search.set_query("report", window, cx));
        });
        cx.run_until_parked();

        activate_key(cx, "escape");
        activate_key(cx, "escape");

        assert_eq!(
            harness.read_with(cx, |harness, _| harness.events.borrow().clone()),
            [
                CommandSearchEvent::QueryChanged {
                    id: "test-search".into(),
                    query: "report".into(),
                },
                CommandSearchEvent::QueryChanged {
                    id: "test-search".into(),
                    query: "".into(),
                },
                CommandSearchEvent::Dismissed {
                    id: "test-search".into(),
                },
            ]
        );
    }

    #[gpui::test]
    fn selected_id_survives_snapshot_reorder_and_moves_the_upstream_highlight(
        cx: &mut TestAppContext,
    ) {
        let (harness, cx) = harness(cx);
        activate_key(cx, "down");
        let search = harness.read_with(cx, |harness, _| harness.search.clone());

        cx.update(|window, cx| {
            search.update(cx, |search, cx| {
                search.set_items(
                    [
                        CommandSearchItem::new("second", "Open report"),
                        CommandSearchItem::new("first", "Open report"),
                    ],
                    window,
                    cx,
                );
            });
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        search.read_with(cx, |search, cx| {
            assert_eq!(search.selected_id, Some("second".into()));
            assert_eq!(
                search.state.read(cx).selected_index(),
                Some(gpui_component::IndexPath::new(0))
            );
        });
    }

    #[gpui::test]
    fn thousand_item_palette_paints_only_a_bounded_visible_range_and_reaches_the_end(
        cx: &mut TestAppContext,
    ) {
        let (harness, cx) = harness(cx);
        let search = harness.read_with(cx, |harness, _| harness.search.clone());
        let items = (0..1_000)
            .map(|index| {
                CommandSearchItem::new(format!("large-{index}"), format!("Large command {index}"))
            })
            .collect::<Vec<_>>();
        cx.update(|window, cx| {
            search.update(cx, |search, cx| search.set_items(items, window, cx));
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));

        let visible = (0..1_000)
            .filter(|index| {
                let selector: &'static str =
                    Box::leak(format!("command-search-item-large-{index}").into_boxed_str());
                cx.debug_bounds(selector).is_some()
            })
            .count();
        assert!(
            visible < 50,
            "only a small visible range should be painted, got {visible}"
        );
        assert!(cx.debug_bounds("command-search-item-large-999").is_none());

        cx.update(|window, cx| {
            search.update(cx, |search, cx| {
                search.state.update(cx, |state, cx| {
                    state.set_selected_index(Some(gpui_component::IndexPath::new(999)), window, cx);
                });
            });
            window.draw(cx).clear(cx);
        });
        cx.run_until_parked();
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("command-search-item-large-999").is_some());
        assert!(cx.debug_bounds("command-search-item-large-0").is_none());
    }
}
