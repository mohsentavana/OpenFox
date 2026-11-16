//! Providers: the pluggable back end of the assistant. The trait is one
//! synchronous call — `complete(&Session) -> String` — because the
//! interesting decision (where the text comes from) should be
//! swappable, while the transport, the API keys and the retry policy
//! stay in the app layer where they belong.
//!
//! Two remote kinds are modeled as *request shapes, not clients*:
//! `ollama_request` and `openai_request` build the exact JSON body and
//! URL an Ollama `/api/generate` or an OpenAI-compatible
//! `/chat/completions` endpoint expects, and hand them to the caller as
//! data. No HTTP client, no async runtime, no secrets in this crate —
//! a provider surface that cannot exfiltrate is a provider surface
//! that does not need auditing for exfiltration.
//!
//! The `Local` kind needs none of that: it summarizes the page with the
//! extractive fallback in `summarize` and works offline.

use crate::safety::{extract_page_context, CONTEXT_BEGIN};
use crate::session::{Message, Role, Session};
use crate::summarize::summarize;
use crate::AssistantError;
use serde_json::{json, Value};

/// Which provider a configuration names. Carried separately from the
/// trait object so settings UIs and MCP surfaces can describe the
/// assistant without instantiating it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderKind {
    /// The built-in extractive summarizer. No network, no model, no
    /// telemetry — the floor the other kinds have to beat.
    Local,
    /// A local or remote Ollama daemon, `{endpoint}/api/generate`.
    Ollama { endpoint: String },
    /// Any OpenAI-compatible `/chat/completions` endpoint: OpenAI
    /// itself, local gateways, self-hosted mirrors.
    OpenAiCompatible { endpoint: String, model: String },
}

impl ProviderKind {
    pub fn label(&self) -> &'static str {
        match self {
            ProviderKind::Local => "local",
            ProviderKind::Ollama { .. } => "ollama",
            ProviderKind::OpenAiCompatible { .. } => "openai-compatible",
        }
    }
}

/// One assistant turn, end to end.
pub trait Provider {
    fn kind(&self) -> ProviderKind;

    /// Answer the session. Pure with respect to the process: no I/O,
    /// no clock, no global state — providers that need the network are
    /// driven by the app layer, which builds requests with the
    /// functions below and feeds the answer back as an assistant
    /// message.
    fn complete(&self, session: &Session) -> Result<String, AssistantError>;
}

/// The built-in provider: summarize the page (or, absent a page, the
/// user's own text) extractively. Deliberately the least clever thing
/// in this crate — it cannot hallucinate, cannot be prompt-injected
/// into doing anything, and cannot cost money.
#[derive(Debug, Clone)]
pub struct LocalAssistant {
    /// How many sentences a summary keeps.
    pub sentences: usize,
}

impl Default for LocalAssistant {
    fn default() -> Self {
        LocalAssistant { sentences: 3 }
    }
}

impl Provider for LocalAssistant {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Local
    }

    fn complete(&self, session: &Session) -> Result<String, AssistantError> {
        // The page context, if one was wrapped into the session, is the
        // object of the question; without one the last user turn is.
        let page_text = session.messages().iter().find_map(|message| {
            (message.role == Role::System && message.content.contains(CONTEXT_BEGIN))
                .then(|| extract_page_context(&message.content).map(str::to_owned))
                .flatten()
        });
        let source = page_text
            .or_else(|| session.last_user().map(|m| m.content.clone()))
            .ok_or(AssistantError::NothingToSummarize)?;
        let summary = summarize(&source, self.sentences);
        if summary.is_empty() {
            return Err(AssistantError::EmptyResponse);
        }
        Ok(summary)
    }
}

/// An outbound request as pure data: URL plus JSON body. The app layer
/// owns the HTTP client that turns this into bytes on a wire.
#[derive(Debug, Clone)]
pub struct RequestSpec {
    pub url: String,
    pub body: Value,
}

/// The Ollama `/api/generate` body. Ollama's generate API is
/// prompt-shaped rather than message-shaped, so the session is folded:
/// system messages become the `system` field, the remaining turns
/// become a transcript in the `prompt`, and `stream` is off because a
/// chat answer that dribbles in is the transport's problem to stream.
pub fn ollama_request(endpoint: &str, model: &str, session: &Session) -> RequestSpec {
    let system: Vec<&str> = session
        .messages()
        .iter()
        .filter(|m| m.role == Role::System)
        .map(|m| m.content.as_str())
        .collect();
    let transcript: Vec<String> = session
        .messages()
        .iter()
        .filter(|m| m.role != Role::System)
        .map(|m| match m.role {
            Role::User => format!("User: {}", m.content),
            _ => format!("Assistant: {}", m.content),
        })
        .collect();
    RequestSpec {
        url: format!("{}/api/generate", endpoint.trim_end_matches('/')),
        body: json!({
            "model": model,
            "system": system.join("\n"),
            "prompt": format!("{}\nUser:", transcript.join("\n")),
            "stream": false,
        }),
    }
}

/// The OpenAI-compatible `/chat/completions` body: the session maps
/// one-to-one onto `messages`, roles and all.
pub fn openai_request(endpoint: &str, model: &str, session: &Session) -> RequestSpec {
    let role_name = |role: Role| match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    };
    let messages: Vec<Value> = session
        .messages()
        .iter()
        .map(|Message { role, content }| {
            json!({ "role": role_name(*role), "content": content })
        })
        .collect();
    RequestSpec {
        url: format!("{}/chat/completions", endpoint.trim_end_matches('/')),
        body: json!({ "model": model, "messages": messages, "stream": false }),
    }
}
