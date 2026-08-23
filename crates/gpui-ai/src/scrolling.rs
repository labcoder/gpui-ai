//! Scroll behavior primitives: wheel acceleration and middle-click autoscroll.
//!
//! These are behaviors, not components: they own no rendering and no theme.
//! Applications (or composite components) drive them from ordinary GPUI input
//! events and apply the returned distances to whatever scroll position they
//! manage. This mirrors how Zed's own editor handles the wheel — discrete
//! line notches are scaled by a sensitivity multiplier while trackpad pixel
//! deltas pass through untouched — and adds Windows-convention cadence
//! acceleration on top for fast successive scrolling.
//!
//! # Wheel acceleration
//!
//! ```
//! use gpui_ai::scrolling::{WheelAccelerator, LINE_HEIGHT_PX};
//! use gpui::{px, Pixels};
//!
//! let mut wheel = WheelAccelerator::new();
//! // One notch of a conventional wheel; negative dy scrolls down.
//! let boost = wheel.line_notch(-1.0, false);
//! assert!(boost > px(0.) && boost <= px((3.0 - 1.0) * LINE_HEIGHT_PX));
//!
//! // Trackpad pixel deltas never enter the accelerator: the application
//! // forwards them straight to its scroll position, unaccelerated.
//! ```
//!
//! # Middle-click autoscroll
//!
//! ```
//! use gpui_ai::scrolling::Autoscroll;
//! use gpui::{point, px, Pixels};
//!
//! let mut session = Autoscroll::start(point(px(300.), px(200.)));
//! session.track(point(px(300.), px(270.)));
//! // Pointer 70 px below the anchor at ~16 ms/frame:
//! let distance = session.tick(0.016);
//! assert!(distance > Pixels::ZERO);
//! ```

use gpui::{
    HitboxBehavior, IntoElement as _, ListState, Pixels, ScrollWheelEvent, Styled as _, canvas, px,
};

/// Covers a retained list viewport with capture-phase wheel containment.
///
/// GPUI lists consume wheel input during bubbling. When a list is nested in
/// another list, the ancestor may therefore run before the descendant. This
/// mask moves the descendant directly during capture while it has room and
/// releases the event at either edge so ordinary scroll chaining resumes.
pub(crate) fn list_scroll_mask(state: &ListState) -> impl gpui::IntoElement {
    let state = state.clone();
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |_, hitbox, window, _| {
            let line_height = window.line_height();
            let view_id = window.current_view();
            let hitbox_id = hitbox.id;
            window.on_mouse_event(move |event: &ScrollWheelEvent, phase, window, cx| {
                if !(phase.capture() && hitbox_id.should_handle_scroll(window)) {
                    return;
                }
                let delta_y = event.delta.pixel_delta(line_height).y;
                if ScrollRoom::from_list_state(&state).can_absorb(delta_y) {
                    state.scroll_by(-delta_y);
                    cx.notify(view_id);
                    cx.stop_propagation();
                }
            });
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

/// Base multiplier applied to wheel line deltas, matching Zed's editor
/// `scroll_sensitivity` default of 1.0.
pub const WHEEL_SENSITIVITY: f32 = 1.0;

/// Multiplier applied to wheel line deltas while Alt is held, matching Zed's
/// editor `fast_scroll_sensitivity` convention.
pub const WHEEL_FAST_SENSITIVITY: f32 = 4.0;

/// Consecutive same-direction notches before acceleration reaches full
/// strength. Windows conventions ramp over a handful of notches.
pub const WHEEL_ACCEL_NOTCHES_TO_FULL: f32 = 5.0;

/// Peak wheel acceleration multiplier once cadence saturates.
pub const WHEEL_ACCEL_MAX_MULTIPLIER: f32 = 3.0;

/// Seconds without a notch after which cadence decays back to rest.
pub const WHEEL_ACCEL_DECAY_SECONDS: f32 = 0.35;

/// Pixels assumed per scroll line when converting line deltas to distances,
/// matching gpui's `List` element conversion.
pub const LINE_HEIGHT_PX: f32 = 20.0;

/// Pointer distance from the anchor at which autoscroll reaches full speed.
pub const AUTOSCROLL_FULL_SPEED_DISTANCE_PX: f32 = 140.0;

/// Maximum autoscroll speed in pixels per second at or beyond full distance.
pub const MAX_AUTOSCROLL_SPEED_PX_PER_SEC: f32 = 900.0;

/// Cadence-based wheel accelerator over discrete line notches.
///
/// State is tiny (`Copy`); keep one per scroll surface. The accelerator never
/// touches rendering or scroll state — callers apply [`WheelAccelerator::line_notch`]'s
/// returned distance themselves.
#[derive(Debug, Clone, Copy)]
pub struct WheelAccelerator {
    cadence: f32,
    decayed: bool,
}

impl Default for WheelAccelerator {
    fn default() -> Self {
        Self::new()
    }
}

impl WheelAccelerator {
    /// Creates an accelerator at rest.
    pub fn new() -> Self {
        Self {
            cadence: 0.0,
            decayed: true,
        }
    }

    /// Processes one discrete wheel notch and returns the extra scroll
    /// distance to apply beyond the base scroll the framework already made.
    ///
    /// Sign follows wheel convention: negative means scroll down. The boost
    /// ramps linearly over [`WHEEL_ACCEL_NOTCHES_TO_FULL`] same-direction
    /// notches toward [`WHEEL_ACCEL_MAX_MULTIPLIER`], so the first notch of a
    /// burst contributes a small ramp-in boost and later ones contribute up
    /// to the full multiplier minus one. A pause longer than
    /// [`WHEEL_ACCEL_DECAY_SECONDS`] (see [`WheelAccelerator::rest`]) or a
    /// direction reversal restarts the ramp. `fast` applies the
    /// [`WHEEL_FAST_SENSITIVITY`] Alt-multiplier instead of
    /// [`WHEEL_SENSITIVITY`].
    pub fn line_notch(&mut self, dy: f32, fast: bool) -> Pixels {
        let now_decaying = self.decayed;
        self.decayed = false;
        if now_decaying {
            self.cadence = 0.0;
        }
        if dy * self.cadence < 0.0 {
            self.cadence = 0.0;
        }
        self.cadence += dy.signum();
        let strength = (self.cadence.abs() / WHEEL_ACCEL_NOTCHES_TO_FULL).min(1.0);
        let accel = 1.0 + (WHEEL_ACCEL_MAX_MULTIPLIER - 1.0) * strength;
        let sensitivity = if fast {
            WHEEL_FAST_SENSITIVITY
        } else {
            WHEEL_SENSITIVITY
        };
        let boost = -dy * sensitivity * (accel - 1.0);
        gpui::px(boost * LINE_HEIGHT_PX)
    }

    /// Marks the accelerator idle after a pause between gestures, resetting
    /// its cadence. Call this when input handling notices elapsed time rather
    /// than relying solely on per-notch decay checks.
    pub fn rest(&mut self) {
        self.cadence = 0.0;
        self.decayed = true;
    }

    /// Current signed notch cadence (positive scrolling up).
    pub fn cadence(&self) -> f32 {
        self.cadence
    }
}

/// Middle-click autoscroll session anchored to a window position.
///
/// Speed scales linearly with pointer distance from the anchor up to full
/// speed at [`AUTOSCROLL_FULL_SPEED_DISTANCE_PX`], capped at
/// [`MAX_AUTOSCROLL_SPEED_PX_PER_SEC`], matching Windows conventions.
#[derive(Debug, Clone, Copy)]
pub struct Autoscroll {
    anchor_y: Pixels,
    pointer_y: Pixels,
}

impl Autoscroll {
    /// Begins a session anchored at `anchor`'s vertical position.
    pub fn start(anchor: gpui::Point<Pixels>) -> Self {
        Self {
            anchor_y: anchor.y,
            pointer_y: anchor.y,
        }
    }

    /// Updates the tracked pointer position; call from hover/move handlers.
    pub fn track(&mut self, pointer: gpui::Point<Pixels>) {
        self.pointer_y = pointer.y;
    }

    /// Advances the session by `delta_seconds` using the latest tracked
    /// pointer and returns the distance to scroll this frame. Positive values
    /// scroll down; zero means the pointer sits within one pixel of the
    /// anchor.
    pub fn tick(&mut self, delta_seconds: f32) -> Pixels {
        let offset = self.pointer_y - self.anchor_y;
        let distance = offset.abs();
        if distance < gpui::px(1.) {
            return gpui::px(0.);
        }
        let direction = offset.signum();
        let speed = ((distance / gpui::px(AUTOSCROLL_FULL_SPEED_DISTANCE_PX)).min(1.0)
            * MAX_AUTOSCROLL_SPEED_PX_PER_SEC) as f32;
        gpui::px(direction * speed * delta_seconds)
    }
}

/// Whether a vertically scrollable region can still absorb wheel motion in
/// the direction the user is scrolling.
///
/// This is the containment test behind native scroll chaining: an inner
/// component traps the wheel while it has room, and releases it to the
/// surrounding page once it bottoms out or tops up. `offset` and `max_offset`
/// use the GPUI convention where the offset is zero at the top of the
/// content and `-max_offset` at the bottom.
#[derive(Debug, Clone, Copy)]
pub struct ScrollRoom {
    /// Current vertical scroll offset (zero or negative).
    pub offset: Pixels,
    /// Maximum magnitude of the offset (zero means nothing to scroll).
    pub max_offset: Pixels,
}

impl ScrollRoom {
    /// Builds a room snapshot from a GPUI scroll handle's reported state.
    pub fn new(offset: Pixels, max_offset: Pixels) -> Self {
        Self {
            offset,
            max_offset: max_offset.max(px(0.)),
        }
    }

    /// True when scrolling by `delta_y` (negative = down) would move this
    /// region: it has scrollable content and is not already pinned against
    /// the edge in that direction.
    pub fn can_absorb(&self, delta_y: Pixels) -> bool {
        let zero = Pixels::ZERO;
        if self.max_offset <= zero {
            return false;
        }
        if delta_y < zero {
            // Scrolling toward the bottom: room remains unless pinned there.
            self.offset > -self.max_offset + px(0.5)
        } else if delta_y > zero {
            // Scrolling back toward the top.
            self.offset < -px(0.5)
        } else {
            false
        }
    }

    /// Snapshots the room from a GPUI [`ScrollHandle`](gpui::ScrollHandle).
    ///
    /// This is the form used by components whose scrolling lives in a
    /// `div().overflow_y_scroll().track_scroll(&handle)` container.
    pub fn from_handle(handle: &gpui::ScrollHandle) -> Self {
        Self::new(handle.offset().y, handle.max_offset().y)
    }

    /// Snapshots the room from a GPUI [`ListState`](gpui::ListState).
    pub fn from_list_state(state: &gpui::ListState) -> Self {
        Self::new(
            state.scroll_px_offset_for_scrollbar().y,
            state.max_offset_for_scrollbar().y,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px_f(value: Pixels) -> f32 {
        value.as_f32()
    }

    #[test]
    fn first_notch_applies_ramp_in_boost() {
        let mut wheel = WheelAccelerator::new();
        // One notch in: strength = 1/NOTCHES_TO_FULL, so the boost is small
        // but nonzero.
        let expected =
            ((WHEEL_ACCEL_MAX_MULTIPLIER - 1.0) / WHEEL_ACCEL_NOTCHES_TO_FULL) * LINE_HEIGHT_PX;
        assert!((px_f(wheel.line_notch(-1.0, false)) - expected).abs() < 0.01);
    }

    #[test]
    fn cadence_ramps_toward_max_multiplier() {
        let mut wheel = WheelAccelerator::new();
        wheel.line_notch(-1.0, false);
        let mut boosts = Vec::new();
        for _ in 0..6 {
            boosts.push(px_f(wheel.line_notch(-1.0, false)));
        }
        // Monotonically non-decreasing, first boost small, last at max ramp.
        assert!(boosts.windows(2).all(|pair| pair[0] <= pair[1]));
        assert!(boosts[0] > 0.0);
        let expected_full = (WHEEL_ACCEL_MAX_MULTIPLIER - 1.0) * LINE_HEIGHT_PX;
        assert!((boosts[5] - expected_full).abs() < 0.01);
    }

    #[test]
    fn direction_reversal_resets_cadence() {
        let mut wheel = WheelAccelerator::new();
        for _ in 0..8 {
            wheel.line_notch(-1.0, false);
        }
        assert!(wheel.cadence() < 0.0);
        let up_boost = px_f(wheel.line_notch(1.0, false));
        let fresh = px_f(WheelAccelerator::new().line_notch(1.0, false));
        assert!((up_boost - fresh).abs() < 0.01);
    }

    #[test]
    fn rest_clears_cadence() {
        let mut wheel = WheelAccelerator::new();
        for _ in 0..8 {
            wheel.line_notch(-1.0, false);
        }
        assert!(wheel.cadence() < 0.0);
        wheel.rest();
        assert_eq!(wheel.cadence(), 0.0);
        // After rest, the next notch is back to first-notch ramp-in strength.
        let expected =
            ((WHEEL_ACCEL_MAX_MULTIPLIER - 1.0) / WHEEL_ACCEL_NOTCHES_TO_FULL) * LINE_HEIGHT_PX;
        assert!((px_f(wheel.line_notch(-1.0, false)) - expected).abs() < 0.01);
    }

    #[test]
    fn fast_multiplies_the_ramp() {
        let mut base = WheelAccelerator::new();
        let mut fast = WheelAccelerator::new();
        base.line_notch(-1.0, false);
        fast.line_notch(-1.0, false);
        let base_boost = px_f(base.line_notch(-1.0, false));
        let fast_boost = px_f(fast.line_notch(-1.0, true));
        assert!(
            (fast_boost / base_boost - WHEEL_FAST_SENSITIVITY / WHEEL_SENSITIVITY).abs() < 0.01
        );
    }

    #[test]
    fn autoscroll_speed_scales_with_distance_then_caps() {
        let anchor = gpui::point(gpui::px(100.), gpui::px(200.));
        let mut session = Autoscroll::start(anchor);

        session.track(gpui::point(gpui::px(100.), gpui::px(240.)));
        let near = px_f(session.tick(0.016));
        session.track(gpui::point(gpui::px(100.), gpui::px(480.)));
        let far = px_f(session.tick(0.016));
        session.track(gpui::point(gpui::px(100.), gpui::px(900.)));
        let farther = px_f(session.tick(0.016));

        assert!(near > 0.0 && far > near && farther >= far);
        // At 280 px past the anchor the speed is capped: identical ticks match.
        let capped_a = px_f(session.tick(0.016));
        session.track(gpui::point(gpui::px(100.), gpui::px(1200.)));
        let capped_b = px_f(session.tick(0.016));
        assert!((capped_a - capped_b).abs() < 0.001);
    }

    #[test]
    fn autoscroll_near_anchor_is_zero_and_sign_follows_pointer() {
        let anchor = gpui::point(gpui::px(50.), gpui::px(500.));
        let mut session = Autoscroll::start(anchor);
        assert_eq!(session.tick(0.016), gpui::px(0.));
        session.track(gpui::point(gpui::px(50.), gpui::px(600.)));
        assert!(px_f(session.tick(0.016)) > 0.0);
        session.track(gpui::point(gpui::px(50.), gpui::px(400.)));
        assert!(px_f(session.tick(0.016)) < 0.0);
    }

    #[test]
    fn scroll_room_absorbs_only_when_room_remains_in_direction() {
        // Mid-scroll: absorbs both directions.
        let mid = ScrollRoom::new(px(-100.), px(300.));
        assert!(mid.can_absorb(px(-40.)));
        assert!(mid.can_absorb(px(40.)));

        // Pinned at the top: only downward motion is absorbed.
        let at_top = ScrollRoom::new(px(0.), px(300.));
        assert!(!at_top.can_absorb(px(40.)), "at top, upward must chain out");
        assert!(at_top.can_absorb(px(-40.)));

        // Pinned at the bottom: only upward motion is absorbed.
        let at_bottom = ScrollRoom::new(px(-300.), px(300.));
        assert!(
            !at_bottom.can_absorb(px(-40.)),
            "at bottom, downward must chain out"
        );
        assert!(at_bottom.can_absorb(px(40.)));

        // Nothing to scroll at all: never absorbs.
        let empty = ScrollRoom::new(px(0.), px(0.));
        assert!(!empty.can_absorb(px(-40.)));
        assert!(!empty.can_absorb(px(40.)));
    }
}
