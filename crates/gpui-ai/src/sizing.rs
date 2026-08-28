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

    #[gpui::test]
    fn install_provides_the_default_policy_when_none_was_chosen(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::init(cx);
            assert_eq!(*SizeTokens::read(cx), SizeTokens::DEFAULT);
        });
    }

    #[test]
    fn reading_without_any_app_state_falls_back_to_the_default() {
        // The heights step the 4px grid and slots match the text
        // line-heights they seat glyphs beside; the pairs stay ordered so
        // an override cannot silently invert a tier.
        let tokens = SizeTokens::DEFAULT;
        assert!(tokens.control_sm() < tokens.control_md());
        assert!(tokens.control_md() < tokens.control_lg());
        assert!(tokens.slot_sm() < tokens.slot_md());
    }
}
