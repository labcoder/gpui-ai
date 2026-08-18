//! An ambient, always-moving indicator for "the model is alive" moments.

use gpui::{
    Animation, AnimationExt as _, App, IntoElement, ParentElement as _, Pixels, RenderOnce,
    StyleRefinement, Styled, Window, div, px,
};
use gpui_component::{ActiveTheme as _, StyledExt as _};
use std::f32::consts::TAU;
use std::time::Duration;

/// Per-orb motion: (cycle seconds, phase offset, orbit radius factor,
/// diameter factor). Cycle lengths are mutually irrational-ish so the
/// composition never visibly repeats.
const ORBS: [(f32, f32, f32, f32); 3] = [
    (3.6, 0.00, 0.18, 0.62),
    (4.7, 0.33, 0.22, 0.50),
    (5.9, 0.66, 0.16, 0.45),
];

/// Base opacity of each orb; overlaps blend additively.
const ORB_OPACITY: f32 = 0.45;

/// A cluster of softly drifting orbs — an ambient thinking indicator for
/// moments with no progress to report, where a spinner would feel too
/// mechanical (voice idle states, model warm-up, long silent reasoning).
///
/// The orbs take their colors from the active theme (`primary`, `info`,
/// `cyan`), so the indicator follows light/dark modes and custom themes
/// like every other component.
///
/// # Example
///
/// ```ignore
/// Orbs::new()                      // 40px cluster
/// Orbs::new().diameter(px(64.))    // larger
/// ```
#[derive(IntoElement)]
pub struct Orbs {
    style: StyleRefinement,
    diameter: Pixels,
}

impl Orbs {
    /// Creates a 40px orb cluster.
    pub fn new() -> Self {
        Self {
            style: StyleRefinement::default(),
            diameter: px(40.),
        }
    }

    /// Sets the overall cluster diameter. Orb sizes and orbits scale with it.
    pub fn diameter(mut self, diameter: impl Into<Pixels>) -> Self {
        self.diameter = diameter.into();
        self
    }
}

impl Default for Orbs {
    fn default() -> Self {
        Self::new()
    }
}

impl Styled for Orbs {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Orbs {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let tokens = cx.theme().semantic_tokens();
        let d = self.diameter;
        let colors = [cx.theme().primary, cx.theme().info, cx.theme().cyan];

        div()
            .relative()
            .size(d)
            .children(
                ORBS.iter()
                    .enumerate()
                    .map(|(ix, &(secs, phase, orbit, frac))| {
                        let orb = d * frac;
                        let radius = d * orbit;
                        // Rest position: orb centered in the cluster.
                        let center = (d - orb) * 0.5;
                        let color = colors[ix % colors.len()].opacity(ORB_OPACITY);

                        div()
                            .absolute()
                            .size(orb)
                            .rounded(tokens.radius.full)
                            .bg(color)
                            .with_animation(
                                ("orb", ix as u64),
                                Animation::new(Duration::from_secs_f32(secs)).repeat(),
                                move |this, delta| {
                                    let angle = TAU * (delta + phase);
                                    // A slightly squashed orbit plus a breathing
                                    // opacity keeps the motion organic rather than
                                    // planetary.
                                    this.left(center + radius * angle.cos())
                                        .top(center + radius * angle.sin() * 0.8)
                                        .opacity(0.75 + 0.25 * angle.sin())
                                },
                            )
                    }),
            )
            .refine_style(&self.style)
    }
}
