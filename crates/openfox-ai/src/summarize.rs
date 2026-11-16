//! The local extractive summarizer — the assistant that works with the
//! network unplugged. No model, no download, no inference runtime: split
//! the text into sentences, count how often each content word appears,
//! keep the sentences that carry the most of the page's own vocabulary,
//! and return them in reading order.
//!
//! Why extractive and not abstractive: a summarizer that can only quote
//! the page cannot hallucinate a quote. For a browser that puts page
//! text inside an assistant prompt, that property is the whole point —
//! the fallback provider must never be the attack surface.

/// English stopwords, kept as one flat const so the list is auditable at
/// a glance. Function words only; every word here is a word no sentence
/// should win a relevance contest with.
const STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "but", "by", "can", "could", "did", "do", "does",
    "for", "from", "had", "has", "have", "he", "her", "his", "how", "i", "if", "in", "into",
    "is", "it", "its", "just", "may", "might", "more", "most", "must", "no", "not", "of", "on",
    "or", "our", "out", "said", "she", "should", "so", "some", "than", "that", "the", "their",
    "them", "then", "there", "these", "they", "this", "to", "was", "we", "were", "what", "when",
    "where", "which", "while", "who", "why", "will", "with", "would", "you", "your",
];

/// Sentences shorter than this are noise — headings, list bullets,
/// captions — and never candidates for the summary.
const MIN_SENTENCE_CHARS: usize = 12;

/// A newline only splits a sentence when the accumulated span is at
/// least this long: in extracted page text a newline is a block boundary,
/// but wrapped prose should not shatter on it.
const MIN_SPLIT_SPAN: usize = 40;

/// Split text into sentence-ish spans on `.`, `!`, `?` and paragraph
/// breaks, keeping the delimiter with its sentence. A terminator only
/// ends a sentence when whitespace or end-of-text follows — "3.5" and
/// "e.g." stay glued, which is the right failure mode for prose.
pub fn split_sentences(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        let terminated = byte == b'.' || byte == b'!' || byte == b'?';
        let newline_break = byte == b'\n' && text[start..i].trim().len() >= MIN_SPLIT_SPAN;
        if terminated || newline_break {
            let next_is_space = bytes
                .get(i + 1)
                .map_or(true, |&b| b == b' ' || b == b'\n' || b == b'\t' || b == b'\r');
            if terminated && next_is_space {
                // Consume trailing spaces so joins are clean.
                let mut end = i + 1;
                while end < bytes.len()
                    && (bytes[end] == b' ' || bytes[end] == b'\t')
                {
                    end += 1;
                }
                push_sentence(&mut sentences, &text[start..end], &mut start);
                i = end;
                continue;
            }
            if newline_break {
                let end = i + 1;
                push_sentence(&mut sentences, &text[start..end], &mut start);
                i = end;
                continue;
            }
        }
        i += 1;
    }
    if start < text.len() {
        push_sentence(&mut sentences, &text[start..], &mut start);
    }
    sentences
}

fn push_sentence<'a>(out: &mut Vec<&'a str>, span: &'a str, start: &mut usize) {
    let trimmed = span.trim();
    if trimmed.chars().count() >= MIN_SENTENCE_CHARS {
        out.push(trimmed);
    }
    *start += span.len();
}

/// Summarize `text` to its `top_n` highest-scoring sentences, returned
/// in original order — a summary that jumps around reads like a bug,
/// not a summary. Sentences shorter than a clause are never candidates;
/// neither is an empty corpus.
pub fn summarize(text: &str, top_n: usize) -> String {
    let sentences = split_sentences(text);
    if sentences.is_empty() || top_n == 0 {
        return String::new();
    }
    if sentences.len() <= top_n {
        return sentences.join(" ");
    }

    // Term frequency over the whole text, lowercased, stopwords and
    // punctuation stripped. Words are the unit because pages repeat
    // topics, not characters.
    let mut freq: std::collections::HashMap<String, usize> =
        std::collections::HashMap::new();
    for word in content_words(text) {
        *freq.entry(word).or_default() += 1;
    }

    // Score = average term weight, so a long sentence cannot win by
    // stacking the page's nouns; relevance is density, not volume.
    let mut scored: Vec<(usize, f32)> = sentences
        .iter()
        .enumerate()
        .map(|(index, sentence)| {
            let words = content_words(sentence);
            let total: usize = words
                .iter()
                .map(|w| freq.get(w).copied().unwrap_or(0))
                .sum();
            let score = if words.is_empty() {
                0.0
            } else {
                total as f32 / words.len() as f32
            };
            (index, score)
        })
        .collect();

    // Top-n by score, ties broken toward the earlier sentence, then
    // restored to reading order.
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut picked: Vec<usize> = scored[..top_n].iter().map(|(i, _)| *i).collect();
    picked.sort_unstable();
    picked
        .into_iter()
        .map(|i| sentences[i])
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lowercased, stopword-free words of `text`. Unicode letters only —
/// punctuation peels off with `is_alphanumeric`, so "don't" yields
/// "don" and "t", which is fine: both sides of the comparison strip the
/// same way, and the frequency table stays consistent with itself.
fn content_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 2 && !STOPWORDS.contains(w))
        .map(str::to_owned)
        .collect()
}
