//! Semantic size policy: control heights and leading glyph slots.
//!
//! Components draw their interactive controls at one of three heights and
//! seat leading glyphs in one of two slots, so a table's row action, a
//! sidebar's header control, and a composer's send button read as one
//! family instead of six coincidental heights. The values here are
//! defaults, not constants: an application replaces the policy wholesale
//! before its windows render, and components that legitimately vary
//! (density, embedded chrome) expose builder overrides that win over both.
//!
//! # Example
//!
//! ```no_run
//! # use std::time::Duration;
//! # fn example(cx: &mut gpui::App) {
//! gpui_ai::init(cx);
//! gpui_ai::sizing::SizeTokens::default()
//!     .with_control_lg(gpui::px(36.))
//!     .set(cx);
//! # }
//! ```

use gpui::{App, Global, Pixels, px};

/// The crate's size policy: three control heights and two glyph slots.
///
/// `control` heights size pressable controls — `sm` for compact chips and
/// icon buttons, `md` for standalone controls, `lg` for controls that sit
/// beside inputs and table rows and must match their height. `slot` sizes
/// the fixed leading box a glyph is centered in: `sm` matches the
/// extra-small text line, `md` the small text line, so a glyph beside
/// wrappable text stays centered on the first line no matter how far the
/// text wraps.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeTokens {
    control_sm: Pixels,
    control_md: Pixels,
    control_lg: Pixels,
    control_padding_sm: Pixels,
    control_padding_md: Pixels,
    control_padding_lg: Pixels,
    slot_sm: Pixels,
    slot_md: Pixels,
}

impl Global for SizeTokens {}

impl Default for SizeTokens {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl SizeTokens {
    /// The crate's default size policy.
    ///
    /// Control heights land on the 4px grid the spacing scale already
    /// walks, with `lg` equal to the upstream input and table-row height so
    /// adjacent controls rail. Slots equal the text line-heights they seat
    /// glyphs beside (16px for extra-small text, 20px for small).
    pub const DEFAULT: Self = Self {
        control_sm: px(24.),
        control_md: px(28.),
        control_lg: px(32.),
        // A control's label needs room to read as a label rather than as
        // text wedged into a pill. The upstream button's own eight pixels
        // are what the 0.4.0 feel review saw as tight; these are the ramp
        // shadcn and the reference sites settle on, scaled to our tiers.
        control_padding_sm: px(12.),
        control_padding_md: px(14.),
        control_padding_lg: px(18.),
        slot_sm: px(16.),
        slot_md: px(20.),
    };

    /// The installed policy, or the default when the application has not
    /// chosen one (or asks before [`crate::init`] ran).
    pub fn read(cx: &App) -> &Self {
        if cx.has_global::<Self>() {
            cx.global::<Self>()
        } else {
            &Self::DEFAULT
        }
    }

    /// Installs this policy for every component in the application.
    pub fn set(self, cx: &mut App) {
        cx.set_global(self);
    }

    /// Compact chips, icon buttons, and row actions.
    pub const fn control_sm(&self) -> Pixels {
        self.control_sm
    }

    /// Standalone controls: prompt and selection actions.
    pub const fn control_md(&self) -> Pixels {
        self.control_md
    }

    /// Controls that sit beside inputs and table rows.
    pub const fn control_lg(&self) -> Pixels {
        self.control_lg
    }

    /// Horizontal padding inside a compact control.
    pub const fn control_padding_sm(&self) -> Pixels {
        self.control_padding_sm
    }

    /// Horizontal padding inside a standalone control.
    pub const fn control_padding_md(&self) -> Pixels {
        self.control_padding_md
    }

    /// Horizontal padding inside a control that rails with an input.
    pub const fn control_padding_lg(&self) -> Pixels {
        self.control_padding_lg
    }

    /// Replaces [`control_padding_sm`](Self::control_padding_sm).
    pub const fn with_control_padding_sm(mut self, padding: Pixels) -> Self {
        self.control_padding_sm = padding;
        self
    }

    /// Replaces [`control_padding_md`](Self::control_padding_md).
    pub const fn with_control_padding_md(mut self, padding: Pixels) -> Self {
        self.control_padding_md = padding;
        self
    }

    /// Replaces [`control_padding_lg`](Self::control_padding_lg).
    pub const fn with_control_padding_lg(mut self, padding: Pixels) -> Self {
        self.control_padding_lg = padding;
        self
    }

    /// The leading glyph box beside extra-small text.
    pub const fn slot_sm(&self) -> Pixels {
        self.slot_sm
    }

    /// The leading glyph box beside small text.
    pub const fn slot_md(&self) -> Pixels {
        self.slot_md
    }

    /// Replaces the [`control_sm`](Self::control_sm) height.
    pub const fn with_control_sm(mut self, height: Pixels) -> Self {
        self.control_sm = height;
        self
    }

    /// Replaces the [`control_md`](Self::control_md) height.
    pub const fn with_control_md(mut self, height: Pixels) -> Self {
        self.control_md = height;
        self
    }

    /// Replaces the [`control_lg`](Self::control_lg) height.
    pub const fn with_control_lg(mut self, height: Pixels) -> Self {
        self.control_lg = height;
        self
    }

    /// Replaces the [`slot_sm`](Self::slot_sm) box.
    pub const fn with_slot_sm(mut self, size: Pixels) -> Self {
        self.slot_sm = size;
        self
    }

    /// Replaces the [`slot_md`](Self::slot_md) box.
    pub const fn with_slot_md(mut self, size: Pixels) -> Self {
        self.slot_md = size;
        self
    }
}

/// Installs the default policy unless the application already chose one,
/// so either order of `init` and customization lands on the application's
/// values.
pub(crate) fn install(cx: &mut App) {
    if !cx.has_global::<SizeTokens>() {
        SizeTokens::DEFAULT.set(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[gpui::test]
    fn install_respects_a_policy_the_application_chose_first(cx: &mut TestAppContext) {
        cx.update(|cx| {
            SizeTokens::default().with_control_lg(px(36.)).set(cx);
            crate::init(cx);
            assert_eq!(SizeTokens::read(cx).control_lg(), px(36.));
        });
    }

    /// Installing really installs. `read` falls back to the default when
    /// no policy is present, so only the global's presence distinguishes
    /// an installed policy from an absent one.
    #[gpui::test]
    fn install_provides_the_default_policy_when_none_was_chosen(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            assert!(cx.has_global::<SizeTokens>());
            assert_eq!(*SizeTokens::read(cx), SizeTokens::DEFAULT);
        });
    }

    /// The tiers stay ordered, so an override cannot silently invert one.
    #[test]
    fn the_default_tiers_are_ordered() {
        let tokens = SizeTokens::DEFAULT;
        assert!(tokens.control_sm() < tokens.control_md());
        assert!(tokens.control_md() < tokens.control_lg());
        assert!(tokens.slot_sm() < tokens.slot_md());
        assert!(tokens.control_padding_sm() < tokens.control_padding_lg());
    }
}
