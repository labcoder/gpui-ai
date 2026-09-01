# Styling, themes, and motion

gpui-ai inherits gpui-component's semantic theme. Application customization
should preserve that semantic layer so the same component remains legible in
light, dark, bundled, and custom themes.

## Style the component itself

Every component accepts GPUI `Styled` methods. Caller refinements are applied
after component defaults:

```rust
ApprovalCard::new("publish", "Publish the launch plan?")
    .border_color(cx.theme().border)
    .bg(cx.theme().background)
```

Apply the refinement to the component frame. A wrapper paints around the
component and does not replace its own background, border, radius, or text ink.
For entity-backed components, apply styles at construction unless its current
API exposes a later mutation.

## Tokens and rem-relative layout

Use `cx.theme()` and semantic tokens for normal UI color, type, radius, shadow,
and spacing. Use rem-relative helpers for application layout so base-font zoom
changes geometry with text.

Raw pixels are appropriate for physical facts: measured geometry, raster
dimensions, a small animation displacement from a token-resolved resting
position, or virtual-list overdraw. They are not a second spacing scale.

If custom application code caches any resolved layout or text measurement, key
the cache on `window.rem_size()`. A cached row estimate that ignores rem changes
will clip or drift when the user zooms.

Use gpui-component's theming guidance for loading or authoring a custom JSON
theme. gpui-ai does not introduce a parallel color system.

## Global policies

gpui-ai installs application-wide policy objects during `gpui_ai::init`:

- `MotionTokens` — reduced-motion preference, tempos, springs, and shared
  transition roles;
- `SizeTokens` — control heights, label padding, and glyph slots;
- `ScrollbarTokens` — visibility and overlay/gutter placement;
- `PopupTokens` — preferred side and hover open/close timing.

An application may replace a policy before opening windows. Inspect the pinned
revision for its `with_*` builders and call `.set(cx)` once. Prefer one coherent
application policy over per-component tweaks that make related controls move or
size differently.

## Decorations

Use a `Decoration` for application-owned expression behind or above a framed
component: an image, branded field, scrim, canvas, or other element that should
not change layout.

```rust
use gpui_ai::prelude::{Decoration, decoration};

ApprovalCard::new("publish", "Publish?").decoration(
    Decoration::behind(
        img("nebula.jpg").rounded(decoration::frame_radius(cx)),
    )
    .and_above(div().size_full().bg(scrim)),
)
```

A layer that reaches a component edge must carry
`decoration::frame_radius(cx)`. GPUI clips content to a rectangular mask, not a
rounded subtree. A layer that never reaches a corner does not need the radius.

The above layer passes pointer input through to the component. It should remain
visual reinforcement, not hide a separate interactive control.

Use `decoration::animated` for a repeating, visibility-aware loop and
`decoration::toward` for an application value easing toward a target. They use
the shared motion preference and stop paying for invisible work. Do not detach
an independent animation clock for a decoration.

Normal component presentation stays token-driven. A decoration may
intentionally carry brand or image colors because it belongs to the
application, but the application is responsible for text contrast in every
state and theme.

## Motion semantics

Motion should explain a change: arrival, acknowledgment, retargeting,
expansion, reordering, or progress. It cannot be the only carrier of meaning.

- Use the library's role-specific helpers and policies rather than unrelated
  durations and easings.
- Keep visible motion fluid; save work by suspending offscreen or settled
  animations, not by making on-screen movement coarse.
- Reduced motion must leave a useful static frame and immediate access to all
  controls and meaning.
- When an application value changes mid-transition, retarget from the current
  visual value instead of restarting from the old origin.
