//! The chrome's glyph catalog: SVG icons on a 16-unit viewBox, drawn in the
//! element's colour (`stroke="currentColor"`). A browser chrome needs about a
//! dozen glyphs; owning them keeps the crate free of an icon-font dependency
//! and every glyph crisp at 10–16 pt.

use rune::{Color, El, Icon, icon};

pub static BACK: Icon = Icon {
    name: "fox-back",
    view: 16.0,
    body: r#"<path d="M10 3L5 8l5 5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub static FORWARD: Icon = Icon {
    name: "fox-forward",
    view: 16.0,
    body: r#"<path d="M6 3l5 5-5 5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub static RELOAD: Icon = Icon {
    name: "fox-reload",
    view: 16.0,
    body: r#"<path d="M8 2.5a5.5 5.5 0 1 1-4.7 2.7" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/><path d="M3 1.5v4h4" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub static CLOSE: Icon = Icon {
    name: "fox-close",
    view: 16.0,
    body: r#"<path d="M4 4l8 8M12 4l-8 8" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
};
pub static PLUS: Icon = Icon {
    name: "fox-plus",
    view: 16.0,
    body: r#"<path d="M8 3.5v9M3.5 8h9" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
};
pub static SEARCH: Icon = Icon {
    name: "fox-search",
    view: 16.0,
    body: r#"<circle cx="7" cy="7" r="4.5" fill="none" stroke="currentColor" stroke-width="1.6"/><path d="M10.5 10.5L13.5 13.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
};
pub static SHIELD: Icon = Icon {
    name: "fox-shield",
    view: 16.0,
    body: r#"<path d="M8 1.8l5.2 2v4.1c0 3.2-2.2 5.6-5.2 6.4-3-.8-5.2-3.2-5.2-6.4V3.8z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#,
};
pub static READER: Icon = Icon {
    name: "fox-reader",
    view: 16.0,
    body: r#"<path d="M2.5 4.5c2-1.3 4-1.3 5.5 0 1.5-1.3 3.5-1.3 5.5 0v7c-2-1.3-4-1.3-5.5 0-1.5-1.3-3.5-1.3-5.5 0z" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linejoin="round"/><path d="M8 4.5v7" fill="none" stroke="currentColor" stroke-width="1.4"/>"#,
};
pub static DOWNLOAD: Icon = Icon {
    name: "fox-download",
    view: 16.0,
    body: r#"<path d="M8 2.5v7.5M4.5 6.8L8 10.3l3.5-3.5" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/><path d="M2.5 13.5h11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"/>"#,
};
pub static LIBRARY: Icon = Icon {
    name: "fox-library",
    view: 16.0,
    body: r#"<path d="M4 2.5h8v11l-4-2.8-4 2.8z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"#,
};
pub static SETTINGS: Icon = Icon {
    name: "fox-settings",
    view: 16.0,
    body: r#"<path d="M2.5 5h11M2.5 11h11" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/><circle cx="6" cy="5" r="1.9" fill="currentColor"/><circle cx="10.5" cy="11" r="1.9" fill="currentColor"/>"#,
};
pub static CHEVRON_UP: Icon = Icon {
    name: "fox-chevron-up",
    view: 16.0,
    body: r#"<path d="M5 9.5L8 6.5l3 3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub static CHEVRON_DOWN: Icon = Icon {
    name: "fox-chevron-down",
    view: 16.0,
    body: r#"<path d="M5 6.5l3 3 3-3" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"/>"#,
};
pub static LOCK: Icon = Icon {
    name: "fox-lock",
    view: 16.0,
    body: r#"<rect x="3.5" y="7" width="9" height="6.5" rx="1.2" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2" fill="none" stroke="currentColor" stroke-width="1.4"/>"#,
};
pub static GLOBE: Icon = Icon {
    name: "fox-globe",
    view: 16.0,
    body: r#"<circle cx="8" cy="8" r="5.8" fill="none" stroke="currentColor" stroke-width="1.4"/><path d="M2.2 8h11.6M8 2.2c-2.3 2-2.3 9.6 0 11.6 2.3-2 2.3-9.6 0-11.6z" fill="none" stroke="currentColor" stroke-width="1.2"/>"#,
};

/// A glyph tinted `c`, hidden from the accessibility tree: inside a labeled
/// button the label carries the meaning and the glyph is decoration.
pub fn glyph(i: &'static Icon, size: f32, c: Color) -> El {
    icon(i, size).color(c).a11y_hidden()
}
