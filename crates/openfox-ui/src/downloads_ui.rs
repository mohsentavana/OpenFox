//! The downloads panel: rows that mirror the download manager's state and
//! intents the command bus executes. The panel owns nothing — the download
//! manager owns truth and pushes rows back after every state change.

/// Download lifecycle, one badge per state. The strings are l10n keys, not
/// English: the panel never hardcodes a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStatus {
    Active,
    Paused,
    Done,
    Failed,
    Cancelled,
}

impl DownloadStatus {
    pub fn badge_key(&self) -> &'static str {
        match self {
            DownloadStatus::Active => "dl.active",
            DownloadStatus::Paused => "dl.paused",
            DownloadStatus::Done => "dl.done",
            DownloadStatus::Failed => "dl.failed",
            DownloadStatus::Cancelled => "dl.cancelled",
        }
    }
}

/// One download as the panel shows it: what openfox-download reports, minus
/// everything a list row does not need (temp paths, retry counters).
#[derive(Debug, Clone)]
pub struct DownloadRow {
    pub id: u64,
    pub filename: String,
    /// Bytes done over bytes total, clamped to 0.0..=1.0.
    pub progress: f32,
    pub status: DownloadStatus,
    pub speed_bps: u64,
}

impl DownloadRow {
    pub fn new(id: u64, filename: impl Into<String>) -> DownloadRow {
        DownloadRow { id, filename: filename.into(), progress: 0.0, status: DownloadStatus::Active, speed_bps: 0 }
    }

    /// Progress from byte counters. Total zero means size unknown: the row
    /// shows a spinner, not a lying 0%.
    pub fn set_progress(&mut self, done: u64, total: u64, speed_bps: u64) {
        self.progress = if total == 0 { 0.0 } else { (done as f32 / total as f32).clamp(0.0, 1.0) };
        self.speed_bps = speed_bps;
    }

    pub fn percent(&self) -> u8 {
        (self.progress * 100.0).round() as u8
    }

    /// Human speed for active rows; empty elsewhere — a done file has no
    /// speed and showing "0 KB/s" would be noise.
    pub fn speed_hint(&self) -> String {
        if self.status != DownloadStatus::Active || self.speed_bps == 0 {
            return String::new();
        }
        if self.speed_bps < 1_000_000 {
            format!("{:.0} KB/s", self.speed_bps as f32 / 1_000.0)
        } else {
            format!("{:.1} MB/s", self.speed_bps as f32 / 1_000_000.0)
        }
    }
}

/// What a button press in the panel asks the browser to do. Every variant
/// travels the command bus to the download manager; the panel never mutates
/// a download itself, so an agent driving the MCP surface and a user
/// clicking see the same state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadsIntent {
    Pause(u64),
    Resume(u64),
    Cancel(u64),
    Reveal(u64),
    Open(u64),
}

/// The panel model. `open` is chrome state; `items` is a mirror pushed by
/// the download manager after each of its own transitions.
#[derive(Debug, Clone, Default)]
pub struct DownloadsPanel {
    pub open: bool,
    pub items: Vec<DownloadRow>,
}

impl DownloadsPanel {
    pub fn toggle(&mut self) {
        self.open = !self.open;
    }

    /// Merges one row by id. Rows arrive out of order when several
    /// downloads run; the panel keeps stable order by insert.
    pub fn upsert(&mut self, row: DownloadRow) {
        match self.items.iter().position(|item| item.id == row.id) {
            Some(index) => self.items[index] = row,
            None => self.items.push(row),
        }
    }

    pub fn active_count(&self) -> usize {
        self.items.iter().filter(|row| row.status == DownloadStatus::Active).count()
    }

    /// Removes finished rows from the list. Pure chrome: the files stay on
    /// disk, only the rows go — which is why this is a method and not an
    /// intent.
    pub fn clear_done(&mut self) {
        self.items
            .retain(|row| !matches!(row.status, DownloadStatus::Done | DownloadStatus::Cancelled | DownloadStatus::Failed));
    }

    /// The intent a row's primary button maps to, given its state: active
    /// rows pause, paused rows resume, everything else opens.
    pub fn primary_intent(&self, id: u64) -> Option<DownloadsIntent> {
        match self.items.iter().find(|row| row.id == id)?.status {
            DownloadStatus::Active => Some(DownloadsIntent::Pause(id)),
            DownloadStatus::Paused => Some(DownloadsIntent::Resume(id)),
            _ => Some(DownloadsIntent::Open(id)),
        }
    }
}

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

use crate::{icons, theme, widgets};
use rune::{Color, El, LoopSpec, ScrollSpec, col, el, row, text, tok};
use rune::tok::{motion, type_};

const PANEL_W: f32 = 360.0;

/// The downloads panel: a raised card with a header, hairline-split rows and
/// per-row progress. Handlers run against `DownloadsPanel` — the browser's
/// shell owns the model and routes the intents the row buttons ask for.
pub fn view(s: &DownloadsPanel, dark: bool) -> El {
    let t = tok::theme(dark);
    let has_done = s.items.iter().any(|row| row.status == DownloadStatus::Done);
    el()
        .key("downloads-panel")
        .role("region")
        .label("Downloads")
        .w(PANEL_W)
        .h_full()
        .radius(tok::radius::LG)
        .fill(t.card)
        .border(1.0, t.card_border)
        .shadow(8.0, 24.0, t.shadow)
        .motion(motion::SNAPPY)
        .child(
            row()
                .key("downloads-head")
                .px(tok::space::S3)
                .py(tok::space::S2)
                .gap(tok::space::S2)
                .items_center()
                .child(icons::glyph(&icons::DOWNLOAD, 15.0, theme::accent(dark)))
                .child(text("Downloads").font(type_::TITLE).color(t.text).grow(1.0))
                .child(
                    widgets::text_button("downloads-clear", "Clear done", dark, move |s: &mut DownloadsPanel| {
                        s.clear_done();
                    })
                    .disabled(!has_done)
                    .dim(if has_done { 1.0 } else { 0.45 }),
                ),
        )
        .child(widgets::hairline(dark))
        .child(
            el()
                .key("downloads-scroll")
                .w_full()
                .grow(1.0)
                .scroll(ScrollSpec { stick_bottom: true })
                .child(col().key("downloads-rows").w_full().children(
                    if s.items.is_empty() {
                        vec![el()
                            .key("downloads-empty")
                            .px(tok::space::S3)
                            .py(tok::space::S6)
                            .child(text("No downloads yet.").font(type_::BODY).color(t.text_secondary))]
                    } else {
                        s.items.iter().map(|row| download_row(row, dark)).collect()
                    },
                )),
        )
}

/// One download row: filename, status caption and speed hint, a progress
/// bar (determinate or, when the size is unknown, the same accent sweep the
/// chrome's loading line uses), and the row's primary action.
fn download_row(item: &DownloadRow, dark: bool) -> El {
    let t = tok::theme(dark);
    let accent = theme::accent(dark);
    let id = item.id;
    let (action, glyph) = match item.status {
        DownloadStatus::Active => ("Pause", &icons::CLOSE),
        DownloadStatus::Paused => ("Resume", &icons::DOWNLOAD),
        _ => ("Open", &icons::DOWNLOAD),
    };
    col()
        .key(format!("download-{}", item.id))
        .child(
            row()
                .key("download-main")
                .w_full()
                .px(tok::space::S3)
                .py(tok::space::S2)
                .gap(tok::space::S2)
                .items_center()
                .hover(t.hover)
                .press(t.press)
                .motion(motion::SNAPPY)
                .child(
                    col()
                        .key("download-text")
                        .grow(1.0)
                        .gap(1.0)
                        .child(text(item.filename.clone()).font(type_::LABEL).color(t.text).truncate())
                        .child(
                            row()
                                .key("download-status")
                                .gap(tok::space::S2)
                                .child(text(item.status.badge_key()).font(type_::SECTION).color(t.text_secondary))
                                .child(text(item.speed_hint()).font(type_::MONO_CAPTION).color(t.text_secondary)),
                        ),
                )
                .child(
                    el()
                        .key("download-action")
                        .role("button")
                        .label(format!("{action} {}", item.filename))
                        .focusable(true)
                        .size(28.0, 28.0)
                        .radius(14.0)
                        .fill(Color::TRANSPARENT)
                        .hover(t.hover)
                        .press(t.press)
                        .press_scale(0.94)
                        .motion(motion::SNAPPY)
                        .items_center()
                        .justify_center()
                        // The intent travels the command bus in the host;
                        // the panel mirrors the manager's next push.
                        .on_press(move |s: &mut DownloadsPanel| {
                            if let Some(intent) = s.primary_intent(id) {
                                tracing::debug!(target: "openfox::downloads", ?intent, "download intent");
                            }
                        })
                        .child(icons::glyph(glyph, 13.0, t.text_secondary)),
                ),
        )
        .child(
            el()
                .key("download-progress-track")
                .mx(tok::space::S3)
                .mb(9.0)
                .h(3.0)
                .radius(2.0)
                .w_full()
                .fill(t.control_track)
                .child(progress_fill(item, accent)),
        )
        .child(widgets::hairline(dark))
}

/// The progress fill: a fraction of the track when the size is known, the
/// chrome's indeterminate accent sweep when it is not.
fn progress_fill(row: &DownloadRow, accent: rune::Color) -> El {
    let fill = el().key("download-progress-fill").h(3.0).radius(2.0).fill(accent);
    if row.status == DownloadStatus::Active && row.progress <= 0.0 {
        fill.w_frac(0.3).looping(LoopSpec::slide(1.4, [-1.0, 0.0], [3.4, 0.0]))
    } else {
        fill.w_frac(row.progress.clamp(0.0, 1.0))
    }
}
