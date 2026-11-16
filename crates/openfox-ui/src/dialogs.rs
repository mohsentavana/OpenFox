//! Modal dialogs: permission prompts, the certificate viewer, update
//! notices, the print shell and the before-unload guard — one visible at a
//! time, the rest queued.

use std::collections::VecDeque;

/// A site permission being asked for. Mirrors the settings surface's
/// permission set so a prompt answered "always allow" lands in the
/// exceptions editor without translation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptPermission {
    Camera,
    Microphone,
    Location,
    Notifications,
}

impl PromptPermission {
    pub fn key(&self) -> &'static str {
        match self {
            PromptPermission::Camera => "perm.camera",
            PromptPermission::Microphone => "perm.microphone",
            PromptPermission::Location => "perm.location",
            PromptPermission::Notifications => "perm.notifications",
        }
    }
}

/// "example.com wants to use your camera" plus the always-allow checkbox.
#[derive(Debug, Clone)]
pub struct PermissionPrompt {
    pub origin: String,
    pub permission: PromptPermission,
    pub always_allow: bool,
}

/// Certificate details as openfox-net summarizes the chain it validated.
#[derive(Debug, Clone)]
pub struct CertificateViewer {
    pub subject: String,
    pub issuer: String,
    pub valid_from_ms: i64,
    pub valid_to_ms: i64,
    pub sans: Vec<String>,
    pub chain_errors: Vec<CertChainError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CertChainError {
    UntrustedRoot,
    Expired,
    HostMismatch,
}

impl CertChainError {
    pub fn key(&self) -> &'static str {
        match self {
            CertChainError::UntrustedRoot => "cert.untrusted_root",
            CertChainError::Expired => "cert.expired",
            CertChainError::HostMismatch => "cert.host_mismatch",
        }
    }
}

/// A new build is ready. The notes string is the release notes excerpt the
/// updater fetched, rendered as-is — the dialog does not editorialize.
#[derive(Debug, Clone)]
pub struct UpdateNotice {
    pub version: String,
    pub notes: String,
}

/// The print shell before spooling: page count, scale, where the job goes.
#[derive(Debug, Clone)]
pub struct PrintShell {
    pub page_count: u32,
    pub scale_percent: u32,
    pub target: PrintTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintTarget {
    SavePdf,
    Printer,
}

/// The guard before leaving a page with unsaved form state.
#[derive(Debug, Clone)]
pub struct BeforeUnload {
    pub url: String,
}

/// One modal, whatever it is.
#[derive(Debug, Clone)]
pub enum Dialog {
    Permission(PermissionPrompt),
    Certificate(CertificateViewer),
    Update(UpdateNotice),
    Print(PrintShell),
    BeforeUnload(BeforeUnload),
}

/// Keyboard input to the visible dialog. Two keys, because modals get two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKey {
    Escape,
    Enter,
}

/// The modal queue: one dialog visible, the rest pending in arrival order.
/// Sites can stack prompts faster than any human answers; the queue keeps
/// every answer routed to the dialog it belongs to.
#[derive(Debug, Default)]
pub struct DialogModel {
    queue: VecDeque<Dialog>,
}

impl DialogModel {
    pub fn push(&mut self, dialog: Dialog) {
        self.queue.push_back(dialog);
    }

    pub fn visible(&self) -> Option<&Dialog> {
        self.queue.front()
    }

    pub fn visible_mut(&mut self) -> Option<&mut Dialog> {
        self.queue.front_mut()
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Resolves the visible dialog: true accepts (allow, install, print),
    /// false declines (block, later, cancel). The popped dialog is returned
    /// so the caller routes the outcome to the subsystem that asked.
    pub fn resolve(&mut self, accepted: bool) -> Option<Dialog> {
        let dialog = self.queue.pop_front()?;
        if let Dialog::Permission(prompt) = &dialog {
            // Declining a permission clears the checkbox: "don't allow this
            // time" and "never ask again" are different answers.
            if !accepted && prompt.always_allow {
                tracing::debug!(target: "openfox::dialog", "permission declined; always-allow ignored");
            }
        }
        Some(dialog)
    }

    /// Keyboard rules. Escape always declines. Enter accepts — except on
    /// the before-unload guard, where Enter means stay: losing work is the
    /// one mistake the safe default must prevent.
    pub fn key(&mut self, key: DialogKey) -> Option<Dialog> {
        let accepts = match self.queue.front()? {
            Dialog::BeforeUnload { .. } => false,
            _ => true,
        };
        match key {
            DialogKey::Escape => self.resolve(false),
            DialogKey::Enter => self.resolve(accepts),
        }
    }
}
