//! about: pages. They render through the engine like any other page —
//! no special chrome, no private rendering path. A page that is part
//! of the browser still plays by the browser's rules.

pub enum AboutPage {
    /// Version, shield totals, uptime — the honest front door.
    Fox,
    Blank,
    /// The crash log, from openfox-crash's queue.
    Crashes,
    /// Per-crate allocation counters, from the global allocator hooks.
    Memory,
    /// One honest paragraph about what this build does and does not
    /// collect. Not legal slop — an answer.
    Rights,
    /// Unknown about: host — the engine renders its error page.
    Unknown,
}

pub const FOX_HTML: &str = r#"<!doctype html>
<html><head><title>about:fox</title></head>
<body>
<h1>OpenFox</h1>
<p>Pure Rust. No ads. MCP-native.</p>
<p id="shield">Shield counters render from the live totals.</p>
</body></html>"#;

pub const RIGHTS_HTML: &str = r#"<!doctype html>
<html><head><title>about:rights</title></head>
<body>
<h1>Your rights</h1>
<p>OpenFox stores your history, bookmarks and passwords on your device.
It does not send them anywhere. Sync is end-to-end encrypted and off
until you turn it on. Crash reports are local until you choose to send
one. There is no telemetry, no sponsored tiles, and no ad substitution:
a blocked slot stays empty.</p>
</body></html>"#;

pub const BLANK_HTML: &str = "<!doctype html><html><head><title>about:blank</title></head><body></body></html>";

/// Resolve an about: host to its page. Unknown hosts are the engine's
/// error page, same as any unresolvable url — about: is not a namespace
/// for anyone's easter eggs.
pub fn resolve(url: &str) -> AboutPage {
    let host = url
        .strip_prefix("about:")
        .unwrap_or_default()
        .split(&['/', '?'][..])
        .next()
        .unwrap_or_default();
    match host {
        "fox" => AboutPage::Fox,
        "blank" => AboutPage::Blank,
        "crashes" => AboutPage::Crashes,
        "memory" => AboutPage::Memory,
        "rights" => AboutPage::Rights,
        _ => AboutPage::Unknown,
    }
}

/// The HTML for a resolved page. Crash and memory pages are dynamic —
/// their content is a function the caller fills from live state.
pub fn html_for(page: &AboutPage) -> &'static str {
    match page {
        AboutPage::Fox => FOX_HTML,
        AboutPage::Blank => BLANK_HTML,
        AboutPage::Rights => RIGHTS_HTML,
        // The dynamic pages carry their skeleton here; the numbers are
        // injected by the shell before the engine parses.
        AboutPage::Crashes => CRASHES_SKELETON,
        AboutPage::Memory => MEMORY_SKELETON,
        AboutPage::Unknown => UNKNOWN_SKELETON,
    }
}

pub const CRASHES_SKELETON: &str = r#"<!doctype html>
<html><head><title>about:crashes</title></head>
<body><h1>Crash reports</h1><ul id="queue"></ul></body></html>"#;

pub const MEMORY_SKELETON: &str = r#"<!doctype html>
<html><head><title>about:memory</title></head>
<body><h1>Memory</h1><table id="counters"></table></body></html>"#;

pub const UNKNOWN_SKELETON: &str = r#"<!doctype html>
<html><head><title>Unknown about page</title></head>
<body><h1>No such about: page</h1></body></html>"#;
