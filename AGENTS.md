# AGENTS.md

Working agreements for anyone — human or agent — making changes in this repository.

## Start here

Read [README.md](README.md) for the consumer-facing project surface, then inspect the relevant crate and its tests. This file is the source of truth for contributor architecture, dependency, and quality rules.

## What this project is

A layer of AI-native components above gpui-component, exactly as Beautiful UI layers above shadcn/ui. It is an application-independent library first; a showcase site comes later.

## Architecture boundaries

- **Never fork or copy upstream components.** Compose gpui-component's styled components; when their styling is too opinionated for a design, drop down to `gpui-base` behaviors and own the presentation. If neither works, write a custom element — do not vendor upstream code.
- **All presentation comes from theme tokens.** Every color, radius, spacing, shadow, and type style resolves through `cx.theme()`. Zero hardcoded colors in `crates/mighty-gpui`. This rule is what makes the entire theme system (light/dark, bundled themes, custom JSON themes, live color editing) work for free — it is not negotiable.
- **Progress is modeled once.** Components that display progressive work consume `Progressive<T>` and `ProgressState`. Do not add per-component timers or bespoke lifecycle enums.
- **Demo data stays out of the library.** Simulated token streams, fake transcripts, and fixtures live in `crates/gallery`. Library components take real data types.
- **The site is plain web tech.** Page chrome (nav, docs, code panels) is HTML/JS in `site/`; only component examples run as WASM, via a single shared gallery binary. Do not build page chrome in WASM.

## Dependency policy (read before touching any Cargo.toml)

GPUI's crates.io release is outdated; the real work lives in the Zed repository. All gpui crates come from git:

- Declare gpui exactly as `{ git = "https://github.com/zed-industries/zed" }` with **no `rev` field**. gpui-component declares it the same way; adding `rev` makes Cargo treat it as a different source, which produces two incompatible copies of gpui and breaks every shared type.
- The actual commit is pinned in `Cargo.lock`, and it must match the revision gpui-component's own `Cargo.lock` pins. To set or fix it:

  ```sh
  cargo update -p gpui --precise <rev-from-gpui-component-lock>
  ```

  The current component and Zed revisions are authoritative in `Cargo.toml` and `Cargo.lock`; `npm run check:upstream` verifies that they agree with the selected upstream lockfile.
- To bump upstream, run `npm run update:upstream` — it resolves the latest `gpui-component` commit, updates `gpui-component` and `gpui-component-assets` as one exact pair, reads the gpui revision from *their* `Cargo.lock`, updates our `Cargo.toml` + `Cargo.lock`, and runs `cargo check`. (`script/update-upstream.sh` is the implementation; pass a full revision with `npm run update:upstream -- <rev>`.) Commit `Cargo.toml` and `Cargo.lock` together.
- `Cargo.lock` is tracked in git — it is the pinning mechanism. Never delete it casually.
- Do not add new dependencies without need; prefer what gpui, gpui-component, and the standard library already provide.
- Keep workspace packages unpublished while required GPUI dependencies come from Git. Revisit registry publication only when Cargo can package the complete dependency graph from accepted registry sources.

## Code standards

- Stateless components are `RenderOnce` fluent builders (`ToolChip::new("id", "label").status(...)`); stateful composites (chat, prompt bar) are entities. Every interaction uses an `on_event` callback, a public typed event enum, and stable application IDs rather than collection indices. No stringly-typed APIs.
- Follow gpui-component's naming and module conventions so the two libraries feel like one ecosystem.
- `#![deny(missing_docs)]` holds on the library crate; public items get doc comments with runnable examples where practical.
- No `unwrap` in library code (`clippy::unwrap_used` is denied); `expect` with a meaningful message is acceptable in the gallery binary only.
- Every component gets a story in `crates/gallery` in the same change that adds the component.
- Clippy runs clean under `--deny warnings`; run `cargo fmt` before committing.

## Accessibility and usability

- Raw `SharedString`, `String`, and `&str` children are visual-only in GPUI. Put meaningful static text behind a named semantic parent, or use an identified text element. User-readable prose, markdown, and code that a person would reasonably copy must use gpui-component's selectable text behavior.
- Compose custom controls from `gpui-base` or gpui-component behaviors. Every interactive element needs a stable ID, the correct AccessKit role/name/state, keyboard activation, and a visible theme-token focus treatment. Do not render a focusable control without a handler.
- Color, icons, and motion may reinforce state but cannot be its only carrier. Expose pending/running/success/failure, checked/expanded, progress, and error information semantically.
- Repeating animation must use GPUI's animation facilities so reduced-motion mode produces a useful static frame. Essential meaning or controls must never depend on animation.
- Any gallery, list, trace, or composite whose content can exceed its bounds must have an intentional overflow strategy and a regression test that proves the final content remains reachable.
- Add direct AccessKit regression tests for component roles, names, descriptions, values, and actions. Pointer-only tests are insufficient for interactive controls.

## Definition of done for a component

1. Compiles natively and for `wasm32-unknown-unknown`.
2. Has a gallery story exercising its real states (loading, streaming, error, done — whichever apply).
3. Verified against at least three themes including light and dark — no hardcoded-color leaks.
4. Visually compared side-by-side with its beautifului.dev reference; deliberate differences noted in the story.
5. Readable content can be selected/copied where expected; every control is keyboard-operable with visible focus, and semantic state is covered by an AccessKit regression test.
6. Reduced-motion behavior is useful, and constrained layouts keep all content reachable by scrolling or another explicit overflow design.
7. Public API documented; `cargo doc` builds without warnings.

## Verification commands

```sh
npm run check               # fmt + clippy --deny warnings + script tests + Rust tests + docs
npm run check:native        # native workspace build check
npm run build:wasm          # compile and bind the shared gallery for the browser
npm run check:web           # web host tests
npm run dev                 # look at it — visual review is part of done
npm run prod                # optimized native gallery for performance review
npm run dev:web             # inspect the real WASM gallery in a browser
```

The root `package.json` is a task runner only (no JavaScript dependencies); the scripts shell out to cargo. Add new workflows there so they stay discoverable.

## Durable vs. temporary

Consumer guidance belongs in the README, crate documentation, and the future public site. Contributor rules that every change must obey belong in this file or executable tooling. Development decisions, implementation plans, scratch notes, and exploratory output belong in ignored `docs/internal/` and are never prerequisites for consuming the library.
