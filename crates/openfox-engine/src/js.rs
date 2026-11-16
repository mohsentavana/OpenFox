//! The JS boundary. The engine speaks to a script host through this
//! trait; the phase-2 Boa integration implements it. Nothing here
//! imports a JS engine — the boundary is the contract, and the contract
//! is what lets the rest of the engine be built and tested without one.

use crate::dom::NodeId;

/// The values that cross the boundary. Objects and arrays are handles,
/// not copies: a handle is a realm-local id the host resolves.
#[derive(Debug, Clone, PartialEq)]
pub enum JsValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Object(u64),
    Array(u64),
}

#[derive(Debug, thiserror::Error)]
pub enum JsError {
    #[error("syntax error: {0}")]
    Syntax(String),
    #[error("runtime error: {0}")]
    Runtime(String),
    #[error("realm disposed")]
    RealmDisposed,
}

/// The host contract. `eval` runs a script to completion; `call` invokes
/// a function by name on the realm's global; `gc_hint` is advisory.
pub trait ScriptHost {
    fn eval(&mut self, source: &str, url: &str) -> Result<JsValue, JsError>;
    fn call(&mut self, function: &str, args: &[JsValue]) -> Result<JsValue, JsError>;
    fn gc_hint(&mut self);
}

/// The microtask queue: drained after every task, before the next one.
/// setTimeout is NOT a microtask — it maps onto the event loop's timer
/// wheel; this queue is promise callbacks only.
#[derive(Debug, Default)]
pub struct MicrotaskQueue {
    queued: Vec<Microtask>,
}

#[derive(Debug, Clone)]
pub struct Microtask {
    pub callback: String,
    pub args: Vec<JsValue>,
}

impl MicrotaskQueue {
    pub fn enqueue(&mut self, task: Microtask) {
        self.queued.push(task);
    }

    /// Drain to completion: a microtask that enqueues another microtask
    /// runs in the same drain — the spec's "drain the microtask queue"
    /// is a loop, not a pass. The depth guard is ours: runaway promise
    /// chains must not hang the engine.
    pub fn drain<H: ScriptHost>(&mut self, host: &mut H) -> Result<(), JsError> {
        let mut depth = 0;
        while !self.queued.is_empty() {
            depth += 1;
            if depth > 10_000 {
                return Err(JsError::Runtime("microtask depth exceeded 10k".into()));
            }
            let batch: Vec<Microtask> = std::mem::take(&mut self.queued);
            for task in batch {
                host.call(&task.callback, &task.args)?;
            }
        }
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.queued.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queued.is_empty()
    }
}

/// One realm per document: same-origin iframes share a realm; cross-
/// origin frames never do. The realm owns its microtask queue and its
/// object table — a page navigation disposes the realm and everything in
/// it, which is the entire isolation story.
#[derive(Debug, Default)]
pub struct Realm {
    pub document: Option<NodeId>,
    pub url: String,
    pub microtasks: MicrotaskQueue,
    next_handle: u64,
}

impl Realm {
    pub fn new(url: &str) -> Self {
        Realm {
            document: None,
            url: url.to_owned(),
            microtasks: MicrotaskQueue::default(),
            next_handle: 1,
        }
    }

    /// Allocate an object handle. Handles are never zero and never
    /// reused within a realm — a stale handle is a dead handle, not a
    /// confused one.
    pub fn next_object_handle(&mut self) -> u64 {
        let handle = self.next_handle;
        self.next_handle += 1;
        handle
    }
}

/// The host functions OpenFox registers in every realm. fetch routes
/// through the network stack's privacy pipeline; setTimeout routes
/// through the event loop. Neither spins a thread — the chrome never
/// runs page script, so a page's worst day is a page problem.
pub const HOST_FUNCTIONS: &[(&str, &str)] = &[
    ("fetch", "openfox::net::fetch — same policy as the browser's own requests"),
    ("setTimeout", "openfox::loop::timer — millisecond wheel, not a thread"),
    ("queueMicrotask", "realm microtask queue"),
];
