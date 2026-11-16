pub mod a11y_inspector;
pub mod console;
pub mod debugger;
pub mod fdp;
pub mod inspector;
pub mod network;
pub mod profiler;
pub mod storage_inspector;

pub use a11y_inspector::A11yInspector;
pub use console::Console;
pub use debugger::JsDebugger;
pub use fdp::{handle as fdp_handle, FdpError};
pub use inspector::DomInspector;
pub use network::{NetworkLog, RequestRecord, WaterfallBar};
pub use profiler::FrameProfiler;
pub use storage_inspector::StorageInspector;

use serde_json::{json, Value};

/// The devtools aggregate. Devtools are a client of the browser, not a fork
/// of it: the FDP socket, the MCP tools and the in-app panels all read the
/// same three buffers.
pub struct Devtools {
    pub console: Console,
    pub network: NetworkLog,
    pub profiler: FrameProfiler,
}

impl Devtools {
    pub fn new() -> Self {
        Devtools { console: Console::new(), network: NetworkLog::new(), profiler: FrameProfiler::new() }
    }

    pub fn summary(&self) -> Value {
        json!({
            "console_entries": self.console.len(),
            "network_requests": self.network.len(),
            "frames_recorded": self.profiler.frames(),
        })
    }

    /// Handles one FDP message. Unknown methods are errors, not silence —
    /// a devtools client deserves a straight answer.
    pub fn handle_fdp(&mut self, message: &str) -> Result<Value, fdp::FdpError> {
        self.handle_fdp_value(serde_json::from_str(message)?)
    }

    pub fn handle_fdp_value(&mut self, message: Value) -> Result<Value, fdp::FdpError> {
        fdp::handle(self, message)
    }
}

impl Default for Devtools {
    fn default() -> Self {
        Devtools::new()
    }
}
