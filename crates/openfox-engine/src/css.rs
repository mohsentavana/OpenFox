/// A CSS parser covering the subset that ships pages: selector lists with
/// descendant and child combinators, class/id/tag/simple compounds,
/// declarations with `!important` stripped at parse time, `@media` blocks
/// skipped whole, `/* comments */` removed anywhere.
#[derive(Debug, Clone)]
pub struct Stylesheet {
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone)]
pub struct Rule {
    pub selectors: Vec<Selector>,
    pub declarations: Vec<Declaration>,
}

/// A selector is a chain of parts; each part carries the combinator that
/// links it to the part on its left.
#[derive(Debug, Clone)]
pub struct Selector {
    pub parts: Vec<SelectorPart>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    Descendant,
    Child,
}

#[derive(Debug, Clone)]
pub struct SelectorPart {
    pub combinator: Combinator,
    pub simple: SimpleSelector,
}

#[derive(Debug, Clone, Default)]
pub struct SimpleSelector {
    pub tag: Option<String>,
    pub classes: Vec<String>,
    pub id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Declaration {
    pub property: String,
    pub value: String,
}

pub fn parse_stylesheet(input: &str) -> Stylesheet {
    let cleaned = strip_comments(input);
    let mut rules = Vec::new();
    let mut chars = cleaned.chars().peekable();

    loop {
        skip_ws(&mut chars);
        match chars.peek() {
            None => break,
            Some('@') => {
                let at = take_while(&mut chars, |c| !c.is_whitespace() && c != '{' && c != ';');
                match chars.peek() {
                    Some('{') => skip_block(&mut chars),
                    Some(';') => {
                        chars.next();
                    }
                    _ => {}
                }
                // @media applies at breakpoint matching, which phase 1 does
                // not have: the block is skipped whole, never guessed at.
                drop(at);
            }
            Some(_) => {
                if let Some(rule) = parse_rule(&mut chars) {
                    // A rule with an empty declaration block is still a
                    // rule: its selector is real (the DOM query path is
                    // built on exactly that shape), and CSS itself keeps
                    // `h1 {}` as a valid rule that matches nothing new.
                    rules.push(rule);
                }
            }
        }
    }
    Stylesheet { rules }
}

fn parse_rule<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) -> Option<Rule> {
    let selector_text = take_while(chars, |c| c != '{');
    if selector_text.trim().is_empty() {
        return None;
    }
    let selectors: Vec<Selector> = selector_text
        .split(',')
        .map(parse_selector)
        .filter(|s| !s.parts.is_empty())
        .collect();
    if selectors.is_empty() || chars.peek() != Some(&'{') {
        return None;
    }
    chars.next();

    let mut declarations = Vec::new();
    loop {
        skip_ws(chars);
        match chars.peek() {
            None | Some('}') => {
                chars.next();
                break;
            }
            _ => {
                let property = take_while(chars, |c| c != ':' && c != '}' && c != ';');
                skip_ws(chars);
                if chars.peek() == Some(&':') {
                    chars.next();
                }
                skip_ws(chars);
                let mut value = take_while(chars, |c| c != ';' && c != '}');
                if let Some(i) = value.find('!') {
                    value.truncate(i);
                }
                value = value.trim().to_owned();
                let property = property.trim().to_ascii_lowercase();
                if !property.is_empty() && !value.is_empty() {
                    declarations.push(Declaration { property, value });
                }
                match chars.peek() {
                    Some(';') => {
                        chars.next();
                    }
                    Some('}') => {
                        chars.next();
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
    Some(Rule { selectors, declarations })
}

fn parse_selector(text: &str) -> Selector {
    let mut parts: Vec<SelectorPart> = Vec::new();
    let mut combinator = Combinator::Descendant;
    let mut token = String::new();

    for ch in text.chars() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                if !token.is_empty() {
                    if let Some(part) = parse_simple(&token, combinator) {
                        parts.push(part);
                    }
                    token.clear();
                    combinator = Combinator::Descendant;
                }
            }
            '>' => {
                if !token.is_empty() {
                    if let Some(part) = parse_simple(&token, combinator) {
                        parts.push(part);
                    }
                    token.clear();
                }
                combinator = Combinator::Child;
            }
            _ => token.push(ch),
        }
    }
    if !token.is_empty() {
        if let Some(part) = parse_simple(&token, combinator) {
            parts.push(part);
        }
    }
    Selector { parts }
}

fn parse_simple(token: &str, combinator: Combinator) -> Option<SelectorPart> {
    let mut simple = SimpleSelector::default();
    let mut rest = token;
    if let Some(stripped) = rest.strip_prefix('*') {
        rest = stripped;
    } else {
        let tag_len = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').count();
        if tag_len > 0 {
            simple.tag = Some(rest[..tag_len].to_ascii_lowercase());
            rest = &rest[tag_len..];
        }
    }
    while let Some(dollar) = rest.find(&['.', '#'][..]) {
        let (before, after) = rest.split_at(dollar);
        if !before.is_empty() {
            return None;
        }
        let kind = after[..1].to_owned();
        rest = &after[1..];
        let name_len = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').count();
        if name_len == 0 {
            return None;
        }
        let name = rest[..name_len].to_owned();
        if kind == "." {
            simple.classes.push(name.to_ascii_lowercase());
        } else {
            simple.id = Some(name);
        }
        rest = &rest[name_len..];
    }
    if !rest.is_empty() {
        return None;
    }
    if simple.tag.is_none() && simple.classes.is_empty() && simple.id.is_none() {
        return None;
    }
    Some(SelectorPart { combinator, simple })
}

fn skip_ws<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) {
    while chars.peek().is_some_and(|&c| c.is_whitespace()) {
        chars.next();
    }
}

fn take_while<I: Iterator<Item = char>>(
    chars: &mut std::iter::Peekable<I>,
    pred: impl Fn(char) -> bool,
) -> String {
    let mut out = String::new();
    while let Some(&c) = chars.peek() {
        if pred(c) {
            out.push(c);
            chars.next();
        } else {
            break;
        }
    }
    out
}

fn skip_block<I: Iterator<Item = char>>(chars: &mut std::iter::Peekable<I>) {
    let mut depth = 0usize;
    while let Some(&c) = chars.peek() {
        chars.next();
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return;
                }
            }
            _ => {}
        }
    }
}

fn strip_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(start) = rest.find("/*") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        match after.find("*/") {
            Some(end) => {
                rest = &after[end + 2..];
            }
            None => {
                rest = "";
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
    fn parses_rules_and_skips_media() {
        let sheet = parse_stylesheet(
            "body { margin: 0; color: #222 } \n\
             @media (min-width: 600px) { .sidebar { width: 200px } } \n\
             .post > .title, #main h1 { font-weight: bold !important; }",
        );
        assert_eq!(sheet.rules.len(), 2);
        let body = &sheet.rules[0];
        assert_eq!(body.selectors[0].parts[0].simple.tag.as_deref(), Some("body"));
        assert_eq!(body.declarations.len(), 2);

        let compound = &sheet.rules[1];
        assert_eq!(compound.selectors.len(), 2);
        let first = &compound.selectors[0];
        assert_eq!(first.parts.len(), 2);
        assert_eq!(first.parts[1].combinator, Combinator::Child);
        let title = &first.parts[1].simple;
        assert_eq!(title.classes, vec!["title".to_owned()]);
        let imp = &compound.declarations[0];
        assert_eq!(imp.value, "bold");
    }

    #[test]
    fn comments_are_stripped_anywhere() {
        let sheet = parse_stylesheet("/* a */ p { color: /* b */ red; }");
        assert_eq!(sheet.rules.len(), 1);
        assert_eq!(sheet.rules[0].declarations[0].value, "red");
    }
}
