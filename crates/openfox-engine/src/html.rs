use crate::dom::{Dom, NodeId};

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Doctype,
    StartTag { name: String, attrs: Vec<(String, String)>, self_closing: bool },
    EndTag { name: String },
    Text(String),
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track", "wbr",
];

const RAW_TEXT: &[&str] = &["script", "style", "textarea", "title"];

/// An HTML tokenizer: state machine over bytes, emitting a token stream.
/// Handles comments, doctypes, quoted and bare attributes, self-closing
/// tags, raw-text elements and numeric + named entities. Not a spec-faithful
/// tree builder: recovery is "close what you can, keep going".
pub struct Tokenizer<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Tokenizer<'a> {
    pub fn new(input: &'a str) -> Self {
        Tokenizer { input: input.as_bytes(), pos: 0 }
    }

    fn rest_starts_with(&self, needle: &str) -> bool {
        self.input[self.pos..].starts_with(needle.as_bytes())
    }

    fn skip_until(&mut self, needle: &str) {
        while self.pos < self.input.len() && !self.rest_starts_with(needle) {
            self.pos += 1;
        }
        self.pos = (self.pos + needle.len()).min(self.input.len());
    }

    pub fn tokenize(mut self) -> Vec<Token> {
        let mut tokens = Vec::with_capacity(256);
        while self.pos < self.input.len() {
            if self.rest_starts_with("<!--") {
                self.skip_until("-->");
                continue;
            }
            if self.rest_starts_with("<!") {
                self.skip_until(">");
                tokens.push(Token::Doctype);
                continue;
            }
            if self.rest_starts_with("</") {
                self.pos += 2;
                let name = self.take_while(|b| b != b'>');
                self.skip_byte(b'>');
                if !name.is_empty() {
                    tokens.push(Token::EndTag { name: name.to_ascii_lowercase() });
                }
                continue;
            }
            if self.peek() == Some(b'<') && self.peek_at(1).is_some_and(|b| b.is_ascii_alphabetic()) {
                self.pos += 1;
                let name = self.take_while(|b| !b.is_ascii_whitespace() && b != b'>' && b != b'/');
                let (attrs, self_closing) = self.take_attrs();
                if !name.is_empty() {
                    tokens.push(Token::StartTag {
                        name: name.to_ascii_lowercase(),
                        attrs,
                        self_closing,
                    });
                }
                if RAW_TEXT.contains(&name.as_str()) && !self_closing {
                    let closer = format!("</{}", name);
                    let raw = self.take_until_ci(&closer);
                    if !raw.is_empty() {
                        tokens.push(Token::Text(decode_entities(&raw)));
                    }
                    if let Some(end) = self.find_ci(&closer) {
                        self.pos = end + closer.len();
                        self.skip_byte(b'>');
                        tokens.push(Token::EndTag { name });
                    }
                }
                continue;
            }
            let text = self.take_while(|b| b != b'<');
            if !text.is_empty() {
                tokens.push(Token::Text(decode_entities(&collapse_ws(&text))));
            }
        }
        tokens
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.input.get(self.pos + offset).copied()
    }

    fn skip_byte(&mut self, b: u8) {
        if self.peek() == Some(b) {
            self.pos += 1;
        }
    }

    fn take_while<F: Fn(u8) -> bool>(&mut self, pred: F) -> String {
        let start = self.pos;
        while self.pos < self.input.len() && pred(self.input[self.pos]) {
            self.pos += 1;
        }
        String::from_utf8_lossy(&self.input[start..self.pos]).into_owned()
    }

    fn take_until_ci(&mut self, closer: &str) -> String {
        let end = self.find_ci(closer).unwrap_or(self.input.len());
        let raw = String::from_utf8_lossy(&self.input[self.pos..end]).into_owned();
        self.pos = end;
        raw
    }

    fn find_ci(&self, needle: &str) -> Option<usize> {
        let hay = &self.input[self.pos..];
        let needle = needle.as_bytes();
        if needle.is_empty() {
            return None;
        }
        hay.windows(needle.len()).position(|w| w.eq_ignore_ascii_case(needle)).map(|i| i + self.pos)
    }

    fn take_attrs(&mut self) -> (Vec<(String, String)>, bool) {
        let mut attrs = Vec::new();
        let mut self_closing = false;
        loop {
            self.take_while(|b| b.is_ascii_whitespace());
            match self.peek() {
                None => break,
                Some(b'>') => {
                    self.pos += 1;
                    break;
                }
                Some(b'/') => {
                    self.pos += 1;
                    self_closing = true;
                }
                _ => {
                    let name = self.take_while(|b| !b.is_ascii_whitespace() && b != b'=' && b != b'>' && b != b'/');
                    if name.is_empty() {
                        self.pos += 1; // stray byte: drop it, keep going
                        continue;
                    }
                    self.take_while(|b| b.is_ascii_whitespace());
                    let value = if self.peek() == Some(b'=') {
                        self.pos += 1;
                        self.take_while(|b| b.is_ascii_whitespace());
                        match self.peek() {
                            Some(q @ (b'"' | b'\'')) => {
                                self.pos += 1;
                                let v = self.take_while(|b| b != q);
                                self.skip_byte(q);
                                v
                            }
                            _ => self.take_while(|b| !b.is_ascii_whitespace() && b != b'>'),
                        }
                    } else {
                        String::new()
                    };
                    attrs.push((name.to_ascii_lowercase(), decode_entities(&value)));
                }
            }
        }
        (attrs, self_closing)
    }
}

/// Builds a DOM from the token stream: a stack of open elements, void tags
/// auto-close, `p`/`li` close their own kind implicitly, and everything
/// unbalanced at EOF is dropped by construction (the stack unwinds).
pub fn parse_html(input: &str) -> Dom {
    let tokens = Tokenizer::new(input).tokenize();
    let mut dom = Dom::with_document();
    let mut stack: Vec<NodeId> = vec![dom.root];

    for token in tokens {
        match token {
            Token::Doctype => {}
            Token::Text(text) => {
                let parent = *stack.last().expect("stack invariant");
                let id = dom.create_text(text);
                dom.append(parent, id);
            }
            Token::StartTag { name, attrs, self_closing } => {
                if closes_implicitly(&name, dom.tag(*stack.last().expect("stack invariant"))) {
                    stack.pop();
                }
                let parent = *stack.last().expect("stack invariant");
                let id = dom.create_element(name.clone(), attrs);
                dom.append(parent, id);
                if !self_closing && !VOID.contains(&name.as_str()) {
                    stack.push(id);
                }
            }
            Token::EndTag { name } => {
                if let Some(idx) = stack.iter().rposition(|&n| dom.tag(n) == Some(name.as_str())) {
                    // Popping to the matched index also closes unclosed children.
                    stack.truncate(idx);
                }
            }
        }
    }
    dom
}

fn closes_implicitly(opening: &str, top: Option<&str>) -> bool {
    match (opening, top) {
        ("p", Some("p")) | ("li", Some("li")) | ("dt", Some("dt")) | ("dd", Some("dd")) => true,
        _ => false,
    }
}

fn collapse_ws(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_ws = false;
    for ch in text.chars() {
        if ch.is_ascii_whitespace() {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(ch);
            in_ws = false;
        }
    }
    // Whitespace-only nodes are pure separators and vanish; boundary spaces
    // around real words survive so "Hello <b>fox</b>" still reads "Hello fox".
    if out.trim().is_empty() {
        String::new()
    } else {
        out
    }
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let tail = &rest[start + 1..];
        let Some(end) = tail.find(';') else {
            out.push('&');
            out.push_str(tail);
            return out;
        };
        let entity = &tail[..end];
        rest = &tail[end + 1..];
        match entity {
            "amp" => out.push('&'),
            "lt" => out.push('<'),
            "gt" => out.push('>'),
            "quot" => out.push('"'),
            "apos" | "rsquo" => out.push('\''),
            "nbsp" => out.push('\u{00a0}'),
            "copy" => out.push('\u{00a9}'),
            "mdash" => out.push('\u{2014}'),
            "hellip" => out.push('\u{2026}'),
            name => {
                if let Some(code) = entity.strip_prefix('#') {
                    let radix = if let Some(hex) = code.strip_prefix('x').or_else(|| code.strip_prefix('X')) {
                        u32::from_str_radix(hex, 16)
                    } else {
                        code.parse::<u32>()
                    };
                    if let Ok(cp) = radix {
                        if let Some(decoded) = char::from_u32(cp) {
                            out.push(decoded);
                            continue;
                        }
                    }
                }
                out.push('&');
                out.push_str(name);
                out.push(';');
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_nested_tree() {
        let dom = parse_html("<html><body><p title=\"a&amp;b\">Hello <b>fox</b></p><br></body></html>");
        let body = dom.element_children(dom.root)[0];
        let body = dom.element_children(body)[0];
        let p = dom.element_children(body)[0];
        assert_eq!(dom.tag(p), Some("p"));
        assert_eq!(dom.attr(p, "title"), Some("a&b"));
        assert!(dom.inner_text(p).contains("Hello fox"));
    }

    #[test]
    fn entities_decode() {
        let dom = parse_html("<title>Rust &amp; Fox &#233;</title>");
        let title = dom.find_first_tag("title").unwrap();
        assert_eq!(dom.inner_text(title), "Rust & Fox é");
    }

    #[test]
    fn raw_text_survives_markup() {
        let dom = parse_html("<script>if (a < b && c) { d }</script><p>x</p>");
        let script = dom.find_first_tag("script").unwrap();
        assert!(dom.inner_text(script).contains("a < b"));
    }

    #[test]
    fn unbalanced_tags_are_dropped_not_looped() {
        let dom = parse_html("<div><span>unclosed<div>deep</div>");
        let mut count = 0;
        dom.walk(dom.root, &mut |_, _| count += 1);
        assert!(count >= 3);
    }
}
