//! Picture-in-picture policy: the *when* and *how big* of a PiP window.
//! The surface itself belongs to the platform crate — commands and states
//! are plain data so a bridge can replay them without a compositor in the
//! room. Frames come from openfox-render; this module never touches one.

use std::collections::HashMap;

/// Size requested when a host has no remembered geometry: 16:9, like
/// almost everything.
pub const DEFAULT_PIP_SIZE: (u32, u32) = (480, 270);

/// Lifecycle of a PiP window, media-time inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipState {
    /// No window, nothing to show.
    Closed,
    /// A surface has been requested; the platform has not confirmed yet.
    Opening,
    /// Playing at this media position, in milliseconds.
    Playing { media_ms: u64 },
    /// Paused at this media position, in milliseconds.
    Paused { media_ms: u64 },
}

impl PipState {
    /// The media position this state carries, if any. `Closed` and
    /// `Opening` carry none — one has no window, the other no frames yet.
    pub fn media_ms(&self) -> Option<u64> {
        match self {
            PipState::Playing { media_ms } | PipState::Paused { media_ms } => Some(*media_ms),
            PipState::Closed | PipState::Opening => None,
        }
    }
}

/// Commands the platform bridge sends. Each maps to one state transition;
/// none of them opens a window by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipCommand {
    Play,
    Pause,
    SeekToMs(u64),
    Mute,
    Unmute,
    Close,
}

/// When playback moves into PiP without being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoEnterPolicy {
    /// Never auto-enter: the user presses the button or nothing happens.
    #[default]
    Off,
    /// Enter PiP as the tab is hidden. Mirrors Android's PiP intent
    /// contract — `onUserLeaveHint` fires on swipe-away and video moves to
    /// the small window instead of dying with the activity.
    OnTabHide,
    /// Enter PiP when the user leaves fullscreen: the shrinking-window
    /// hand-off desktop users expect from the big video sites.
    OnFullscreenExit,
}

/// Surface geometry this crate asks the platform for. The platform crate
/// fulfills the spec; this crate never opens a window itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SurfaceSpec {
    pub w: u32,
    pub h: u32,
    pub always_on_top: bool,
}

/// PiP policy machine: state, auto-enter rules, and per-host size memory so
/// a window the user carefully resized doesn't snap back on the next video.
pub struct PipWindow {
    state: PipState,
    auto_enter: AutoEnterPolicy,
    size_memory: HashMap<String, (u32, u32)>,
    always_on_top: bool,
    muted: bool,
}

impl PipWindow {
    /// A policy machine with the given auto-enter rule. PiP windows are
    /// always-on-top by default; floating junk video is the point.
    pub fn new(auto_enter: AutoEnterPolicy) -> Self {
        PipWindow {
            state: PipState::Closed,
            auto_enter,
            size_memory: HashMap::new(),
            always_on_top: true,
            muted: false,
        }
    }

    /// Current state.
    pub fn state(&self) -> PipState {
        self.state
    }

    /// Whether PiP audio is muted.
    pub fn is_muted(&self) -> bool {
        self.muted
    }

    /// Begin opening a window: the only path into [`PipState::Opening`]
    /// and the only state from which [`PipWindow::surface_request`] speaks.
    pub fn open(&mut self) -> PipState {
        self.state = PipState::Opening;
        self.state
    }

    /// Apply a command and return the resulting state. Senseless commands
    /// are ignored, not errored: a stray `Pause` on a closed window is noise.
    pub fn apply(&mut self, command: PipCommand) -> PipState {
        match command {
            PipCommand::Play => {
                if !matches!(self.state, PipState::Closed) {
                    let media_ms = self.state.media_ms().unwrap_or(0);
                    self.state = PipState::Playing { media_ms };
                }
            }
            PipCommand::Pause => {
                if let PipState::Playing { media_ms } = self.state {
                    self.state = PipState::Paused { media_ms };
                }
            }
            PipCommand::SeekToMs(ms) => match self.state {
                PipState::Playing { .. } => self.state = PipState::Playing { media_ms: ms },
                PipState::Paused { .. } => self.state = PipState::Paused { media_ms: ms },
                PipState::Closed | PipState::Opening => {}
            },
            PipCommand::Mute => self.muted = true,
            PipCommand::Unmute => self.muted = false,
            PipCommand::Close => self.state = PipState::Closed,
        }
        self.state
    }

    /// Remember the size the user chose for a host, so the next PiP window
    /// from the same site opens where they left it.
    pub fn remember_size(&mut self, host: &str, w: u32, h: u32) {
        if w > 0 && h > 0 {
            self.size_memory.insert(host.to_owned(), (w, h));
        }
    }

    /// Whether the policy says playback should enter PiP right now.
    pub fn should_auto_enter(&self, tab_hidden: bool, fullscreen_exited: bool) -> bool {
        match self.auto_enter {
            AutoEnterPolicy::Off => false,
            AutoEnterPolicy::OnTabHide => tab_hidden,
            AutoEnterPolicy::OnFullscreenExit => fullscreen_exited,
        }
    }

    /// The surface request while the window is `Opening`: remembered size
    /// for this host if any, else the 16:9 default. `None` once the surface
    /// exists or the window is gone.
    pub fn surface_request(&self, host: &str) -> Option<SurfaceSpec> {
        if self.state != PipState::Opening {
            return None;
        }
        let (w, h) = self
            .size_memory
            .get(host)
            .copied()
            .unwrap_or(DEFAULT_PIP_SIZE);
        Some(SurfaceSpec { w, h, always_on_top: self.always_on_top })
    }
}
