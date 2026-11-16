//! The JavaScript debugger: breakpoints, step granularity, paused state
//! with scopes, and the source registry.

use serde_json::{json, Value};

/// A breakpoint. `condition` is a JS expression the engine evaluates at
/// the breakpoint site before pausing — the debugger never evaluates
/// page script itself.
#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub id: u64,
    pub url: String,
    pub line: u32,
    pub col: u32,
    pub condition: Option<String>,
    pub hits: u64,
}

/// Step granularity for the next engine step after a pause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepGranularity {
    Into,
    Over,
    Out,
}

/// One lexical scope as the debugger shows it: a name and its variables,
/// values already stringified by the engine's value formatter.
#[derive(Debug, Clone)]
pub struct Scope {
    pub name: String,
    pub vars: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct StackFrame {
    pub function: String,
    pub url: String,
    pub line: u32,
    pub scopes: Vec<Scope>,
}

#[derive(Debug, Clone)]
pub struct PausedState {
    pub reason: String,
    pub frames: Vec<StackFrame>,
}

/// One registered source. `has_map` records that a source map arrived with
/// the file: mapped positions are honored when present, and raw generated
/// lines show otherwise — the registry stores what shipped, and the map
/// resolves at display time.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub url: String,
    pub lines: Vec<String>,
    pub has_map: bool,
}

/// The debugger model. The engine reports hits and pauses; this struct
/// keeps the bookkeeping the panel renders.
pub struct JsDebugger {
    breakpoints: Vec<Breakpoint>,
    next_id: u64,
    paused: Option<PausedState>,
    sources: Vec<SourceFile>,
    pending_step: Option<StepGranularity>,
}

impl JsDebugger {
    pub fn new() -> JsDebugger {
        JsDebugger { breakpoints: Vec::new(), next_id: 1, paused: None, sources: Vec::new(), pending_step: None }
    }

    pub fn add_breakpoint(
        &mut self,
        url: impl Into<String>,
        line: u32,
        col: u32,
        condition: Option<String>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.breakpoints.push(Breakpoint { id, url: url.into(), line, col, condition, hits: 0 });
        id
    }

    pub fn remove_breakpoint(&mut self, id: u64) -> bool {
        let before = self.breakpoints.len();
        self.breakpoints.retain(|breakpoint| breakpoint.id != id);
        before != self.breakpoints.len()
    }

    pub fn breakpoints(&self) -> &[Breakpoint] {
        &self.breakpoints
    }

    /// Records a breakpoint hit at a location; returns the breakpoint id
    /// when the engine should pause. Conditional breakpoints pause on
    /// every hit — the engine decides whether the condition holds.
    pub fn record_hit(&mut self, url: &str, line: u32) -> Option<u64> {
        self.breakpoints
            .iter_mut()
            .find(|breakpoint| breakpoint.url == url && breakpoint.line == line)
            .map(|breakpoint| {
                breakpoint.hits += 1;
                breakpoint.id
            })
    }

    pub fn pause(&mut self, reason: impl Into<String>, frames: Vec<StackFrame>) {
        self.paused = Some(PausedState { reason: reason.into(), frames });
    }

    /// Resumes execution; true when there was a pause to leave.
    pub fn resume(&mut self) -> bool {
        self.pending_step = None;
        self.paused.take().is_some()
    }

    /// Queues the granularity for the engine's next step. Steps only make
    /// sense while paused; a step without a pause is ignored, not stored.
    pub fn step(&mut self, granularity: StepGranularity) {
        if self.paused.is_some() {
            self.pending_step = Some(granularity);
        }
    }

    pub fn paused(&self) -> Option<&PausedState> {
        self.paused.as_ref()
    }

    pub fn pending_step(&self) -> Option<StepGranularity> {
        self.pending_step
    }

    /// Registers a fetched source, split into lines for display.
    pub fn register_source(&mut self, url: impl Into<String>, text: &str, has_map: bool) {
        let url = url.into();
        let lines = text.lines().map(str::to_owned).collect();
        match self.sources.iter_mut().find(|source| source.url == url) {
            Some(source) => {
                source.lines = lines;
                source.has_map = has_map;
            }
            None => self.sources.push(SourceFile { url, lines, has_map }),
        }
    }

    /// One 1-based line of a source, for the source view's gutter sync.
    pub fn source_line(&self, url: &str, line: u32) -> Option<&str> {
        let source = self.sources.iter().find(|source| source.url == url)?;
        source.lines.get(line.checked_sub(1)? as usize).map(String::as_str)
    }

    pub fn to_json(&self) -> Value {
        let breakpoints: Vec<Value> = self
            .breakpoints
            .iter()
            .map(|breakpoint| {
                json!({
                    "id": breakpoint.id,
                    "url": breakpoint.url,
                    "line": breakpoint.line,
                    "col": breakpoint.col,
                    "condition": breakpoint.condition,
                    "hits": breakpoint.hits,
                })
            })
            .collect();
        let paused = self.paused.as_ref().map(|state| {
            json!({
                "reason": state.reason,
                "frames": state.frames.iter().map(|frame| {
                    json!({
                        "function": frame.function,
                        "url": frame.url,
                        "line": frame.line,
                        "scopes": frame.scopes.iter().map(|scope| {
                            json!({ "name": scope.name, "vars": scope.vars })
                        }).collect::<Vec<_>>(),
                    })
                }).collect::<Vec<_>>(),
            })
        });
        json!({ "breakpoints": breakpoints, "paused": paused, "sources": self.sources.len() })
    }
}

impl Default for JsDebugger {
    fn default() -> Self {
        JsDebugger::new()
    }
}
