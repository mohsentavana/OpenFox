//! The headless loop: no window, no GPU surface, MCP-driven. The same
//! engine, the same net stack, the same shield — a screenshot from a
//! headless run and a screenshot from a desktop run come out of the
//! same display list. Agents get the browser, not an imitation.

use std::sync::{Arc, Mutex};

/// Exit codes, documented where they are born: 0 clean, 2 bad url, 3
/// profile lock. An agent reading stderr deserves numbers that mean
/// something.
pub const EXIT_CLEAN: i32 = 0;
pub const EXIT_BAD_URL: i32 = 2;
pub const EXIT_PROFILE_LOCK: i32 = 3;

/// The fixed farbling seed for headless runs. Determinism is the
/// feature: two runs of the same headless session produce byte-identical
/// screenshots, because canvas jitter keys off this constant instead of
/// a per-boot random.
pub const HEADLESS_FARBLING_SEED: [u8; 32] = [0xF0; 32];

pub struct HeadlessLoop {
    backend: Arc<Mutex<dyn openfox_mcp::Backend>>,
}

impl HeadlessLoop {
    pub fn new(backend: Arc<Mutex<dyn openfox_mcp::Backend>>) -> Self {
        HeadlessLoop { backend }
    }

    /// A second handle for the HTTP transport: the backend is shared,
    /// so both transports drive the same browser state — one process,
    /// two front doors, zero divergence.
    pub fn clone_for_http(&self) -> HeadlessLoop {
        HeadlessLoop {
            backend: self.backend.clone(),
        }
    }

    /// Serve MCP stdio until the transport closes. This is the loop an
    /// agent drives: every tool call lands in the same backend the
    /// desktop window uses.
    pub async fn serve_stdio(&self) -> Result<(), std::io::Error> {
        openfox_mcp::stdio::serve(self.backend.clone()).await
    }

    /// Serve MCP over localhost HTTP — the CI path, where stdio is
    /// awkward and a port is easy.
    pub async fn serve_http(&self, port: u16) -> Result<(), std::io::Error> {
        openfox_mcp::http::serve(self.backend.clone(), port).await
    }

    /// One-shot mode for scripts: navigate, render, screenshot, exit.
    /// The exit code mirrors the navigation outcome so shell scripts
    /// can branch without parsing JSON.
    pub fn run_once(&self, url: &str, out_png: &std::path::Path) -> i32 {
        let Ok(mut backend) = self.backend.lock() else {
            return EXIT_PROFILE_LOCK;
        };
        match backend.navigate(url) {
            Ok(summary) => {
                if let Some(status) = summary.get("status").and_then(|s| s.as_u64()) {
                    if !(200..300).contains(&status) {
                        return EXIT_BAD_URL;
                    }
                }
                match backend.screenshot(true) {
                    Ok(png) => {
                        let bytes = decode_b64(png.as_str().unwrap_or_default());
                        if std::fs::write(out_png, bytes).is_err() {
                            EXIT_BAD_URL
                        } else {
                            EXIT_CLEAN
                        }
                    }
                    Err(_) => EXIT_BAD_URL,
                }
            }
            Err(_) => EXIT_BAD_URL,
        }
    }
}

/// Standard base64 decode for the screenshot payloads MCP emits. The
/// transport speaks JSON; the pixels inside are still bytes.
fn decode_b64(text: &str) -> Vec<u8> {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;
    for byte in text.bytes() {
        if byte == b'=' {
            break;
        }
        let Some(value) = ALPHABET.iter().position(|&a| a == byte) else {
            continue;
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    out
}
