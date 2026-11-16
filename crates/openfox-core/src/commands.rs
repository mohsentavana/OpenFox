//! The command bus: every UI action is a command. Keybindings speak
//! commands, MCP tools speak commands, undo replays commands. One
//! vocabulary, three drivers.

use crate::tab::TabId;

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Navigate { tab: TabId, url: String },
    GoBack { tab: TabId },
    GoForward { tab: TabId },
    Reload { tab: TabId },
    Stop { tab: TabId },
    NewTab { url: Option<String> },
    CloseTab { tab: TabId },
    SelectTab { tab: TabId },
    ToggleShield { tab: TabId },
    SetZoom { tab: TabId, level: f32 },
    ResetZoom { tab: TabId },
    ToggleReader { tab: TabId },
    Find { tab: TabId, query: String },
    FindNext { tab: TabId },
    FindPrevious { tab: TabId },
    GrantPermission { origin: String, permission: String },
    RevokePermission { origin: String, permission: String },
    OpenDownloads,
    OpenLibrary,
    TogglePrivateWindow,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Ack {
    Done,
    NoSuchTab,
    Rejected { reason: String },
}

const UNDO_CAP: usize = 32;

pub struct CommandBus {
    undo: Vec<Command>,
    redo: Vec<Command>,
}

impl Default for CommandBus {
    fn default() -> Self {
        CommandBus {
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }
}

impl CommandBus {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a command as undoable. Only reversible commands land here:
    /// Navigate, SetZoom, ToggleShield, ToggleReader. Closing a tab is
    /// reversible only through session restore — a different mechanism
    /// with a different guarantee.
    pub fn note_executed(&mut self, command: &Command) {
        let reversible = matches!(
            command,
            Command::Navigate { .. }
                | Command::SetZoom { .. }
                | Command::ToggleShield { .. }
                | Command::ToggleReader { .. }
                | Command::GrantPermission { .. }
        );
        if !reversible {
            return;
        }
        self.undo.push(command.clone());
        if self.undo.len() > UNDO_CAP {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Pop the last undoable command, if any.
    pub fn undo(&mut self) -> Option<Command> {
        let command = self.undo.pop()?;
        self.redo.push(command.clone());
        Some(command)
    }

    /// Pop the last undone command, if any.
    pub fn redo(&mut self) -> Option<Command> {
        let command = self.redo.pop()?;
        self.undo.push(command.clone());
        Some(command)
    }

    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }
}
