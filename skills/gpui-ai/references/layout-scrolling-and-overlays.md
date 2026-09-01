# Layout, scrolling, and overlays

Most gpui-ai layout failures happen at composition boundaries: a correct
component is mounted in a host that will not let it shrink, scroll, or paint a
transient surface outside a mask.

## Bounded flex hosts

A flexible child can scroll only when its ancestors give it a finite region.
In a vertical flex composition, put `min_h_0()` on the flexible parent that
contains a vertically growing component. Use `min_w_0()` for horizontally
constrained content whose text or table must shrink instead of forcing the
parent wider.

```rust
v_flex()
    .size_full()
    .child(header)
    .child(div().flex_1().min_h_0().child(chat))
```

Do not fix clipping by assigning an arbitrary large height. Test the actual
smallest supported window and increased rem size.

## One intentional overflow owner

The generated component index records whether each surface grows vertically or
wide. Let the component that understands its content own that scrolling
behavior. Nested scroll regions that respond to the same wheel or touch gesture
make final content difficult to reach.

When composing several surfaces, decide explicitly which region owns vertical
growth and which may scroll horizontally. Keep headers, footers, and primary
actions outside the content scroller when they must remain available.

Entity-backed lists and tables already virtualize growing collections. Preserve
their entity and stable row identity; recreating one for each snapshot discards
scroll anchors, focus, measurement caches, and incremental work.

## Scrollbar policy

`ScrollbarTokens` chooses visibility and whether the bar overlays content or
reserves a gutter. An overlaid bar may cross reflowing prose, but it must not
cover a button, menu trigger, resize target, or other hit target.

gpui-ai components reserve their own control lanes. When adding an application
control inside a custom scroll row, either leave room for the overlaid bar or
choose gutter placement for the application. Do not hide the scrollbar merely
to avoid the collision; the surface should advertise that it scrolls.

## Popups and transient surfaces

Use gpui-component popovers and menus or gpui-ai surfaces backed by the shared
positioner. `PopupTokens` supplies a preferred side, but placement is adaptive:
the positioner flips when the preferred side does not fit and clamps when
neither side has enough room.

Do not hand-position a menu with a guessed offset inside a virtual list or
scroll mask. It may compile and then be clipped or move with recycled content.
Mount transient content through the component's public popup path and verify it
near every window edge.

Escape should dismiss the topmost transient surface and return focus to its
trigger. Opening must have a keyboard path; hover and pointer activation can
reinforce it but cannot be the only path.

## Composition review

For each bounded surface, verify:

- the last item remains reachable;
- the current item remains anchored when content arrives above or below it;
- controls remain clickable beside an overlay scrollbar;
- menus remain visible near viewport edges;
- focus does not disappear when virtualization recycles a row;
- changing rem size invalidates measurements without jumping to unrelated
  content.
