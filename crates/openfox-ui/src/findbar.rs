//! The find bar: query, match counts, and the intents that walk them. The
//! view is a bottom-floating rounded panel that slides in — Rune mounts it
//! with an offset+opacity transition, so no timer or animation state exists
//! here.

/// What a find-bar interaction asks for. Next and Prev wrap around: a
/// user hunting a term reads the matches as a ring, not a list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindIntent {
    Next,
    Prev,
    Reset,
    ToggleHighlightAll,
    ToggleCase,
}

/// Find state for one tab. The engine owns the actual search; this is the
/// chrome's honest mirror of it.
#[derive(Debug, Clone)]
pub struct FindBar {
    pub query: String,
    /// Total matches for the current query.
    pub matches: usize,
    /// Zero-based index of the current match.
    pub current: usize,
    pub highlight_all: bool,
    pub case_sensitive: bool,
    pub visible: bool,
}

impl Default for FindBar {
    fn default() -> FindBar {
        // Highlight-all on by default: seeing every hit is how the feature
        // earns its keep.
        FindBar {
            query: String::new(),
            matches: 0,
            current: 0,
            highlight_all: true,
            case_sensitive: false,
            visible: false,
        }
    }
}

impl FindBar {
    pub fn open(&mut self) {
        self.visible = true;
        // The previous query stays: repeated searches are the norm, and
        // Firefox muscle memory expects it.
    }

    /// Esc closes the bar and clears the highlights, but keeps the query —
    /// the next Ctrl-F should offer the same term.
    pub fn dismiss(&mut self) {
        self.visible = false;
        self.matches = 0;
        self.current = 0;
    }

    /// Incremental search: every keystroke restarts the search from the
    /// first match. Matching Firefox here matters more than saving a
    /// search — users type without looking at the bar and expect the page
    /// to react per character.
    pub fn type_char(&mut self, ch: char) {
        self.query.push(ch);
        self.current = 0;
    }

    pub fn backspace(&mut self) {
        self.query.pop();
        self.current = 0;
    }

    /// The engine reports a fresh result after each search.
    pub fn result(&mut self, matches: usize) {
        self.matches = matches;
        if self.current >= matches {
            self.current = 0;
        }
    }

    pub fn apply(&mut self, intent: FindIntent) {
        match intent {
            FindIntent::Next => {
                if self.matches > 0 {
                    self.current = (self.current + 1) % self.matches;
                }
            }
            FindIntent::Prev => {
                if self.matches > 0 {
                    self.current = (self.current + self.matches - 1) % self.matches;
                }
            }
            FindIntent::Reset => {
                self.query.clear();
                self.matches = 0;
                self.current = 0;
            }
            FindIntent::ToggleHighlightAll => self.highlight_all = !self.highlight_all,
            FindIntent::ToggleCase => {
                self.case_sensitive = !self.case_sensitive;
                self.current = 0;
            }
        }
    }

    /// "3 of 17" — one-based, because that is what every browser shows.
    /// An empty query shows nothing; a missed query says so in plain words.
    pub fn count_display(&self) -> String {
        if self.query.is_empty() {
            return String::new();
        }
        if self.matches == 0 {
            return "Phrase not found".into();
        }
        format!("{} of {}", self.current + 1, self.matches)
    }
}

/// The find bar as the chrome mounts it: pinned to the page area's
/// bottom-right corner, raised above the page, sliding up 8 pt as it fades
/// in. Match count rides in `MONO_CAPTION`; prev/next/close are the standard
/// 32 pt icon buttons.
pub fn view(s: &crate::chrome::ChromeState) -> rune::El {
    use crate::icons;
    use crate::widgets;
    use rune::{el, row, text, tok, Transition};
    use rune::tok::{motion, type_};

    let t = tok::theme(s.dark);
    let find = &s.find;
    let query = if find.query.is_empty() { "Find in page".to_owned() } else { find.query.clone() };

    row()
        .key("findbar")
        .role("search")
        .label("Find in page")
        .pin(None, None, Some(tok::space::S3), Some(tok::space::S3))
        .raise()
        .h(40.0)
        .px(tok::space::S2)
        .gap(tok::space::S2)
        .items_center()
        .radius(tok::radius::LG)
        .fill(t.card)
        .border(1.0, t.card_border)
        .shadow(6.0, 20.0, t.shadow)
        .transition(Transition::offset(0.0, 8.0).combined(Transition::opacity()))
        .motion(motion::SNAPPY)
        .child(icons::glyph(&icons::SEARCH, 13.0, t.text_secondary))
        .child(
            el()
                .key("findbar-query")
                .min_w(140.0)
                .child(text(query).font(type_::BODY).color(t.text).truncate()),
        )
        .child(
            el()
                .key("findbar-count")
                .min_w(72.0)
                .items_center()
                .justify_end()
                .child(text(find.count_display()).font(type_::MONO_CAPTION).color(t.text_secondary)),
        )
        .child(widgets::icon_button(
            "findbar-prev",
            &icons::CHEVRON_UP,
            "Previous match",
            s.dark,
            true,
            |s: &mut crate::chrome::ChromeState| s.find.apply(FindIntent::Prev),
        ))
        .child(widgets::icon_button(
            "findbar-next",
            &icons::CHEVRON_DOWN,
            "Next match",
            s.dark,
            true,
            |s: &mut crate::chrome::ChromeState| s.find.apply(FindIntent::Next),
        ))
        .child(widgets::icon_button(
            "findbar-close",
            &icons::CLOSE,
            "Close find bar",
            s.dark,
            true,
            |s: &mut crate::chrome::ChromeState| s.find.dismiss(),
        ))
}
