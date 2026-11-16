//! The DOM event model: capture, target, bubble. Listeners are data on
//! the arena; dispatch walks the composed path and hands each matching
//! listener to the engine's executor (the JS realm or a WASM ext).

use crate::dom::NodeId;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventName {
    Click,
    Input,
    Change,
    Submit,
    KeyDown,
    KeyUp,
    Load,
    DOMContentLoaded,
    Scroll,
    Focus,
    Blur,
    MouseOver,
    MouseOut,
    /// Escape hatch for custom events — the engine passes them through.
    Custom(String),
}

impl EventName {
    pub fn parse(name: &str) -> EventName {
        match name {
            "click" => EventName::Click,
            "input" => EventName::Input,
            "change" => EventName::Change,
            "submit" => EventName::Submit,
            "keydown" => EventName::KeyDown,
            "keyup" => EventName::KeyUp,
            "load" => EventName::Load,
            "DOMContentLoaded" => EventName::DOMContentLoaded,
            "scroll" => EventName::Scroll,
            "focus" => EventName::Focus,
            "blur" => EventName::Blur,
            "mouseover" => EventName::MouseOver,
            "mouseout" => EventName::MouseOut,
            other => EventName::Custom(other.into()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Capturing,
    AtTarget,
    Bubbling,
}

/// A registered listener: where, which phase, and the two flags that
/// change dispatch semantics.
#[derive(Debug, Clone, Copy)]
pub struct Listener {
    pub node: NodeId,
    pub phase: Phase,
    /// Fire once, then the engine unregisters it.
    pub once: bool,
    /// The listener promises not to call preventDefault — scroll stays
    /// smooth because the engine can skip the round-trip.
    pub passive: bool,
}

#[derive(Debug, Clone, Default)]
pub struct DispatchState {
    pub propagation_stopped: bool,
    pub default_prevented: bool,
}

/// Dispatch one event along the composed path. `run` executes a
/// listener (JS realm call or WASM ext export) and returns whether it
/// stopped propagation or prevented default. The engine owns listener
/// registration and once-removal; this owns the walk order.
pub fn dispatch(
    path: &[NodeId],
    target: NodeId,
    listeners: &[Listener],
    run: &mut dyn FnMut(&Listener) -> ListenerOutcome,
) -> DispatchState {
    let mut state = DispatchState::default();

    // Capture: root toward target, skipping the target itself.
    for node in path.iter().take(path.len().saturating_sub(1)) {
        if walk_node(*node, Phase::Capturing, listeners, run, &mut state) {
            return state;
        }
    }

    // At target: both capture and bubble registrations run, capture first.
    if walk_node(target, Phase::Capturing, listeners, run, &mut state) {
        return state;
    }
    if walk_node(target, Phase::Bubbling, listeners, run, &mut state) {
        return state;
    }

    // Bubble: target toward root, skipping the target itself.
    for node in path.iter().rev().skip(1) {
        if walk_node(*node, Phase::Bubbling, listeners, run, &mut state) {
            return state;
        }
    }
    state
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ListenerOutcome {
    pub stopped: bool,
    pub prevented: bool,
}

/// Run matching listeners on one node. Returns true when propagation is
/// stopped and the walk must end.
fn walk_node(
    node: NodeId,
    phase: Phase,
    listeners: &[Listener],
    run: &mut dyn FnMut(&Listener) -> ListenerOutcome,
    state: &mut DispatchState,
) -> bool {
    for listener in listeners {
        if listener.node != node || listener.phase != phase {
            continue;
        }
        let outcome = run(listener);
        // A passive listener's preventDefault is ignored by contract —
        // enforced here, not trusted from the host.
        if outcome.prevented && !listener.passive {
            state.default_prevented = true;
        }
        if outcome.stopped {
            state.propagation_stopped = true;
            return true;
        }
    }
    false
}

/// Default actions (link navigation on anchor clicks, form submission)
/// are engine policy evaluated AFTER dispatch: preventDefault changes
/// the answer, nothing else does.
pub fn should_run_default_action(state: &DispatchState) -> bool {
    !state.default_prevented
}
