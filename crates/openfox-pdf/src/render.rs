//! The object model and the content-stream interpreter. Classic xref
//! tables, the seven object kinds, and the twenty operators that draw
//! almost every PDF ever made.

use crate::PdfError;

/// The PDF object graph. `Stream` keeps dict and raw bytes together —
/// filters (FlateDecode) are applied by the caller before `execute`.
#[derive(Debug, Clone, PartialEq)]
pub enum Obj {
    Null,
    Bool(bool),
    Int(i64),
    Real(f64),
    Str(Vec<u8>),
    Name(String),
    Array(Vec<Obj>),
    Dict(Vec<(String, Obj)>),
    Stream { dict: Vec<(String, Obj)>, data: Vec<u8> },
}

impl Obj {
    pub fn dict_get(&self, key: &str) -> Option<&Obj> {
        match self {
            Obj::Dict(pairs) | Obj::Stream { dict: pairs, .. } => {
                pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct XrefEntry {
    pub offset: u64,
    pub generation: u16,
    pub in_use: bool,
}

/// Parse a classic `xref ... trailer` table. Each entry is exactly 20
/// bytes: `NNNNNNNNNN GGGGG n\r\n` — the spec fixed the width so offsets
/// could be found by arithmetic, and so can we.
pub fn parse_xref(bytes: &[u8]) -> Result<Vec<XrefEntry>, PdfError> {
    let marker = find(bytes, b"xref").ok_or(PdfError::NotAPdf)?;
    let mut pos = skip_ws(bytes, marker + 4);
    let mut entries = Vec::new();
    loop {
        let (first, next) = parse_uint(bytes, pos).ok_or(PdfError::BadXref(pos))?;
        pos = skip_ws(bytes, next);
        let (count, next) = parse_uint(bytes, pos).ok_or(PdfError::BadXref(pos))?;
        pos = skip_ws(bytes, next);
        let _ = first;
        if count == 0 || pos + (count as usize) * 20 > bytes.len() {
            return Err(PdfError::BadXref(pos));
        }
        for index in 0..count {
            let base = pos + (index as usize) * 20;
            let text = String::from_utf8_lossy(&bytes[base..base + 20]);
            let mut parts = text.split_whitespace();
            let offset: u64 = parts
                .next()
                .and_then(|p| p.parse().ok())
                .ok_or(PdfError::BadXref(base))?;
            let generation: u16 = parts
                .next()
                .and_then(|p| p.parse().ok())
                .ok_or(PdfError::BadXref(base))?;
            let in_use = parts.next() == Some("n");
            entries.push(XrefEntry { offset, generation, in_use });
        }
        pos = skip_ws(bytes, pos + (count as usize) * 20);
        if bytes[pos..].starts_with(b"xref") {
            pos = skip_ws(bytes, pos + 4);
        } else {
            break;
        }
    }
    Ok(entries)
}

/// One drawing command after interpretation. This is the same display
/// list the HTML engine already knows how to paint.
#[derive(Debug, Clone, PartialEq)]
pub enum DisplayItem {
    Rect { x: f32, y: f32, w: f32, h: f32, gray: f32 },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, gray: f32, width: f32 },
    GlyphRun { x: f32, y: f32, size: f32, font: String, text: String },
}

#[derive(Debug, Clone, Copy, Default)]
struct Matrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix {
    fn identity() -> Self {
        Matrix { a: 1.0, ..Default::default() }
    }

    fn multiply(&self, other: &Matrix) -> Matrix {
        Matrix {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    fn apply(&self, x: f32, y: f32) -> (f32, f32) {
        (
            self.a * x + self.c * y + self.e,
            self.b * x + self.d * y + self.f,
        )
    }
}

#[derive(Debug, Clone, Copy)]
struct State {
    ctm: Matrix,
    gray: f32,
    stroke_gray: f32,
    line_width: f32,
}

impl Default for State {
    fn default() -> Self {
        State {
            ctm: Matrix::identity(),
            gray: 0.0,
            stroke_gray: 0.0,
            line_width: 1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct TextState {
    x: f32,
    y: f32,
    size: f32,
}

#[derive(Debug, Clone)]
enum Token {
    Number(f32),
    Name(String),
    Str(Vec<u8>),
    Op(String),
}

fn tokenize(content: &[u8]) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < content.len() {
        let byte = content[pos];
        if byte.is_ascii_whitespace() {
            pos += 1;
        } else if byte == b'(' {
            let (text, next) = read_literal_string(content, pos);
            tokens.push(Token::Str(text));
            pos = next;
        } else if byte == b'/' {
            let start = pos + 1;
            let mut end = start;
            while end < content.len()
                && !content[end].is_ascii_whitespace()
                && !b"[]<>(){}".contains(&content[end])
            {
                end += 1;
            }
            tokens.push(Token::Name(
                String::from_utf8_lossy(&content[start..end]).into_owned(),
            ));
            pos = end;
        } else if byte.is_ascii_digit() || byte == b'-' || byte == b'+' || byte == b'.' {
            let start = pos;
            let mut end = pos;
            while end < content.len()
                && (content[end].is_ascii_digit()
                    || content[end] == b'.'
                    || content[end] == b'-'
                    || content[end] == b'+')
            {
                end += 1;
            }
            if let Ok(value) = std::str::from_utf8(&content[start..end]).unwrap_or("").parse() {
                tokens.push(Token::Number(value));
            }
            pos = end;
        } else {
            let start = pos;
            let mut end = pos;
            while end < content.len()
                && !content[end].is_ascii_whitespace()
                && !b"[]<>(){}".contains(&content[end])
            {
                end += 1;
            }
            if end > start {
                tokens.push(Token::Op(
                    String::from_utf8_lossy(&content[start..end]).into_owned(),
                ));
            }
            pos = end.max(pos + 1);
        }
    }
    tokens
}

fn read_literal_string(content: &[u8], start: usize) -> (Vec<u8>, usize) {
    let mut out = Vec::new();
    let mut depth = 1;
    let mut pos = start + 1;
    while pos < content.len() {
        match content[pos] {
            b'\\' if pos + 1 < content.len() => {
                out.push(unescape(content[pos + 1]));
                pos += 2;
            }
            b'(' => {
                depth += 1;
                out.push(b'(');
                pos += 1;
            }
            b')' => {
                depth -= 1;
                if depth == 0 {
                    pos += 1;
                    break;
                }
                out.push(b')');
                pos += 1;
            }
            byte => {
                out.push(byte);
                pos += 1;
            }
        }
    }
    (out, pos)
}

fn unescape(byte: u8) -> u8 {
    match byte {
        b'n' => b'\n',
        b'r' => b'\r',
        b't' => b'\t',
        other => other,
    }
}

fn win_ansi_to_string(bytes: &[u8]) -> String {
    // WinAnsi is latin-1 shaped; lossy keeps the pipeline moving the same
    // way Firefox's pdf.js treats legacy encodings.
    bytes.iter().map(|&b| b as char).collect()
}

/// Execute a content stream: tokenize, keep the graphics-state stack
/// (q/Q push/pop, cm multiplies), and emit display items. Text assumes
/// standard-14 fonts; CID fonts route through `openfox-render` shaping.
pub fn execute(content: &[u8]) -> Result<Vec<DisplayItem>, PdfError> {
    let mut items = Vec::new();
    let mut stack: Vec<State> = vec![State::default()];
    let mut numbers: Vec<f32> = Vec::new();
    let mut names: Vec<String> = Vec::new();
    let mut pending_text: Vec<u8> = Vec::new();
    let mut text = TextState { size: 12.0, ..Default::default() };

    for token in tokenize(content) {
        match token {
            Token::Number(value) => numbers.push(value),
            Token::Name(name) => names.push(name),
            Token::Str(bytes) => {
                // String operand: the text that Tj/TJ will show next.
                pending_text = bytes;
            }
            Token::Op(op) => {
                let state = stack.last().cloned().unwrap_or_default();
                match op.as_str() {
                    "q" => stack.push(state),
                    "Q" => {
                        stack.pop();
                    }
                    "cm" if numbers.len() >= 6 => {
                        let m = Matrix {
                            a: numbers[numbers.len() - 6],
                            b: numbers[numbers.len() - 5],
                            c: numbers[numbers.len() - 4],
                            d: numbers[numbers.len() - 3],
                            e: numbers[numbers.len() - 2],
                            f: numbers[numbers.len() - 1],
                        };
                        let top = stack.last_mut().expect("state stack never empty");
                        top.ctm = m.multiply(&top.ctm);
                    }
                    "w" if !numbers.is_empty() => {
                        stack.last_mut().expect("state stack never empty").line_width =
                            *numbers.last().expect("checked non-empty");
                    }
                    "rg" if numbers.len() >= 3 => {
                        let gray = rgb_to_gray(
                            numbers[numbers.len() - 3],
                            numbers[numbers.len() - 2],
                            numbers[numbers.len() - 1],
                        );
                        stack.last_mut().expect("state stack never empty").gray = gray;
                    }
                    "RG" if numbers.len() >= 3 => {
                        let gray = rgb_to_gray(
                            numbers[numbers.len() - 3],
                            numbers[numbers.len() - 2],
                            numbers[numbers.len() - 1],
                        );
                        stack.last_mut().expect("state stack never empty").stroke_gray = gray;
                    }
                    "re" if numbers.len() >= 4 => {
                        let (x, y) = state.ctm.apply(
                            numbers[numbers.len() - 4],
                            numbers[numbers.len() - 3],
                        );
                        items.push(DisplayItem::Rect {
                            x,
                            y,
                            w: numbers[numbers.len() - 2] * state.ctm.a,
                            h: numbers[numbers.len() - 1] * state.ctm.d,
                            gray: state.gray,
                        });
                    }
                    "f" | "F" | "f*" | "B" | "B*" => {
                        // The rect already emitted on `re`; fill confirms it.
                    }
                    "S" => {
                        if let Some(DisplayItem::Rect { x, y, w, h, .. }) = items.last().cloned() {
                            items.push(DisplayItem::Line {
                                x1: x,
                                y1: y,
                                x2: x + w,
                                y2: y + h,
                                gray: state.stroke_gray,
                                width: state.line_width,
                            });
                        }
                    }
                    "n" => {}
                    "BT" => text = TextState { size: text.size, ..Default::default() },
                    "ET" => {}
                    "Tf" if !numbers.is_empty() => {
                        text.size = *numbers.last().expect("checked non-empty");
                    }
                    "Td" if numbers.len() >= 2 => {
                        text.x += numbers[numbers.len() - 2];
                        text.y += numbers[numbers.len() - 1];
                    }
                    "Tj" | "TJ" => {
                        if !pending_text.is_empty() {
                            let (x, y) = state.ctm.apply(text.x, text.y);
                            items.push(DisplayItem::GlyphRun {
                                x,
                                y,
                                size: text.size * state.ctm.d,
                                font: names
                                    .last()
                                    .cloned()
                                    .unwrap_or_else(|| "Helvetica".into()),
                                text: win_ansi_to_string(&pending_text),
                            });
                            // Advance by a rough advance width so runs do
                            // not stack; real metrics come from the shaper.
                            text.x += text.size * 0.5 * pending_text.len() as f32;
                            pending_text.clear();
                        }
                    }
                    _ => {}
                }
                numbers.clear();
                names.clear();
            }
        }
    }
    Ok(items)
}

fn rgb_to_gray(r: f32, g: f32, b: f32) -> f32 {
    0.3 * r + 0.59 * g + 0.11 * b
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn skip_ws(bytes: &[u8], mut pos: usize) -> usize {
    while pos < bytes.len() && bytes[pos].is_ascii_whitespace() {
        pos += 1;
    }
    pos
}

fn parse_uint(bytes: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    while pos < bytes.len() && (bytes[pos] == b' ' || bytes[pos] == b'\t') {
        pos += 1;
    }
    let start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    if pos == start {
        return None;
    }
    std::str::from_utf8(&bytes[start..pos])
        .ok()?
        .parse()
        .ok()
        .map(|value| (value, pos))
}
