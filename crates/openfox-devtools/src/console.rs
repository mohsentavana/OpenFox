use serde_json::{json, Value};
use std::collections::VecDeque;

pub const LEVEL_ERROR: u8 = 1;
pub const LEVEL_WARN: u8 = 2;
pub const LEVEL_INFO: u8 = 3;
pub const LEVEL_LOG: u8 = 4;

const CAP: usize = 512;

#[derive(Debug, Clone)]
pub struct ConsoleEntry {
    pub level: u8,
    pub text: String,
    pub ts_ms: u64,
}

/// A bounded console. Pages can log faster than any human can read; the
/// oldest entries go first and the count never lies about what happened.
pub struct Console {
    entries: VecDeque<ConsoleEntry>,
    epoch: std::time::Instant,
}

impl Console {
    pub fn new() -> Self {
        Console { entries: VecDeque::with_capacity(CAP), epoch: std::time::Instant::now() }
    }

    pub fn log(&mut self, level: u8, text: impl Into<String>) {
        let ts_ms = self.epoch.elapsed().as_millis() as u64;
        if self.entries.len() == CAP {
            self.entries.pop_front();
        }
        self.entries.push_back(ConsoleEntry { level, text: text.into(), ts_ms });
    }

    pub fn error(&mut self, text: impl Into<String>) {
        self.log(LEVEL_ERROR, text);
    }

    pub fn warn(&mut self, text: impl Into<String>) {
        self.log(LEVEL_WARN, text);
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.log(LEVEL_INFO, text);
    }

    /// Entries at or above a level, oldest first.
    pub fn entries(&self, min_level: Option<u8>) -> Vec<&ConsoleEntry> {
        let min = min_level.unwrap_or(LEVEL_LOG);
        self.entries.iter().filter(|entry| entry.level <= min).collect()
    }

    pub fn to_json(&self, min_level: Option<u8>) -> Value {
        let entries: Vec<Value> = self
            .entries(min_level)
            .into_iter()
            .map(|entry| {
                json!({
                    "level": entry.level,
                    "levelName": level_name(entry.level),
                    "text": entry.text,
                    "ts": entry.ts_ms,
                })
            })
            .collect();
        json!({ "entries": entries })
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for Console {
    fn default() -> Self {
        Console::new()
    }
}

fn level_name(level: u8) -> &'static str {
    match level {
        LEVEL_ERROR => "error",
        LEVEL_WARN => "warn",
        LEVEL_INFO => "info",
        _ => "log",
    }
}
