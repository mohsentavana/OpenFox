//! The animation driver: property animations on display items, sampled on
//! the compositor thread. Layout is never re-entered — an animation that
//! needed layout would jank the page it decorates.

/// Easing curves with real math. Spring takes physical parameters and
/// may overshoot; the polynomial curves are clamped monotonic.
#[derive(Debug, Clone, Copy)]
pub enum Easing {
    Linear,
    QuadIn,
    QuadOut,
    CubicIn,
    CubicOut,
    Spring { stiffness: f32, damping: f32 },
}

impl Easing {
    /// Samples the curve. `t` is normalized progress for the polynomial
    /// curves; for Spring it is elapsed seconds, so a spring reaches
    /// settling within whatever duration the animation carries.
    pub fn sample_at(&self, t: f32) -> f32 {
        match *self {
            Easing::Linear => t,
            Easing::QuadIn => t * t,
            Easing::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::CubicIn => t * t * t,
            Easing::CubicOut => {
                let inv = 1.0 - t;
                1.0 - inv * inv * inv
            }
            Easing::Spring { stiffness, damping } => {
                // Damped harmonic oscillator, unit mass, from rest to 1.
                let omega = stiffness.max(0.0001).sqrt();
                // Zeta is clamped just under critical: chrome springs may
                // settle, never crawl — an overdamped panel feels broken.
                let zeta = (damping / (2.0 * omega)).min(0.99);
                let damped = omega * (1.0 - zeta * zeta).sqrt();
                1.0 - (-zeta * omega * t).exp() * ((damped * t).cos() + (zeta * omega / damped) * (damped * t).sin())
            }
        }
    }
}

/// Which property of a display item the animation drives. The compositor
/// applies these without rebuilding the display list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimatedProperty {
    Opacity,
    TranslateX,
    TranslateY,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    pub target: u64,
    pub property: AnimatedProperty,
    pub value: f32,
}

/// One animation, from `from` to `to` on one property of one item.
#[derive(Debug, Clone)]
pub struct Animation {
    pub target: u64,
    pub property: AnimatedProperty,
    pub from: f32,
    pub to: f32,
    pub duration_ms: u32,
    pub easing: Easing,
    /// Compositor clock, so vsync alignment is a property of the clock.
    pub started_ms: u64,
}

/// No chrome animation runs longer than ten seconds. A browser control
/// that takes ten seconds to settle is an ad, not an animation.
pub const MAX_DURATION_MS: u32 = 10_000;

impl Animation {
    /// Samples at a compositor timestamp. Finished animations return their
    /// endpoint so the last frame is exact.
    pub fn sample(&self, now_ms: u64) -> f32 {
        let elapsed = now_ms.saturating_sub(self.started_ms) as f32 / self.duration_ms.max(1) as f32;
        if elapsed >= 1.0 {
            return self.to;
        }
        let eased = self.easing.sample_at(elapsed);
        self.from + (self.to - self.from) * eased
    }
}

/// Drives every running animation. Ticks are vsync-aligned: the compositor
/// calls `tick` once per presented frame and applies the returned samples.
/// While occluded the clock stands still, so animations resume where they
/// stopped instead of teleporting to their endpoints.
pub struct AnimationDriver {
    active: Vec<Animation>,
    occluded: bool,
}

impl AnimationDriver {
    pub fn new() -> AnimationDriver {
        AnimationDriver { active: Vec::new(), occluded: false }
    }

    /// Enqueues an animation, replacing any animation already driving the
    /// same property of the same item — the old one cancels from its
    /// current value, which is why the caller samples before replacing.
    /// Durations clamp to the cap.
    pub fn push(&mut self, animation: Animation) {
        let mut animation = animation;
        animation.duration_ms = animation.duration_ms.min(MAX_DURATION_MS);
        self.active.retain(|running| !(running.target == animation.target && running.property == animation.property));
        self.active.push(animation);
    }

    /// Cancels every animation on one target. Returns how many stopped.
    pub fn cancel(&mut self, target: u64) -> usize {
        let before = self.active.len();
        self.active.retain(|animation| animation.target != target);
        before - self.active.len()
    }

    pub fn set_occluded(&mut self, occluded: bool) {
        self.occluded = occluded;
    }

    pub fn is_occluded(&self) -> bool {
        self.occluded
    }

    pub fn len(&self) -> usize {
        self.active.len()
    }

    pub fn is_empty(&self) -> bool {
        self.active.is_empty()
    }

    /// Advances to a timestamp and returns the samples to apply this
    /// frame. Finished animations are removed after being sampled once at
    /// their endpoint.
    pub fn tick(&mut self, now_ms: u64) -> Vec<Sample> {
        if self.occluded {
            return Vec::new();
        }
        let samples: Vec<Sample> = self
            .active
            .iter()
            .map(|animation| Sample {
                target: animation.target,
                property: animation.property,
                value: animation.sample(now_ms),
            })
            .collect();
        self.active.retain(|animation| now_ms.saturating_sub(animation.started_ms) < animation.duration_ms as u64);
        samples
    }
}

impl Default for AnimationDriver {
    fn default() -> Self {
        AnimationDriver::new()
    }
}
