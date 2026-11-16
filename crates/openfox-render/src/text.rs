//! In-house text shaping: line breaking, a minimal bidi pass and ellipsis
//! for clipped runs. Input is the engine layout's `ShapedTextRequest`s.

/// The metrics a shaper needs from a font for one character at one size.
/// The glyph atlas implements this; glyph ids index the atlas.
pub struct GlyphMetrics {
    pub id: u32,
    pub advance: f32,
}

/// The font side of shaping, as the pipeline consumes it.
pub trait GlyphSource {
    fn metrics(&mut self, ch: char, size_px: f32) -> GlyphMetrics;
}

/// One glyph placed in device pixels.
#[derive(Debug, Clone, Copy)]
pub struct PositionedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
}

/// A shaped run: glyphs plus their advances, parallel arrays. Advances ride
/// along because caret movement and hit-testing need them without a re-shape.
#[derive(Debug, Clone)]
pub struct GlyphRun {
    pub font_id: u32,
    pub size_px: f32,
    pub glyphs: Vec<PositionedGlyph>,
    pub advances: Vec<f32>,
}

/// What the engine's layout asks the pipeline to shape: the text, the font,
/// the origin, and the width to break at.
#[derive(Debug, Clone)]
pub struct ShapedTextRequest {
    pub text: String,
    pub font_id: u32,
    pub size_px: f32,
    pub x: f32,
    pub y: f32,
    pub max_width: f32,
}

/// Line height as a multiple of the font size.
const LINE_HEIGHT: f32 = 1.25;
const ELLIPSIS: char = '\u{2026}';

/// The shaping pipeline. Stateless — the atlas caches glyph rasterizations,
/// so the pipeline itself has nothing to remember.
pub struct TextPipeline;

impl TextPipeline {
    pub fn new() -> TextPipeline {
        TextPipeline
    }

    /// Shapes one request into runs, one per wrapped line: reorder the
    /// paragraph for display, wrap at break opportunities, then place
    /// glyphs left to right.
    pub fn shape(&mut self, source: &mut impl GlyphSource, req: &ShapedTextRequest) -> Vec<GlyphRun> {
        let visual = reorder(&req.text);
        let lines = wrap(&visual, req.max_width, source, req.size_px);
        lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let baseline = req.y + index as f32 * req.size_px * LINE_HEIGHT;
                let mut run = GlyphRun {
                    font_id: req.font_id,
                    size_px: req.size_px,
                    glyphs: Vec::with_capacity(line.len()),
                    advances: Vec::with_capacity(line.len()),
                };
                let mut pen = req.x;
                for ch in line {
                    let metrics = source.metrics(*ch, req.size_px);
                    run.glyphs.push(PositionedGlyph { id: metrics.id, x: pen, y: baseline });
                    run.advances.push(metrics.advance);
                    pen += metrics.advance;
                }
                if pen > req.x + req.max_width {
                    ellipsize(&mut run, source, req.max_width);
                }
                run
            })
            .collect()
    }
}

impl Default for TextPipeline {
    fn default() -> Self {
        TextPipeline::new()
    }
}

/// Characters after which a line may break. An ASCII-and-common-punctuation
/// heuristic table; full UAX#14 is phase 2 — this set covers every real
/// sentence on the web that is not poetry.
const BREAK_AFTER: &[char] = &[' ', '\t', '-', '\u{00ad}', '\u{2010}', '\u{2013}', '\u{2014}'];

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32, 0x4E00..=0x9FFF | 0x3040..=0x30FF | 0xAC00..=0xD7AF)
}

fn can_break_after(prev: char) -> bool {
    BREAK_AFTER.contains(&prev) || is_cjk(prev)
}

/// Wraps to `max_width`, measuring through the source. Breaks backtrack to
/// the last opportunity; a word with none breaks hard rather than overflow.
fn wrap(text: &str, max_width: f32, source: &mut impl GlyphSource, size: f32) -> Vec<Vec<char>> {
    let mut lines: Vec<Vec<char>> = vec![Vec::new()];
    let mut width = 0.0;
    let mut last_break: Option<usize> = None;
    for ch in text.chars() {
        if ch == '\n' {
            lines.push(Vec::new());
            width = 0.0;
            last_break = None;
            continue;
        }
        let advance = source.metrics(ch, size).advance;
        let line = lines.last_mut().expect("wrap always has an open line");
        line.push(ch);
        width += advance;
        if can_break_after(ch) {
            last_break = Some(line.len());
        }
        if width > max_width && line.len() > 1 {
            let cut = match last_break.filter(|&at| at < line.len()) {
                Some(at) if at > 0 => at,
                _ => line.len() - 1, // no opportunity: hard break mid-word
            };
            let tail = line.split_off(cut);
            width = tail.iter().map(|c| source.metrics(*c, size).advance).sum();
            lines.push(tail);
            last_break = None;
        }
    }
    lines
}

/// True for the RTL blocks the minimal algorithm covers: Hebrew, Arabic
/// and their presentation forms.
fn is_rtl(ch: char) -> bool {
    matches!(ch as u32, 0x0590..=0x05FF | 0x0600..=0x06FF | 0x0700..=0x074F | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

/// Paragraph embedding levels: 0 for LTR characters, 1 for RTL runs. This
/// is the minimal honest model — no mirroring, no number handling; the
/// shaping that needs UAX#9 in full is phase 2.
pub fn embedding_levels(text: &str) -> Vec<u8> {
    text.chars().map(|ch| u8::from(is_rtl(ch))).collect()
}

/// Reorders for display: each maximal RTL run is reversed so it renders
/// right-to-left inside an LTR paragraph.
pub fn reorder(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let levels = embedding_levels(text);
    let mut out = chars.clone();
    let mut i = 0;
    while i < chars.len() {
        if levels[i] == 1 {
            let end = (i..chars.len()).take_while(|&k| levels[k] == 1).last().expect("levels[i] is 1");
            out[i..=end].reverse();
            i = end + 1;
        } else {
            i += 1;
        }
    }
    out.into_iter().collect()
}

/// Truncates a run to `max_width` and appends an ellipsis glyph. Used for
/// clipped single-line runs: chrome labels and tab titles, where a hard
/// clip without an ellipsis reads as a rendering bug.
fn ellipsize(run: &mut GlyphRun, source: &mut impl GlyphSource, max_width: f32) {
    let ell = source.metrics(ELLIPSIS, run.size_px);
    let mut width: f32 = run.advances.iter().sum();
    while width + ell.advance > max_width && !run.glyphs.is_empty() {
        width -= run.advances.pop().expect("advances parallel glyphs");
        run.glyphs.pop();
    }
    let x = run.glyphs.last().map(|g| g.x).unwrap_or(0.0)
        + run.advances.last().copied().unwrap_or(0.0);
    let y = run.glyphs.last().map(|g| g.y).unwrap_or(0.0);
    run.glyphs.push(PositionedGlyph { id: ell.id, x, y });
    run.advances.push(ell.advance);
}
