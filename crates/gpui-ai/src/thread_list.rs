//! Conversation list: new, switch, rename, archive, delete, and search.
//!
//! The application owns the threads (grouped into sections such as Today /
//! Yesterday / Earlier) and the active thread ID; the list owns only its
//! native search input, the "show archived" toggle, and which row has its
//! actions expanded. Every intent — selecting, creating, renaming,
//! archiving, deleting — is a typed [`ThreadListEvent`] keyed by stable ID.

use crate::cues::{self, Cue};
use crate::{
    control::{composed_button, outlined_control},
    motion::{reveal, reveal_staggered},
    surface::{eyebrow, icon_button},
    theme::SemanticStyledExt as _,
};
use gpui::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    ScrollHandle, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window,
    div, prelude::FluentBuilder as _,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Sizable as _,
    button::Button,
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement as _,
    v_flex,
};
use std::sync::Arc;

/// One conversation with stable identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadItem {
    id: SharedString,
    title: SharedString,
    subtitle: Option<SharedString>,
    archived: bool,
}

impl ThreadItem {
    /// Creates a thread with a stable identifier and visible title.
    pub fn new(id: impl Into<SharedString>, title: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            subtitle: None,
            archived: false,
        }
    }

    /// Adds secondary text (a timestamp or the last message).
    pub fn subtitle(mut self, subtitle: impl Into<SharedString>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Marks the thread archived; archived threads hide until requested.
    pub fn archived(mut self, archived: bool) -> Self {
        self.archived = archived;
        self
    }

    /// Returns the stable thread identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible title.
    pub fn title(&self) -> &SharedString {
        &self.title
    }

    /// Returns the secondary text, when present.
    pub fn subtitle_text(&self) -> Option<&SharedString> {
        self.subtitle.as_ref()
    }

    /// Whether the thread is archived.
    pub fn is_archived(&self) -> bool {
        self.archived
    }

    fn matches(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let needle = query.to_lowercase();
        self.title.to_lowercase().contains(&needle)
            || self
                .subtitle
                .as_ref()
                .is_some_and(|subtitle| subtitle.to_lowercase().contains(&needle))
    }
}

/// A labeled group of threads (for example "Today").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadSection {
    id: SharedString,
    label: SharedString,
    items: Vec<ThreadItem>,
}

impl ThreadSection {
    /// Creates a section with a stable identifier and visible label.
    pub fn new(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            items: Vec::new(),
        }
    }

    /// Sets the section's threads, in display order.
    pub fn items(mut self, items: impl IntoIterator<Item = ThreadItem>) -> Self {
        self.items = items.into_iter().collect();
        self
    }

    /// Returns the stable section identifier.
    pub fn id(&self) -> &SharedString {
        &self.id
    }

    /// Returns the visible label.
    pub fn label(&self) -> &SharedString {
        &self.label
    }

    /// Returns the section's threads.
    pub fn thread_items(&self) -> &[ThreadItem] {
        &self.items
    }
}

/// An interaction emitted by [`ThreadList`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadListEvent {
    /// The user chose a thread.
    Selected {
        /// Stable thread identifier.
        id: SharedString,
    },
    /// The user asked for a new conversation.
    NewRequested,
    /// The user wants to rename a thread.
    RenameRequested {
        /// Stable thread identifier.
        id: SharedString,
    },
    /// The user wants to archive a thread.
    ArchiveRequested {
        /// Stable thread identifier.
        id: SharedString,
    },
    /// The user wants to restore an archived thread.
    UnarchiveRequested {
        /// Stable thread identifier.
        id: SharedString,
    },
    /// The user wants to delete a thread.
    DeleteRequested {
        /// Stable thread identifier.
        id: SharedString,
    },
    /// The search query changed.
    QueryChanged {
        /// The complete current query.
        query: SharedString,
    },
}

fn sections_are_well_formed(sections: &[ThreadSection]) -> bool {
    let mut section_ids = std::collections::HashSet::new();
    let mut item_ids = std::collections::HashSet::new();
    sections.iter().all(|section| {
        section_ids.insert(&section.id)
            && section.items.iter().all(|item| item_ids.insert(&item.id))
    })
}

/// A controlled conversation list with native search.
///
/// # Example
///
/// ```ignore
/// let threads = cx.new(|cx| ThreadList::new("threads", window, cx));
/// threads.update(cx, |threads, cx| {
///     threads.set_sections(
///         [ThreadSection::new("today", "Today").items([
///             ThreadItem::new("t-1", "Supplier pricing").subtitle("2 min ago"),
///         ])],
///         cx,
///     );
///     threads.set_active(Some("t-1"), cx);
/// });
/// ```
pub struct ThreadList {
    id: SharedString,
    sections: Arc<[ThreadSection]>,
    active: Option<SharedString>,
    query: SharedString,
    input: Entity<InputState>,
    show_archived: bool,
    open_actions: Option<SharedString>,
    scroll: ScrollHandle,
    _input_subscription: Subscription,
}

impl ThreadList {
    /// Creates an empty list around a native search input.
    pub fn new(id: impl Into<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Search conversations"));
        let subscription =
            cx.subscribe_in(&input, window, |this, input, event: &InputEvent, _, cx| {
                if let InputEvent::Change = event {
                    let query: SharedString = input.read(cx).value().to_string().into();
                    this.apply_query(query, cx);
                }
            });
        Self {
            id: id.into(),
            sections: Arc::from([]),
            active: None,
            query: "".into(),
            input,
            show_archived: false,
            open_actions: None,
            scroll: ScrollHandle::new(),
            _input_subscription: subscription,
        }
    }

    /// Replaces the controlled sections. Snapshots with duplicate section or
    /// thread IDs are ignored so identities can never alias.
    pub fn set_sections(
        &mut self,
        sections: impl IntoIterator<Item = ThreadSection>,
        cx: &mut Context<Self>,
    ) {
        let sections: Arc<[ThreadSection]> = sections.into_iter().collect();
        if !sections_are_well_formed(&sections) || self.sections == sections {
            return;
        }
        let retained = |id: &Option<SharedString>| {
            id.as_ref().is_some_and(|id| {
                sections
                    .iter()
                    .any(|section| section.items.iter().any(|item| &item.id == id))
            })
        };
        if !retained(&self.open_actions) {
            self.open_actions = None;
        }
        self.sections = sections;
        cx.notify();
    }

    /// Sets the active thread by stable ID (or none).
    pub fn set_active(&mut self, id: Option<impl Into<SharedString>>, cx: &mut Context<Self>) {
        let id = id.map(Into::into);
        if self.active != id {
            self.active = id;
            cx.notify();
        }
    }

    /// Returns the active thread ID.
    pub fn active_id(&self) -> Option<&SharedString> {
        self.active.as_ref()
    }

    /// Shows or hides archived threads.
    pub fn set_show_archived(&mut self, show: bool, cx: &mut Context<Self>) {
        if self.show_archived != show {
            self.show_archived = show;
            cx.notify();
        }
    }

    /// Whether archived threads are shown.
    pub fn shows_archived(&self) -> bool {
        self.show_archived
    }

    /// Sets the search query programmatically.
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
        self.input.update(cx, |input, cx| {
            input.set_value(query.to_string(), window, cx)
        });
        // InputState suppresses Change for programmatic values.
        self.apply_query(query, cx);
    }

    /// Returns the current search query.
    pub fn query(&self) -> &SharedString {
        &self.query
    }

    /// Moves keyboard focus into the search input.
    pub fn focus_search(&self, window: &mut Window, cx: &mut App) {
        self.input.read(cx).focus_handle(cx).focus(window, cx);
    }

    /// Scrolls the list to its end (the oldest visible thread).
    pub fn scroll_to_end(&mut self, cx: &mut Context<Self>) {
        self.scroll.scroll_to_bottom();
        cx.notify();
    }

    fn apply_query(&mut self, query: SharedString, cx: &mut Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query.clone();
        cx.emit(ThreadListEvent::QueryChanged { query });
        cx.notify();
    }

    fn visible_sections(&self) -> Vec<(&ThreadSection, Vec<&ThreadItem>)> {
        self.sections
            .iter()
            .filter_map(|section| {
                let items: Vec<&ThreadItem> = section
                    .items
                    .iter()
                    .filter(|item| {
                        (self.show_archived || !item.archived) && item.matches(&self.query)
                    })
                    .collect();
                (!items.is_empty()).then_some((section, items))
            })
            .collect()
    }

    fn total_visible_candidates(&self) -> usize {
        self.sections
            .iter()
            .flat_map(|section| section.items.iter())
            .filter(|item| self.show_archived || !item.archived)
            .count()
    }

    fn render_item(
        &self,
        item: &ThreadItem,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tokens = cx.theme().semantic_tokens();
        let root_id = ElementId::from(self.id.clone());
        let item_id = ElementId::from((root_id.clone(), item.id.clone()));
        let selected = self.active.as_ref() == Some(&item.id);
        let actions_open = self.open_actions.as_ref() == Some(&item.id);
        let accessibility_label: SharedString = if item.archived {
            format!("{}, archived", item.title).into()
        } else {
            item.title.clone()
        };
        let debug_id = item.id.to_string();
        let more_debug_id = item.id.to_string();
        let select_id = item.id.clone();
        let toggle_id = item.id.clone();

        let row = h_flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(tokens.spacing.xxs)
            .child(
                composed_button((item_id.clone(), "select"), accessibility_label)
                    .debug_selector(move || format!("thread-{debug_id}"))
                    .role(Role::ListBoxOption)
                    .selected(selected)
                    .aria_selected(selected)
                    .flex_1()
                    .min_w_0()
                    .px(tokens.spacing.sm)
                    .py(tokens.spacing.xs)
                    .rounded(tokens.radius.md)
                    .border_l_2()
                    .border_color(if selected {
                        cx.theme().primary
                    } else {
                        cx.theme().transparent
                    })
                    .bg(if selected {
                        cx.theme().accent
                    } else {
                        cx.theme().transparent
                    })
                    .hover(|style| style.bg(cx.theme().accent.opacity(0.6)))
                    .active(|style| style.bg(cx.theme().accent))
                    .focus_visible(|style| style.border_color(cx.theme().ring))
                    .child(
                        v_flex()
                            .min_w_0()
                            .items_start()
                            .child(
                                div()
                                    .w_full()
                                    .truncate()
                                    .text_token(tokens.typography.sm)
                                    .text_color(cx.theme().foreground)
                                    .child(item.title.clone()),
                            )
                            .when_some(item.subtitle.clone(), |this, subtitle| {
                                this.child(
                                    div()
                                        .w_full()
                                        .truncate()
                                        .text_token(tokens.typography.xs)
                                        .text_color(cx.theme().muted_foreground)
                                        .child(subtitle),
                                )
                            }),
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(ThreadListEvent::Selected {
                            id: select_id.clone(),
                        });
                        cues::emit(cx, Cue::ThreadSelected);
                    })),
            )
            .child(
                icon_button(
                    (item_id.clone(), "more"),
                    IconName::Ellipsis,
                    format!("More actions for {}", item.title),
                    cx,
                )
                .debug_selector(move || format!("thread-more-{more_debug_id}"))
                .aria_expanded(actions_open)
                .selected(actions_open)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.open_actions = if this.open_actions.as_ref() == Some(&toggle_id) {
                        None
                    } else {
                        Some(toggle_id.clone())
                    };
                    cx.notify();
                })),
            );

        let actions = actions_open.then(|| {
            let rename_id = item.id.clone();
            let archive_id = item.id.clone();
            let delete_id = item.id.clone();
            let archived = item.archived;
            let rename_debug = item.id.to_string();
            let archive_debug = item.id.to_string();
            let delete_debug = item.id.to_string();
            reveal(
                h_flex()
                    .id((item_id.clone(), "actions"))
                    .role(Role::Toolbar)
                    .aria_label(format!("Actions for {}", item.title))
                    .gap(tokens.spacing.xs)
                    .pl(tokens.spacing.sm)
                    .pb(tokens.spacing.xxs)
                    .child(
                        outlined_control((item_id.clone(), "rename"), "Rename", cx)
                            .debug_selector(move || format!("thread-rename-{rename_debug}"))
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(ThreadListEvent::RenameRequested {
                                    id: rename_id.clone(),
                                });
                            })),
                    )
                    .child(
                        outlined_control(
                            (item_id.clone(), "archive"),
                            if archived { "Unarchive" } else { "Archive" },
                            cx,
                        )
                        .debug_selector(move || format!("thread-archive-{archive_debug}"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.open_actions = None;
                            cx.emit(if archived {
                                ThreadListEvent::UnarchiveRequested {
                                    id: archive_id.clone(),
                                }
                            } else {
                                ThreadListEvent::ArchiveRequested {
                                    id: archive_id.clone(),
                                }
                            });
                            cx.notify();
                        })),
                    )
                    .child(
                        outlined_control((item_id.clone(), "delete"), "Delete", cx)
                            .debug_selector(move || format!("thread-delete-{delete_debug}"))
                            .text_color(cx.theme().danger)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.open_actions = None;
                                cx.emit(ThreadListEvent::DeleteRequested {
                                    id: delete_id.clone(),
                                });
                                cx.notify();
                            })),
                    ),
                (item_id.clone(), "actions-reveal"),
                window,
                cx,
            )
        });

        reveal_staggered(
            v_flex()
                .id((item_id, "row"))
                .w_full()
                .min_w_0()
                .gap(tokens.spacing.xxs)
                .child(row)
                .children(actions),
            (root_id, format!("reveal-{}", item.id)),
            index,
            window,
            cx,
        )
        .into_any_element()
    }
}

impl EventEmitter<ThreadListEvent> for ThreadList {}

impl Focusable for ThreadList {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl Render for ThreadList {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let root_id = ElementId::from(self.id.clone());
        let visible = self.visible_sections();
        let candidates = self.total_visible_candidates();
        let show_archived = self.show_archived;
        let empty_message: Option<SharedString> = if candidates == 0 {
            Some("No conversations yet".into())
        } else if visible.is_empty() {
            Some(format!("No conversations match “{}”", self.query).into())
        } else {
            None
        };
        let mut index = 0usize;
        let mut sections = Vec::new();
        for (section, items) in &visible {
            let mut rows = Vec::with_capacity(items.len());
            for item in items {
                rows.push(self.render_item(item, index, window, cx));
                index += 1;
            }
            sections.push(
                v_flex()
                    .id((root_id.clone(), section.id.clone()))
                    .role(Role::Group)
                    .aria_label(section.label.clone())
                    .w_full()
                    .min_w_0()
                    .gap(tokens.spacing.xxs)
                    .child(eyebrow(section.label.clone(), cx).px(tokens.spacing.sm))
                    .children(rows),
            );
        }

        v_flex()
            .id(root_id.clone())
            .role(Role::Group)
            .aria_label("Conversations")
            .size_full()
            .min_h_0()
            .min_w_0()
            .gap(tokens.spacing.sm)
            .child(
                h_flex()
                    .w_full()
                    .items_center()
                    .gap(tokens.spacing.xs)
                    .child(
                        div().debug_selector(|| "thread-list-new".into()).child(
                            Button::new((root_id.clone(), "new"))
                                .outline()
                                .small()
                                .icon(IconName::Plus)
                                .label("New chat")
                                .accessibility_id(format!("{}-new", self.id))
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.emit(ThreadListEvent::NewRequested);
                                })),
                        ),
                    )
                    .child(div().flex_1())
                    .child(
                        icon_button(
                            (root_id.clone(), "archived"),
                            IconName::Inbox,
                            if show_archived {
                                "Hide archived conversations"
                            } else {
                                "Show archived conversations"
                            },
                            cx,
                        )
                        .debug_selector(|| "thread-list-archived-toggle".into())
                        .selected(show_archived)
                        .aria_expanded(show_archived)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_archived = !this.show_archived;
                            cx.notify();
                        })),
                    ),
            )
            .child(
                h_flex()
                    .debug_selector(|| "thread-list-search".into())
                    .w_full()
                    .items_center()
                    .gap(tokens.spacing.xs)
                    .px(tokens.spacing.xs)
                    .child(
                        Icon::new(IconName::Search)
                            .xsmall()
                            .text_color(cx.theme().muted_foreground),
                    )
                    .child(div().flex_1().min_w_0().child(Input::new(&self.input))),
            )
            .child(
                // The outer frame owns the flex constraint; the inner
                // scroll area fills it so long lists scroll instead of
                // growing the pane.
                div()
                    .id((root_id.clone(), "list-frame"))
                    .debug_selector(|| "thread-list-scroll".into())
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_hidden()
                    .vertical_scrollbar(&self.scroll)
                    .child(
                        v_flex()
                            .id((root_id.clone(), "list"))
                            .role(Role::ListBox)
                            .aria_label("Conversation list")
                            .size_full()
                            .gap(tokens.spacing.md)
                            .overflow_y_scroll()
                            .track_scroll(&self.scroll)
                            .children(sections)
                            .when_some(empty_message, |this, message| {
                                this.child(
                                    div()
                                        .id((root_id, "empty"))
                                        .debug_selector(|| "thread-list-empty".into())
                                        .role(Role::Status)
                                        .aria_label(message.clone())
                                        .p(tokens.spacing.md)
                                        .text_token(tokens.typography.sm)
                                        .text_color(cx.theme().muted_foreground)
                                        .child(message),
                                )
                            }),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_is_case_insensitive_over_title_and_subtitle() {
        let item = ThreadItem::new("t", "Supplier pricing").subtitle("Compared Alpenrose");
        assert!(item.matches(""));
        assert!(item.matches("PRICING"));
        assert!(item.matches("alpenrose"));
        assert!(!item.matches("delivery"));
    }

    #[test]
    fn malformed_snapshots_are_rejected() {
        let duplicate_items = [ThreadSection::new("a", "A")
            .items([ThreadItem::new("t", "One"), ThreadItem::new("t", "Two")])];
        assert!(!sections_are_well_formed(&duplicate_items));
        let duplicate_sections = [
            ThreadSection::new("a", "A"),
            ThreadSection::new("a", "Again"),
        ];
        assert!(!sections_are_well_formed(&duplicate_sections));
        let fine = [
            ThreadSection::new("a", "A").items([ThreadItem::new("t-1", "One")]),
            ThreadSection::new("b", "B").items([ThreadItem::new("t-2", "Two")]),
        ];
        assert!(sections_are_well_formed(&fine));
    }
}
