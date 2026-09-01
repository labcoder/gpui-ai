# Verification and review

Verify the application behavior that gpui-ai is meant to preserve. A test that
repeats a constructor's input or asserts a local constant has not protected the
user from anything.

## Build and documentation

Run the commands appropriate to the consumer repository, at minimum:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- --deny warnings
cargo test --workspace
cargo doc --workspace --no-deps
```

Add `cargo check --target wasm32-unknown-unknown` and the real browser-host tests
when the application ships WASM. Do not run gpui-ai's own repository task
runner in an unrelated consumer application.

## Behavioral tests

Prefer change-detector tests that cross a public boundary:

- emit a real typed event and assert the application updates its controlled
  snapshot;
- append or replace progressive content and assert the visible/semantic result
  advances once;
- replace, sort, or filter items and assert stable IDs preserve selection,
  focus, and scroll anchoring;
- exercise Pending, Running, Complete, and Failed states through the same
  application transition used in production;
- activate controls through AccessKit/keyboard paths as well as the pointer;
- constrain the host and prove the final item and primary action remain
  reachable;
- enable reduced motion and prove the resulting static frame still carries the
  meaning.

Avoid tests that merely reconstruct implementation details: checking that a
builder stores the value just passed to it, duplicating a match table in the
test, or asserting exact private child counts. Those tests resist refactoring
without detecting a broken consumer contract.

## Visual matrix

Review representative states in at least:

- one light theme;
- one dark theme;
- one visually distinct bundled or custom theme;
- the default rem size and an increased rem size;
- the normal host and the smallest supported constrained host;
- normal and reduced motion.

Look at loading, progressive, failed, empty, disabled, selected, focused, and
resolved states that apply. Verify readable text can be selected or copied
where a person would reasonably expect it.

## Domain review checklist

When reviewing existing gpui-ai usage, report findings by severity and point to
the application source. Check for:

### Ownership

- durable model/tool work or fake progress running inside a view;
- entity-backed components recreated during render;
- subscriptions dropped too early or detached work outliving its owner;
- broad root notifications where a narrower entity owns the change.

### Identity and events

- collection indices used as element, row, message, or option IDs;
- string commands where a typed event exists;
- an event logged but not reflected back into the controlled value;
- pointer-only activation or focusable controls without handlers.

### Presentation

- raw colors replacing semantic tokens in ordinary UI;
- arbitrary pixel spacing that fails base-font zoom;
- wrappers attempting to override a component's own frame;
- motion without a reduced-motion result;
- color, sound, icon, or animation as the only state carrier.

### Layout and performance

- unbounded content without an overflow owner;
- missing `min_h_0`/`min_w_0` at a flexible composition boundary;
- overlay scrollbars covering controls;
- hand-positioned menus inside clipped or virtualized content;
- rebuilding whole snapshots, text measurement, or layout on an unchanged
  progressive revision;
- offscreen repeating work that continues requesting frames.

Distinguish consumer misuse from a gpui-ai defect. If the application uses the
public API correctly and the component still violates its contract, document a
minimal reproduction rather than teaching a workaround as normal usage.
