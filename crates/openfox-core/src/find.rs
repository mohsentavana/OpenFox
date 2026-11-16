//! Find in page: an index over the tab's text runs, match navigation
//! with wrap, case and whole-word options. Rect production belongs to
//! the engine — this module finds, it does not paint.

#[derive(Debug, Clone, serde::Serialize)]
pub struct TextRun {
    pub text: String,
    pub node: usize,
    pub start: u16,
    pub end: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    pub run_index: usize,
    pub offset: usize,
    pub length: usize,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Next,
    Previous,
}

pub struct FindInPage {
    query: String,
    case_sensitive: bool,
    whole_word: bool,
    matches: Vec<Match>,
    current: usize,
}

impl Default for FindInPage {
    fn default() -> Self {
        FindInPage {
            query: String::new(),
            case_sensitive: false,
            whole_word: false,
            matches: Vec::new(),
            current: 0,
        }
    }
}

impl FindInPage {
    /// Incremental search: re-run on every keystroke, because Firefox
    /// muscle memory is "type and watch it jump," not "type, press
    /// enter, squint."
    pub fn search(&mut self, query: &str, runs: &[TextRun]) -> usize {
        self.query = query.to_owned();
        self.matches.clear();
        self.current = 0;
        if query.is_empty() {
            return 0;
        }
        let (needle, match_case): (String, bool) = if self.case_sensitive {
            (query.to_owned(), true)
        } else {
            // ASCII lowering: the engine's text layer folds full Unicode
            // when producing runs; here the pragmatic case is the common
            // one and the honest one.
            (query.to_ascii_lowercase(), false)
        };

        for (run_index, run) in runs.iter().enumerate() {
            let haystack = if match_case {
                run.text.clone()
            } else {
                run.text.to_ascii_lowercase()
            };
            let mut from = 0;
            while let Some(found) = haystack[from..].find(&needle) {
                let at = from + found;
                if self.whole_word && !word_bounded(&haystack, at, needle.len()) {
                    from = at + needle.len().max(1);
                    continue;
                }
                self.matches.push(Match {
                    run_index,
                    offset: at,
                    length: needle.len(),
                });
                from = at + needle.len().max(1);
            }
        }
        self.matches.len()
    }

    pub fn match_count(&self) -> usize {
        self.matches.len()
    }

    /// The 1-based position of the current match, for "3 of 17".
    pub fn position(&self) -> Option<(usize, usize)> {
        if self.matches.is_empty() {
            None
        } else {
            Some((self.current + 1, self.matches.len()))
        }
    }

    /// Walk matches with wrap-around. Wrapping is a feature, not a bug:
    /// the user asked for the next match, and the page is finite.
    pub fn navigate(&mut self, direction: Direction) -> Option<&Match> {
        if self.matches.is_empty() {
            return None;
        }
        self.current = match direction {
            Direction::Next => (self.current + 1) % self.matches.len(),
            Direction::Previous => {
                if self.current == 0 {
                    self.matches.len() - 1
                } else {
                    self.current - 1
                }
            }
        };
        self.matches.get(self.current)
    }

    pub fn current_match(&self) -> Option<&Match> {
        self.matches.get(self.current)
    }

    pub fn set_case_sensitive(&mut self, case_sensitive: bool) {
        self.case_sensitive = case_sensitive;
    }

    pub fn set_whole_word(&mut self, whole_word: bool) {
        self.whole_word = whole_word;
    }

    pub fn query(&self) -> &str {
        &self.query
    }
}

/// Word-boundary check on ASCII text: alphanumerics on both sides of
/// the hit mean it is inside a word, not a word.
fn word_bounded(haystack: &str, at: usize, len: usize) -> bool {
    let before_ok = at == 0
        || !haystack
            .as_bytes()
            .get(at - 1)
            .is_some_and(u8::is_ascii_alphanumeric);
    let after = at + len;
    let after_ok = after >= haystack.len()
        || !haystack
            .as_bytes()
            .get(after)
            .is_some_and(u8::is_ascii_alphanumeric);
    before_ok && after_ok
}
