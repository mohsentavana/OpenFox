//! The new-tab page: the OpenFox wordmark, a search field, and eight
//! quick-link tiles with hue-tinted monograms. State is a plain struct the
//! chrome owns; the search field is a real Rune editor (`TextEdit` +
//! `on_edit`), so scripted typing reaches the actual field.

use crate::chrome::{ChromeIntent, ChromeState};
use crate::{icons, theme};
use rune::edit_access;
use rune::text_input;
use rune::{El, Span, TextStyle, col, el, oklch, rich, row, text, tok};
use rune::tok::{motion, type_};

/// One quick-link tile: where it goes, the two-letter monogram on its face,
/// and the OKLCH hue that tints it. Hues are spread so the grid reads as a
/// shelf of distinct sites without fetching a single favicon.
#[derive(Debug, Clone)]
pub struct QuickLink {
    pub label: String,
    pub url: &'static str,
    pub monogram: &'static str,
    pub hue: f32,
}

/// The new-tab page's state. `search` is the field's editor; `tiles` is the
/// quick-link shelf, defaults below.
#[derive(Debug, Clone)]
pub struct NewTabState {
    pub search: rune::TextEdit,
    pub tiles: Vec<QuickLink>,
}

impl Default for NewTabState {
    fn default() -> NewTabState {
        let link = |label: &str, url: &'static str, monogram: &'static str, hue: f32| QuickLink {
            label: label.to_owned(),
            url,
            monogram,
            hue,
        };
        NewTabState {
            search: rune::TextEdit::new(""),
            tiles: vec![
                link("GitHub", "https://github.com", "GH", 265.0),
                link("Wikipedia", "https://wikipedia.org", "WK", 95.0),
                link("Rust", "https://rust-lang.org", "RS", 55.0),
                link("Hacker News", "news.ycombinator.com", "HN", 75.0),
                link("YouTube", "youtube.com", "YT", 20.0),
                link("Reddit", "reddit.com", "RD", 16.0),
                link("Mastodon", "joinmastodon.org", "MD", 300.0),
                link("Archive", "archive.org", "AR", 210.0),
            ],
        }
    }
}

/// The page: wordmark, search field, tiles — centred on the page slot.
pub fn view(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    col()
        .key("newtab")
        .w_full()
        .grow(1.0)
        .fill(t.bg)
        .items_center()
        .justify_center()
        .child(wordmark(s.dark).mb(tok::space::S6))
        .child(search_field(s).mb(32.0))
        .child(tiles(s))
}

/// "OpenFox": the TITLE style scaled to a wordmark, "Fox" carrying the
/// accent — the one place the identity colour gets to be typography.
fn wordmark(dark: bool) -> El {
    let t = tok::theme(dark);
    rich(
        "OpenFox",
        vec![
            Span::new(0, 4).size(30.0).weight(700.0),
            Span::new(4, 7).size(30.0).weight(700.0).color(theme::accent(dark)),
        ],
    )
    .font(type_::TITLE)
    .color(t.text)
}

/// The search field: a rounded-full card with the accent focus ring, a real
/// editor inside. Return submits: the chrome flips to loading with the
/// address set, the same navigation idiom the toolbar uses.
fn search_field(s: &ChromeState) -> El {
    let t = tok::theme(s.dark);
    let accent = theme::accent(s.dark);
    row()
        .key("newtab-search")
        .role("textbox")
        .label("Search the web")
        .w(480.0)
        .h(44.0)
        .radius(22.0)
        .px(tok::space::S3)
        .gap(tok::space::S2)
        .items_center()
        .fill(t.card)
        .border(1.0, t.card_border)
        .shadow(2.0, 12.0, t.shadow_soft)
        .focus_within_ring(2.0, 2.0, accent)
        .motion(motion::SNAPPY)
        .child(icons::glyph(&icons::SEARCH, 15.0, t.text_secondary))
        .child(
            text_input(&s.newtab.search, type_::BODY)
                .grow(1.0)
                .color(t.text)
                .placeholder("Search the web or enter address", t.text_secondary)
                .caret(accent, accent.alpha(0.3))
                .on_edit(edit_access(|s: &mut ChromeState| &mut s.newtab.search))
                .on_submit(|s: &mut ChromeState, _| {
                    let query = s.newtab.search.text.trim().to_owned();
                    if !query.is_empty() {
                        s.address = query.clone();
                        s.loading = true;
                        s.intents.push(ChromeIntent::Navigate(query));
                    }
                }),
        )
}

/// The 4×2 tile shelf, each tile entering on its own stagger.
fn tiles(s: &ChromeState) -> El {
    let mut shelf = col().key("newtab-tiles").gap(tok::space::S3);
    let mut index = 0usize;
    for chunk in s.newtab.tiles.chunks(4) {
        let mut rank = row().gap(tok::space::S3);
        for tile in chunk {
            rank = rank.child(tile_el(tile, s.dark, index));
            index += 1;
        }
        shelf = shelf.child(rank);
    }
    shelf
}

/// One tile: a 56 pt rounded-square face in a soft hue tint with the
/// two-letter monogram, the label under it in the secondary ink.
fn tile_el(tile: &QuickLink, dark: bool, index: usize) -> El {
    let t = tok::theme(dark);
    let (tint, hover_tint, ink) = if dark {
        (oklch(0.26, 0.02, tile.hue), oklch(0.31, 0.03, tile.hue), oklch(0.78, 0.11, tile.hue))
    } else {
        (oklch(0.945, 0.012, tile.hue), oklch(0.905, 0.02, tile.hue), oklch(0.52, 0.13, tile.hue))
    };
    let url = tile.url;
    let label = tile.label.clone();
    col()
        .key(format!("tile-{}", tile.label))
        .role("link")
        .label(label.clone())
        .focusable(true)
        .gap(6.0)
        .w(76.0)
        .items_center()
        .motion(motion::SNAPPY)
        .press_scale(0.96)
        .enter_delay(index as f32 * 0.025)
        .on_press(move |s: &mut ChromeState| {
            s.address = url.to_owned();
            s.loading = true;
            s.intents.push(ChromeIntent::Navigate(url.to_owned()));
        })
        .child(
            el()
                .key("tile-face")
                .size(56.0, 56.0)
                .radius(tok::radius::LG)
                .fill(tint)
                .hover(hover_tint)
                .items_center()
                .justify_center()
                .child(text(tile.monogram).font(TextStyle::new(18.0, 650.0)).color(ink)),
        )
        .child(text(label).font(type_::LABEL).color(t.text_secondary).truncate())
}
