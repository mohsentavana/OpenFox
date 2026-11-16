//! The assistant: a pluggable, local-first answer surface in the
//! sidebar's shape — conversation model, provider trait, an extractive
//! summarizer that never leaves the process, page-context extraction
//! from the engine's DOM, and the prompt-invasion hygiene that keeps
//! page text as data.
//!
//! The layering is the privacy story. `session` owns turns and the
//! context budget; `provider` owns "who answers" with remote kinds
//! modeled as request *shapes* the app layer must carry to a wire (this
//! crate opens no sockets and stores no keys); `summarize` is the
//! offline fallback that quotes pages instead of paraphrasing them;
//! `page_context` decides which DOM text is assistant-visible at all;
//! and `safety` wraps whatever survives in a labeled envelope so a page
//! can never speak with the operator's voice. A provider surface that
//! cannot phone home by construction is the only one a no-telemetry
//! browser can ship.

pub mod page_context;
pub mod provider;
pub mod safety;
pub mod session;
pub mod summarize;

pub use page_context::{page_context, wrapped_context, PageContext, MAX_CONTEXT_CHARS};
pub use provider::{
    ollama_request, openai_request, LocalAssistant, Provider, ProviderKind, RequestSpec,
};
pub use safety::{
    extract_page_context, wrap_page_context, CONTEXT_BEGIN, CONTEXT_END, CONTEXT_RULE,
};
pub use session::{estimate_tokens, Message, Role, Session, CHARS_PER_TOKEN};
pub use summarize::{split_sentences, summarize};

/// Everything the assistant can fail at. Kept deliberately short: the
/// remote-provider failure modes belong to the app layer's HTTP client
/// and arrive here only as [`AssistantError::Provider`] wrappers.
#[derive(Debug, thiserror::Error)]
pub enum AssistantError {
    /// The session held nothing answerable — no page context wrapped,
    /// and the user has not typed a turn yet.
    #[error("nothing to answer from: no page context and no user text")]
    NothingToSummarize,

    /// A provider ran but produced no usable content (empty summary,
    /// blank completion). Distinct from a transport failure: retrying
    /// this is usually pointless.
    #[error("the provider produced no usable response")]
    EmptyResponse,

    /// A remote provider failed in the app layer's transport. The
    /// string is the app layer's diagnosis; this crate never invents
    /// network errors it did not see.
    #[error("provider failure: {0}")]
    Provider(String),
}
