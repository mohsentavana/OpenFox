//! Conversation model: an ordered message list with a hard context
//! budget. The budget is expressed in tokens but enforced in characters
//! — four characters per token, the industry's usual back-of-napkin —
//! because the alternative is shipping a tokenizer per provider, and a
//! conversation that is 5% over-budget is not a conversation worth that
//! dependency.
//!
//! Trimming policy: system messages never drop (they carry the safety
//! rule and the persona), the newest turns always survive, and the
//! oldest non-system turns are evicted first. A conversation should feel
//! like it forgets the beginning, never the instructions or the last
//! thing the user said.

use serde::{Deserialize, Serialize};

/// Characters per estimated token. Deliberately crude, deliberately
/// public: providers that report real usage can correct the estimate,
/// but the trim decision happens before they ever see the text.
pub const CHARS_PER_TOKEN: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Message { role: Role::System, content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Message { role: Role::User, content: content.into() }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Message { role: Role::Assistant, content: content.into() }
    }

    /// Estimated tokens for this message.
    pub fn est_tokens(&self) -> usize {
        estimate_tokens(&self.content)
    }
}

/// Rough token count: length in characters divided by four, rounded up.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(CHARS_PER_TOKEN)
}

/// A bounded conversation.
#[derive(Debug, Clone)]
pub struct Session {
    messages: Vec<Message>,
    /// Hard ceiling in estimated tokens across all kept messages.
    budget_tokens: usize,
}

impl Session {
    /// A session that trims itself to `budget_tokens` (estimated).
    pub fn new(budget_tokens: usize) -> Self {
        Session { messages: Vec::new(), budget_tokens }
    }

    pub fn budget_tokens(&self) -> usize {
        self.budget_tokens
    }

    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Current estimated token usage.
    pub fn used_tokens(&self) -> usize {
        self.messages.iter().map(Message::est_tokens).sum()
    }

    /// Append a message and trim. The message just pushed is always kept
    /// — even a huge one — because dropping the turn a user just typed
    /// looks exactly like the assistant ignoring them.
    pub fn push(&mut self, message: Message) {
        self.messages.push(message);
        self.trim();
    }

    /// Append without allocating an explicit `Message`.
    pub fn push_role(&mut self, role: Role, content: impl Into<String>) {
        self.push(Message { role, content: content.into() });
    }

    /// Evict oldest non-system messages until the session fits its
    /// budget. System messages are exempt; if they alone exceed the
    /// budget they stay anyway — a safety rule that can be trimmed away
    /// by volume is not a safety rule.
    fn trim(&mut self) {
        let budget = self.budget_tokens;
        while self.used_tokens() > budget {
            let Some(pos) = self.messages.iter().position(|m| m.role != Role::System) else {
                return;
            };
            self.messages.remove(pos);
        }
    }

    /// The most recent user message, if the user has spoken at all.
    pub fn last_user(&self) -> Option<&Message> {
        self.messages.iter().rev().find(|m| m.role == Role::User)
    }
}
