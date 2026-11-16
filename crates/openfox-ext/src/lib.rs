pub mod api;
pub mod manifest;
pub mod runtime;
pub mod webext;

pub use manifest::{Manifest, Permission};
pub use runtime::{Ext, ExtContext, ExtRuntime, RunResult};
pub use webext::{map_api, manifest_verdict, CompatVerdict};

#[derive(Debug, thiserror::Error)]
pub enum ExtError {
    #[error("wasm: {0}")]
    Wasm(#[from] wasmtime::Error),
    #[error("wasm memory access: {0}")]
    Memory(#[from] wasmtime::MemoryAccessError),
    #[error("extension missing export: {0}")]
    MissingExport(String),
    #[error("manifest: {0}")]
    Manifest(String),
}
