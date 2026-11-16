pub mod jsonrpc;
pub mod stdio;
pub mod tools;
pub mod http;

pub use tools::{Backend, Direction, Tool, ToolError};

pub const PROTOCOL_VERSION: &str = "2025-06-18";
pub const SERVER_NAME: &str = "openfox";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

use std::sync::{Arc, Mutex};

/// One protocol message in, one response line out — shared by both
/// transports so stdio and HTTP cannot drift apart. Notifications (no id)
/// produce no output, per the spec.
pub fn handle_message(backend: &Arc<Mutex<dyn Backend>>, body: &str) -> Option<String> {
    let request: jsonrpc::Request = match serde_json::from_str(body) {
        Ok(request) => request,
        Err(_) => {
            return Some(serde_json::to_string(&jsonrpc::Response::error(
                None,
                jsonrpc::PARSE_ERROR,
                "parse error",
            )).expect("error response serializes"));
        }
    };
    let Some(id) = request.id.clone() else {
        return None;
    };

    let (result, error) = match request.method.as_str() {
        "initialize" => (
            Some(serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION,
                },
            })),
            None,
        ),
        "ping" => (Some(serde_json::json!({})), None),
        "tools/list" => (Some(serde_json::json!({ "tools": tools::catalog_serialized() })), None),
        "tools/call" => {
            let name = request.params.get("name").and_then(|v| v.as_str()).unwrap_or_default();
            let arguments = request.params.get("arguments").cloned().unwrap_or(serde_json::json!({}));
            match tools::call(backend, name, &arguments) {
                Ok(value) => (
                    Some(serde_json::json!({
                        "content": [ { "type": "text", "text": serde_json::to_string_pretty(&value).expect("tool result serializes") } ],
                        "isError": false,
                    })),
                    None,
                ),
                Err(tool_error) => (
                    Some(serde_json::json!({
                        "content": [ { "type": "text", "text": tool_error.message } ],
                        "isError": true,
                    })),
                    None,
                ),
            }
        }
        method => (
            None,
            Some((jsonrpc::METHOD_NOT_FOUND, format!("unknown method: {method}"))),
        ),
    };

    let response = match error {
        Some((code, message)) => jsonrpc::Response::error(Some(id), code, &message),
        None => jsonrpc::Response::ok(Some(id), result.unwrap_or(serde_json::json!({}))),
    };
    Some(serde_json::to_string(&response).expect("response serializes"))
}
