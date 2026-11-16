//! The library sidebar: history, bookmarks and the reading list behind one
//! search field, with bulk selection and frecency-ordered history.

use std::collections::HashSet;

/// The three shelves of the library. One sidebar, not three panels: users
/// think "my stuff", not "three databases".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LibrarySection {
    #[default]
    History,
    Bookmarks,
    ReadingList,
}

impl LibrarySection {
    pub fn title_key(&self) -> &'static str {
        match self {
            LibrarySection::History => "lib.history",
            LibrarySection::Bookmarks => "lib.bookmarks",
            LibrarySection::ReadingList => "lib.reading_list",
        }
    }
}

/// One row on any shelf. `visited_ms` is the epoch-milliseconds timestamp
/// the shelf sorts and formats from.
#[derive(Debug, Clone)]
pub struct LibraryItem {
    pub id: u64,
    pub title: String,
    pub url: String,
    pub visited_ms: i64,
}

/// Bulk action over the current selection. The sidebar resolves the ids and
/// routes the action; storage and tab opening belong to the browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibraryAction {
    Delete,
    OpenAll,
    Bookmark,
}

/// A history row with its frecency score. The multipliers match
/// openfox-storage so the sidebar's order and the omnibox's order agree —
/// two different rankings for the same history reads as a bug.
#[derive(Debug, Clone)]
pub struct FrecencyEntry {
    pub url: String,
    pub title: String,
    pub visits: u32,
    pub last_visit_ms: i64,
    pub score: u32,
}

/// Frecency: visits times a recency multiplier (x4 today, x2 this week,
/// x1 older).
pub fn frecency(visits: u32, last_visit_ms: i64, now_ms: i64) -> u32 {
    const DAY_MS: i64 = 24 * 60 * 60 * 1000;
    const WEEK_MS: i64 = 7 * DAY_MS;
    let age = (now_ms - last_visit_ms).max(0);
    let recency = if age < DAY_MS { 4 } else if age < WEEK_MS { 2 } else { 1 };
    visits * recency
}

/// Relative visit time for row subtitles: "4 minutes ago", never an absolute
/// timestamp — the sidebar is scanned, not read.
pub fn relative_time(then_ms: i64, now_ms: i64) -> String {
    let mins = ((now_ms - then_ms).max(0) / 60_000) as u64;
    match mins {
        0 => "just now".into(),
        1 => "1 minute ago".into(),
        2..=59 => format!("{mins} minutes ago"),
        60..=119 => "1 hour ago".into(),
        120..=1439 => format!("{} hours ago", mins / 60),
        1440..=2879 => "yesterday".into(),
        _ => format!("{} days ago", mins / 1440),
    }
}

/// The sidebar model: current shelf, live search text, rows, and the bulk
/// selection. Rows arrive from storage; selection is chrome-owned.
#[derive(Debug, Clone, Default)]
pub struct LibrarySidebar {
    pub section: LibrarySection,
    pub search: String,
    pub items: Vec<LibraryItem>,
    pub selected: HashSet<u64>,
    pub history: Vec<FrecencyEntry>,
}

impl LibrarySidebar {
    /// Every keystroke filters; matching on title or URL, case-insensitive.
    pub fn set_search(&mut self, text: impl Into<String>) {
        self.search = text.into();
        self.selected.clear();
    }

    /// Rows that survive the current filter.
    pub fn visible(&self) -> Vec<&LibraryItem> {
        let needle = self.search.to_lowercase();
        self.items
            .iter()
            .filter(|item| {
                needle.is_empty()
                    || item.title.to_lowercase().contains(&needle)
                    || item.url.to_lowercase().contains(&needle)
            })
            .collect()
    }

    pub fn select(&mut self, id: u64, on: bool) {
        if on {
            self.selected.insert(id);
        } else {
            self.selected.remove(&id);
        }
    }

    pub fn select_visible(&mut self) {
        let ids: Vec<u64> = self.visible().iter().map(|item| item.id).collect();
        for id in ids {
            self.selected.insert(id);
        }
    }

    /// Resolves a bulk action to the ids it applies to and drops them from
    /// the local rows so the panel reacts instantly; storage is the source
    /// of truth and will re-emit on the next sync.
    pub fn apply(&mut self, action: LibraryAction) -> Vec<u64> {
        let ids: Vec<u64> = self.selected.iter().copied().collect();
        match action {
            LibraryAction::Delete | LibraryAction::Bookmark => {
                self.items.retain(|item| !self.selected.contains(&item.id));
            }
            LibraryAction::OpenAll => {}
        }
        self.selected.clear();
        ids
    }

    /// Rebuilds the history shelf's ordering from fresh timestamps.
    pub fn sort_history_by_frecency(&mut self, now_ms: i64) {
        for entry in &mut self.history {
            entry.score = frecency(entry.visits, entry.last_visit_ms, now_ms);
        }
        self.history.sort_by(|a, b| b.score.cmp(&a.score).then(b.last_visit_ms.cmp(&a.last_visit_ms)));
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

use crate::{icons, theme, widgets};
use rune::{Color, El, ScrollSpec, col, el, row, text, tok};
use rune::tok::{motion, type_};

const PANEL_W: f32 = 340.0;

/// The library sidebar as a panel: section pills on top, a sticky search
/// header, and rows split by hairlines. One shelf shows at a time; the
/// history shelf reads its frecency order, the others the filtered items.
/// Handlers run against `LibrarySidebar` itself — the surface the browser
/// mounts this panel on owns the model.
pub fn view(s: &LibrarySidebar, dark: bool) -> El {
    let t = tok::theme(dark);
    col()
        .key("library")
        .role("region")
        .label("Library")
        .w(PANEL_W)
        .h_full()
        .fill(t.card)
        .border_sides([0.0, 1.0, 0.0, 0.0], t.separator)
        .child(
            row()
                .key("library-sections")
                .role("tablist")
                .label("Library sections")
                .px(tok::space::S3)
                .py(tok::space::S2)
                .gap(6.0)
                .items_center()
                .child(section_tab(s, LibrarySection::History, "History", dark))
                .child(section_tab(s, LibrarySection::Bookmarks, "Bookmarks", dark))
                .child(section_tab(s, LibrarySection::ReadingList, "Reading list", dark))
                .child(el().key("library-tail").grow(1.0)),
        )
        .child(widgets::hairline(dark))
        .child(list_area(s, dark))
}

/// One section pill bound to the sidebar's current shelf.
fn section_tab(s: &LibrarySidebar, section: LibrarySection, label: &str, dark: bool) -> El {
    let active = s.section == section;
    widgets::section_pill(
        format!("library-{}", section.title_key()),
        label,
        active,
        dark,
        move |s: &mut LibrarySidebar| s.section = section,
    )
}

/// The scrolling shelf: a sticky search header, then hairline-split rows.
fn list_area(s: &LibrarySidebar, dark: bool) -> El {
    let t = tok::theme(dark);
    let mut rows = col().key("library-rows");
    match s.section {
        LibrarySection::History => {
            for entry in s.history.iter() {
                rows = rows.child(history_row(entry, dark));
            }
        }
        LibrarySection::Bookmarks | LibrarySection::ReadingList => {
            let mut shown = col().key("library-shown");
            for item in s.visible() {
                shown = shown.child(item_row(item, s.selected.contains(&item.id), dark));
            }
            rows = rows.child(shown);
        }
    }
    if s.items.is_empty() && s.history.is_empty() {
        rows = rows.child(
            el()
                .key("library-empty")
                .px(tok::space::S3)
                .py(tok::space::S6)
                .child(text("Nothing here yet.").font(type_::BODY).color(t.text_secondary)),
        );
    }
    el()
        .key("library-scroll")
        .w_full()
        .grow(1.0)
        .scroll(ScrollSpec { stick_bottom: false })
        .child(
            col()
                .key("library-content")
                .child(
                    // The search header sticks to the top of the scroll and
                    // gains its shadow only while stuck.
                    el()
                        .key("library-search")
                        .role("search")
                        .label("Search library")
                        .w_full()
                        .px(tok::space::S3)
                        .py(tok::space::S2)
                        .fill(t.card)
                        .sticky_top(0.0)
                        .stuck_shadow(widgets::sticky_header(dark))
                        .child(
                            row()
                                .gap(tok::space::S2)
                                .items_center()
                                .child(icons::glyph(&icons::SEARCH, 13.0, t.text_secondary))
                                .child(
                                    text(if s.search.is_empty() { "Search".to_owned() } else { s.search.clone() })
                                        .font(type_::BODY)
                                        .color(if s.search.is_empty() { t.text_secondary } else { t.text })
                                        .truncate(),
                                ),
                        ),
                )
                .child(rows),
        )
}

/// One item row (bookmarks, reading list): monogram dot, title, host
/// caption; hairline below; press toggles selection, `checked` reports it.
fn item_row(item: &LibraryItem, selected: bool, dark: bool) -> El {
    let t = tok::theme(dark);
    let id = item.id;
    col()
        .key(format!("item-{}", item.id))
        .child(
            row()
                .key("item-row")
                .role("option")
                .label(item.title.clone())
                .checked(selected)
                .focusable(true)
                .w_full()
                .px(tok::space::S3)
                .py(9.0)
                .gap(tok::space::S2)
                .items_center()
                .fill(if selected { t.hover } else { Color::TRANSPARENT })
                .hover(t.hover)
                .press(t.press)
                .motion(motion::SNAPPY)
                .on_press(move |s: &mut LibrarySidebar| s.select(id, !s.selected.contains(&id)))
                .child(
                    el()
                        .key("item-dot")
                        .size(8.0, 8.0)
                        .radius(4.0)
                        .fill(if selected { theme::accent(dark) } else { t.control_off }),
                )
                .child(
                    col()
                        .key("item-text")
                        .grow(1.0)
                        .gap(1.0)
                        .child(text(item.title.clone()).font(type_::LABEL).color(t.text).truncate())
                        .child(text(host_of(&item.url)).font(type_::SECTION).color(t.text_secondary).truncate()),
                ),
        )
        .child(widgets::hairline(dark))
}

/// One frecency-ordered history row: title, host, score caption.
fn history_row(entry: &FrecencyEntry, dark: bool) -> El {
    let t = tok::theme(dark);
    col()
        .key(format!("history-{}", entry.url))
        .child(
            row()
                .key("history-row")
                .role("option")
                .label(entry.title.clone())
                .focusable(true)
                .w_full()
                .px(tok::space::S3)
                .py(9.0)
                .gap(tok::space::S2)
                .items_center()
                .hover(t.hover)
                .press(t.press)
                .motion(motion::SNAPPY)
                .child(
                    col()
                        .key("history-text")
                        .grow(1.0)
                        .gap(1.0)
                        .child(
                            text(if entry.title.is_empty() { entry.url.clone() } else { entry.title.clone() })
                                .font(type_::LABEL)
                                .color(t.text)
                                .truncate(),
                        )
                        .child(text(host_of(&entry.url)).font(type_::SECTION).color(t.text_secondary).truncate()),
                )
                .child(text(entry.score.to_string()).font(type_::MONO_CAPTION).color(t.text_secondary)),
        )
        .child(widgets::hairline(dark))
}

/// The host of a URL for row captions, or the URL itself when it has none.
fn host_of(url: &str) -> String {
    let rest = url.trim_start_matches("https://").trim_start_matches("http://");
    let host = rest.split('/').next().unwrap_or(rest);
    host.to_owned()
}
