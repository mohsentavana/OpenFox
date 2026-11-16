use serde::Serialize;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, OnceLock};

/// The browser surface an agent drives. Thirty-two operations, no ambient
/// powers: the same commands a user has, plus what a headless session needs.
pub trait Backend: Send {
    fn navigate(&mut self, url: &str) -> Result<Value, ToolError>;
    fn go(&mut self, direction: Direction) -> Result<Value, ToolError>;
    fn tabs_list(&mut self) -> Result<Value, ToolError>;
    fn tabs_new(&mut self, url: Option<&str>) -> Result<Value, ToolError>;
    fn tabs_close(&mut self, tab: u64) -> Result<Value, ToolError>;
    fn tabs_select(&mut self, tab: u64) -> Result<Value, ToolError>;
    fn dom_query(&mut self, selector: &str) -> Result<Value, ToolError>;
    fn dom_snapshot(&mut self, depth: u32) -> Result<Value, ToolError>;
    fn click(&mut self, selector: &str) -> Result<Value, ToolError>;
    fn type_text(&mut self, selector: Option<&str>, text: &str) -> Result<Value, ToolError>;
    /// PNG bytes of the current page, base64-encoded by the caller contract.
    fn screenshot(&mut self, full_page: bool) -> Result<Value, ToolError>;
    fn network_log(&mut self, filter: Option<&str>) -> Result<Value, ToolError>;
    fn console_log(&mut self, level: Option<u8>) -> Result<Value, ToolError>;
    fn adblock_stats(&mut self) -> Result<Value, ToolError>;
    fn history_search(&mut self, query: &str, limit: u32) -> Result<Value, ToolError>;
    fn ext_run(&mut self, wasm_base64: &str) -> Result<Value, ToolError>;

    // ---- the extended surface, tools 17-32 -----------------------------
    // These default to a refusal: a backend that hosts only part of the
    // browser (a kiosk build, a test double) implements the sixteen above
    // and still compiles. A full shell overrides the ones it can serve.

    /// `fox.find`: in-page find; returns the match count and the current
    /// match index for the active tab.
    fn find(&mut self, _query: &str, _direction: FindDirection) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.find"))
    }

    /// `fox.zoom.set`: the active tab's zoom level, already clamped to
    /// 0.3..=3.0 by the dispatcher.
    fn zoom_set(&mut self, _level: f32) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.zoom.set"))
    }

    /// `fox.reader.toggle`: reader mode on the active tab; reports
    /// availability and the estimated read time.
    fn reader_toggle(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.reader.toggle"))
    }

    /// `fox.reader.text`: the extracted article — title, byline, plain text.
    fn reader_text(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.reader.text"))
    }

    /// `fox.screenshot.region`: CSS-pixel viewport region, base64 PNG.
    fn screenshot_region(&mut self, _x: f32, _y: f32, _w: f32, _h: f32) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.screenshot.region"))
    }

    /// `fox.pdf.print`: the active page rendered to PDF, base64.
    fn pdf_print(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.pdf.print"))
    }

    /// `fox.downloads.list`: every download with state, progress, speed.
    fn downloads_list(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.downloads.list"))
    }

    /// `fox.downloads.control`: pause, resume, cancel or reveal by id.
    fn downloads_control(&mut self, _id: u64, _action: DownloadAction) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.downloads.control"))
    }

    /// `fox.media.control`: the media session — play, pause, seek, mute.
    fn media_control(&mut self, _action: MediaAction, _position_ms: Option<u64>) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.media.control"))
    }

    /// `fox.passwords.status`: vault statistics only — record count,
    /// breach flags, last import. Secret values never cross this interface:
    /// a tool result gets logged, cached and quoted, and passwords are
    /// none of those things.
    fn passwords_status(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.passwords.status"))
    }

    /// `fox.cert.inspect`: the TLS chain behind the active origin.
    fn cert_inspect(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.cert.inspect"))
    }

    /// `fox.cache.stats`: entries and bytes by type; clears first when
    /// asked.
    fn cache_stats(&mut self, _clear: bool) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.cache.stats"))
    }

    /// `fox.performance.trace`: frame timings over the interval.
    fn performance_trace(&mut self, _ms: u64) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.performance.trace"))
    }

    /// `fox.a11y.tree`: roles, names and bounds of the a11y tree.
    fn a11y_tree(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.a11y.tree"))
    }

    /// `fox.permissions.set`: grant, deny, or return an origin's
    /// permission to prompt.
    fn permissions_set(&mut self, _origin: &str, _permission: &str, _state: PermissionState) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.permissions.set"))
    }

    /// `fox.crash.log`: past crash reports — hash, timestamp, recovered
    /// session size.
    fn crash_log(&mut self) -> Result<Value, ToolError> {
        Err(ToolError::unsupported("fox.crash.log"))
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Back,
    Forward,
    Reload,
}

#[derive(Debug, Clone, Copy)]
pub enum FindDirection {
    Next,
    Prev,
}

#[derive(Debug, Clone, Copy)]
pub enum DownloadAction {
    Pause,
    Resume,
    Cancel,
    Reveal,
}

#[derive(Debug, Clone, Copy)]
pub enum MediaAction {
    Play,
    Pause,
    Seek,
    Mute,
}

#[derive(Debug, Clone, Copy)]
pub enum PermissionState {
    Grant,
    Deny,
    Prompt,
}

#[derive(Debug, thiserror::Error)]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    pub fn new(message: impl Into<String>) -> ToolError {
        ToolError { message: message.into() }
    }

    /// The default answer of the extended tools on a backend that does not
    /// host the surface they need. A straight refusal, not silence.
    fn unsupported(tool: &'static str) -> ToolError {
        ToolError { message: format!("{tool}: this backend does not expose that surface") }
    }
}

impl std::fmt::Display for ToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

#[derive(Debug, Serialize)]
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    pub input_schema: Value,
}

/// The catalog, exactly thirty-two tools. Names are `fox.*` because agents
/// deserve verbs that cannot collide with a page's own vocabulary.
pub fn catalog() -> &'static [Tool] {
    static TOOLS: OnceLock<Vec<Tool>> = OnceLock::new();
    TOOLS.get_or_init(|| {
        vec![
            Tool {
                name: "fox.navigate",
                description: "Navigate the active tab to a URL and return the page result (title, status, bytes, blocked).",
                input_schema: json!({
                    "type": "object",
                    "properties": { "url": { "type": "string", "description": "Absolute URL" } },
                    "required": ["url"]
                }),
            },
            Tool {
                name: "fox.go",
                description: "Walk tab history: back, forward, or reload.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "direction": { "type": "string", "enum": ["back", "forward", "reload"] } },
                    "required": ["direction"]
                }),
            },
            Tool {
                name: "fox.tabs.list",
                description: "List every open tab with id, title, address and load state.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.tabs.new",
                description: "Open a new tab, optionally starting at a URL.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "url": { "type": "string" } }
                }),
            },
            Tool {
                name: "fox.tabs.close",
                description: "Close a tab by id.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "tab": { "type": "integer", "description": "Tab id from fox.tabs.list" } },
                    "required": ["tab"]
                }),
            },
            Tool {
                name: "fox.tabs.select",
                description: "Make a tab active by id.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "tab": { "type": "integer" } },
                    "required": ["tab"]
                }),
            },
            Tool {
                name: "fox.dom.query",
                description: "Run a selector over the current DOM: tag, .class, #id and descendant chains. Returns matched nodes with text.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "selector": { "type": "string", "description": "CSS subset selector" } },
                    "required": ["selector"]
                }),
            },
            Tool {
                name: "fox.dom.snapshot",
                description: "Serialize the current DOM as a depth-bounded outline.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "depth": { "type": "integer", "minimum": 1, "maximum": 12, "default": 4 } }
                }),
            },
            Tool {
                name: "fox.click",
                description: "Actuate the first element matching a selector (state change, not a synthetic pixel).",
                input_schema: json!({
                    "type": "object",
                    "properties": { "selector": { "type": "string" } },
                    "required": ["selector"]
                }),
            },
            Tool {
                name: "fox.type",
                description: "Type text into the focused field, or into the first element matching a selector first.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "selector": { "type": "string", "description": "Optional target selector" },
                        "text": { "type": "string" }
                    },
                    "required": ["text"]
                }),
            },
            Tool {
                name: "fox.screenshot",
                description: "Render the current page to PNG and return it base64-encoded.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "full_page": { "type": "boolean", "default": true } }
                }),
            },
            Tool {
                name: "fox.network.log",
                description: "Return the network log, optionally filtered by substring, with timings and blocked flags.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "filter": { "type": "string" } }
                }),
            },
            Tool {
                name: "fox.console.log",
                description: "Return console entries, optionally filtered by minimum level (1=error, 2=warn, 3=info, 4=log).",
                input_schema: json!({
                    "type": "object",
                    "properties": { "level": { "type": "integer", "minimum": 1, "maximum": 4 } }
                }),
            },
            Tool {
                name: "fox.adblock.stats",
                description: "Shield counters: ads and trackers blocked, requests saved, estimated bytes saved.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.history.search",
                description: "Search stored history by title or URL, frecency-ranked.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 50, "default": 10 }
                    },
                    "required": ["query"]
                }),
            },
            Tool {
                name: "fox.ext.run",
                description: "Install and run a WASM extension from base64 bytes. Returns the extension's reported result.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "wasm_base64": { "type": "string" } },
                    "required": ["wasm_base64"]
                }),
            },
            Tool {
                name: "fox.find",
                description: "Find in page: returns the match count and the current match index for the active tab.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string" },
                        "direction": { "type": "string", "enum": ["next", "prev"], "default": "next" }
                    },
                    "required": ["query"]
                }),
            },
            Tool {
                name: "fox.zoom.set",
                description: "Set the active tab's zoom level, clamped to 0.3..3.0 (30%..300%).",
                input_schema: json!({
                    "type": "object",
                    "properties": { "level": { "type": "number", "minimum": 0.3, "maximum": 3.0 } },
                    "required": ["level"]
                }),
            },
            Tool {
                name: "fox.reader.toggle",
                description: "Toggle reader mode on the active tab. Returns availability and the estimated read time in minutes.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.reader.text",
                description: "The extracted reader article for the active tab: title, byline and plain text.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.screenshot.region",
                description: "Capture a viewport region as base64 PNG. Coordinates are CSS pixels from the top-left.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "x": { "type": "number" },
                        "y": { "type": "number" },
                        "w": { "type": "number" },
                        "h": { "type": "number" }
                    },
                    "required": ["x", "y", "w", "h"]
                }),
            },
            Tool {
                name: "fox.pdf.print",
                description: "Render the active page to PDF and return it base64-encoded.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.downloads.list",
                description: "List every download with state, progress and current speed.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.downloads.control",
                description: "Pause, resume, cancel or reveal a download by id.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "id": { "type": "integer" },
                        "action": { "type": "string", "enum": ["pause", "resume", "cancel", "reveal"] }
                    },
                    "required": ["id", "action"]
                }),
            },
            Tool {
                name: "fox.media.control",
                description: "Drive the media session of the active tab: play, pause, seek or mute.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "action": { "type": "string", "enum": ["play", "pause", "seek", "mute"] },
                        "position_ms": { "type": "integer" }
                    },
                    "required": ["action"]
                }),
            },
            Tool {
                name: "fox.passwords.status",
                description: "Password vault statistics only: record count, breach flags, last import. Secret values never cross this interface.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.cert.inspect",
                description: "TLS certificate chain summary for the active origin: subject, issuer, validity, SANs and the error taxonomy when validation failed.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.cache.stats",
                description: "Cache entries and bytes by type; optionally clears the caches before counting.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "clear": { "type": "boolean", "default": false } }
                }),
            },
            Tool {
                name: "fox.performance.trace",
                description: "Frame timings over an interval: mean, p95, max and frames over budget.",
                input_schema: json!({
                    "type": "object",
                    "properties": { "ms": { "type": "integer", "minimum": 100, "default": 1000 } }
                }),
            },
            Tool {
                name: "fox.a11y.tree",
                description: "Accessibility tree snapshot for the active tab: roles, names and bounds.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
            Tool {
                name: "fox.permissions.set",
                description: "Set a permission for an origin: grant, deny, or return to prompt.",
                input_schema: json!({
                    "type": "object",
                    "properties": {
                        "origin": { "type": "string" },
                        "permission": { "type": "string" },
                        "state": { "type": "string", "enum": ["grant", "deny", "prompt"] }
                    },
                    "required": ["origin", "permission", "state"]
                }),
            },
            Tool {
                name: "fox.crash.log",
                description: "Past crash reports: content hash, timestamp and the size of the recovered session.",
                input_schema: json!({ "type": "object", "properties": {} }),
            },
        ]
    })
}

pub fn catalog_serialized() -> Value {
    serde_json::to_value(catalog()).expect("tool catalog serializes")
}

pub fn call(backend: &Arc<Mutex<dyn Backend>>, name: &str, params: &Value) -> Result<Value, ToolError> {
    let mut backend = backend.lock().expect("mcp backend lock");
    let str_param = |key: &str| -> Result<String, ToolError> {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| ToolError::new(format!("missing string parameter: {key}")))
    };
    let opt_str_param = |key: &str| -> Result<Option<String>, ToolError> {
        match params.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(ToolError::new(format!("parameter {key} must be a string"))),
        }
    };
    let int_param = |key: &str| -> Result<u64, ToolError> {
        params
            .get(key)
            .and_then(Value::as_u64)
            .ok_or_else(|| ToolError::new(format!("missing integer parameter: {key}")))
    };
    let num_param = |key: &str| -> Result<f32, ToolError> {
        params
            .get(key)
            .and_then(Value::as_f64)
            .map(|value| value as f32)
            .ok_or_else(|| ToolError::new(format!("missing number parameter: {key}")))
    };

    match name {
        "fox.navigate" => backend.navigate(&str_param("url")?),
        "fox.go" => {
            let direction = match str_param("direction")?.as_str() {
                "back" => Direction::Back,
                "forward" => Direction::Forward,
                "reload" => Direction::Reload,
                other => return Err(ToolError::new(format!("direction must be back|forward|reload, got {other}"))),
            };
            backend.go(direction)
        }
        "fox.tabs.list" => backend.tabs_list(),
        "fox.tabs.new" => {
            let url = opt_str_param("url")?;
            backend.tabs_new(url.as_deref())
        }
        "fox.tabs.close" => backend.tabs_close(int_param("tab")?),
        "fox.tabs.select" => backend.tabs_select(int_param("tab")?),
        "fox.dom.query" => backend.dom_query(&str_param("selector")?),
        "fox.dom.snapshot" => {
            let depth = params.get("depth").and_then(Value::as_u64).unwrap_or(4) as u32;
            backend.dom_snapshot(depth.clamp(1, 12))
        }
        "fox.click" => backend.click(&str_param("selector")?),
        "fox.type" => {
            let selector = opt_str_param("selector")?;
            backend.type_text(selector.as_deref(), &str_param("text")?)
        }
        "fox.screenshot" => {
            let full = params.get("full_page").and_then(Value::as_bool).unwrap_or(true);
            backend.screenshot(full)
        }
        "fox.network.log" => {
            let filter = opt_str_param("filter")?;
            backend.network_log(filter.as_deref())
        }
        "fox.console.log" => {
            let level = params.get("level").and_then(Value::as_u64).map(|l| l as u8);
            backend.console_log(level)
        }
        "fox.adblock.stats" => backend.adblock_stats(),
        "fox.history.search" => {
            let limit = params.get("limit").and_then(Value::as_u64).unwrap_or(10) as u32;
            backend.history_search(&str_param("query")?, limit.clamp(1, 50))
        }
        "fox.ext.run" => backend.ext_run(&str_param("wasm_base64")?),
        "fox.find" => {
            let direction = match opt_str_param("direction")?.as_deref() {
                None | Some("next") => FindDirection::Next,
                Some("prev") => FindDirection::Prev,
                Some(other) => return Err(ToolError::new(format!("direction must be next|prev, got {other}"))),
            };
            backend.find(&str_param("query")?, direction)
        }
        "fox.zoom.set" => {
            let level = num_param("level")?;
            backend.zoom_set(level.clamp(0.3, 3.0))
        }
        "fox.reader.toggle" => backend.reader_toggle(),
        "fox.reader.text" => backend.reader_text(),
        "fox.screenshot.region" => {
            let (x, y, w, h) = (num_param("x")?, num_param("y")?, num_param("w")?, num_param("h")?);
            backend.screenshot_region(x, y, w, h)
        }
        "fox.pdf.print" => backend.pdf_print(),
        "fox.downloads.list" => backend.downloads_list(),
        "fox.downloads.control" => {
            let action = match str_param("action")?.as_str() {
                "pause" => DownloadAction::Pause,
                "resume" => DownloadAction::Resume,
                "cancel" => DownloadAction::Cancel,
                "reveal" => DownloadAction::Reveal,
                other => return Err(ToolError::new(format!("action must be pause|resume|cancel|reveal, got {other}"))),
            };
            backend.downloads_control(int_param("id")?, action)
        }
        "fox.media.control" => {
            let action = match str_param("action")?.as_str() {
                "play" => MediaAction::Play,
                "pause" => MediaAction::Pause,
                "seek" => MediaAction::Seek,
                "mute" => MediaAction::Mute,
                other => return Err(ToolError::new(format!("action must be play|pause|seek|mute, got {other}"))),
            };
            let position_ms = params.get("position_ms").and_then(Value::as_u64);
            backend.media_control(action, position_ms)
        }
        "fox.passwords.status" => backend.passwords_status(),
        "fox.cert.inspect" => backend.cert_inspect(),
        "fox.cache.stats" => {
            let clear = params.get("clear").and_then(Value::as_bool).unwrap_or(false);
            backend.cache_stats(clear)
        }
        "fox.performance.trace" => {
            let ms = params.get("ms").and_then(Value::as_u64).unwrap_or(1000);
            backend.performance_trace(ms.max(100))
        }
        "fox.a11y.tree" => backend.a11y_tree(),
        "fox.permissions.set" => {
            let state = match str_param("state")?.as_str() {
                "grant" => PermissionState::Grant,
                "deny" => PermissionState::Deny,
                "prompt" => PermissionState::Prompt,
                other => return Err(ToolError::new(format!("state must be grant|deny|prompt, got {other}"))),
            };
            backend.permissions_set(&str_param("origin")?, &str_param("permission")?, state)
        }
        "fox.crash.log" => backend.crash_log(),
        unknown => Err(ToolError::new(format!("unknown tool: {unknown}"))),
    }
}
