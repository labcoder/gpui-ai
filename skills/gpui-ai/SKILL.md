---
name: gpui-ai
description: Build or modify Rust GPUI application interfaces that consume gpui-ai. Use this skill whenever a task mentions gpui-ai or asks for streamed model output, chat and prompt composers, thinking traces, tool calls, approvals, plans, clarification questions, AI queues, context or evidence, agent-oriented tables, or other AI-native surfaces in a GPUI application—even when gpui-ai is not named. It covers version-compatible setup, component selection, application-owned state, progressive lifecycles, typed events, styling, composition, accessibility, performance, native and WASM verification. Do not use it for contributing new components or internal changes to the gpui-ai repository.
compatibility: Rust application using GPUI, gpui-component, and the Git-hosted gpui-ai crate.
---

# Using gpui-ai

gpui-ai is the application-facing layer for UI that AI products keep
rebuilding. It sits above gpui-component rather than replacing it:

```text
application state, work, IDs, and clock
             ↓ snapshots and typed intent
         gpui-ai components
             ↓ controls and theme tokens
          gpui-component
             ↓
             GPUI
```

Use gpui-component for ordinary buttons, inputs, dialogs, menus, docks, tabs,
and layout. Use gpui-ai when the surface has agent-specific meaning: progressive
answers, tool work, approval, evidence, plans, prompt composition, and similar
workflows.

## Before implementing

1. Resolve the application's exact gpui-ai Git revision from `Cargo.toml` and
   `Cargo.lock`. APIs on another tag or on repository HEAD may differ.
2. Read [setup and versioning](references/setup-and-versioning.md) when adding
   the dependency, initializing an application, or diagnosing incompatible
   GPUI types.
3. Read [component selection](references/component-selection.md), then consult
   the [generated component index](references/generated/components.md). The
   former carries judgment; the latter carries checkout-derived facts.
4. Search the source or rustdoc for the pinned revision before writing a method
   call. Plausible APIs borrowed from React, web component libraries, or older
   GPUI revisions are the most common bad output.
5. Load the other references that match the task. Read a selected reference in
   full before acting on it.

If the `gpui` and `gpui-component` skills are available, use them for framework
and general component questions. This skill only adds gpui-ai's layer.

## Core ownership model

Treat every component as a view of application-owned truth:

- The application owns model requests, retries, storage, durable state,
  lifecycle transitions, clocks, and domain IDs.
- A component renders the snapshot it receives and reports user intent through
  a typed event. An event is not permission for the component to perform the
  application's work.
- Repeated content uses stable domain IDs. A row, message, tool call, result, or
  option must survive replacement and reordering without becoming a different
  object.
- Progressive work uses `Progressive<T>`, `ProgressState`, or
  `StreamedContent`; do not invent a component-specific timer or lifecycle.
- Build a stateless `RenderOnce` component where it is rendered. Create an
  entity-backed component once, retain its `Entity<T>` and subscriptions, and
  update it through its public setters.

Read [state, events, and progress](references/state-events-and-progress.md)
before implementing state flow or asynchronous updates.

## Implementation workflow

1. **Name the job.** Decide whether this is general application UI or an
   AI-native surface. Keep general controls in gpui-component.
2. **Choose the smallest component or composition.** Prefer one existing
   component over recreating its behavior. Combine components only when the
   workflow actually contains several distinct jobs.
3. **Model the domain first.** Define stable IDs, controlled values, progressive
   state, and application transitions before assembling the view.
4. **Wire intent back upward.** Handle the component's real typed event,
   perform the application transition, update the snapshot, and notify the
   narrowest owner.
5. **Style at the component frame.** Apply `Styled` methods directly to the
   component. Use active theme tokens and rem-relative layout; use decoration
   slots for application expression that sits behind or above the content.
6. **Respect motion and geometry.** Use the shared policies and animation
   helpers. Make bounded content reachable and let popups flip or clamp through
   the shared positioning behavior.
7. **Verify behavior, not construction.** Exercise real state transitions,
   typed events, keyboard paths, semantic output, constrained layouts, theme
   changes, and the targets the application ships.

## Reference routing

| Read | When |
| --- | --- |
| [Setup and versioning](references/setup-and-versioning.md) | Dependencies, initialization, assets, incompatible duplicate GPUI crates |
| [Component selection](references/component-selection.md) | Choosing a component or a multi-component workflow |
| [Generated component index](references/generated/components.md) | Current names, constructors, events, lineage, source modules, overflow direction |
| [State, events, and progress](references/state-events-and-progress.md) | Entities, controlled snapshots, subscriptions, streaming, stable IDs, cues |
| [Styling, themes, and motion](references/styling-themes-and-motion.md) | Caller styles, theme tokens, policies, decorations, reduced motion |
| [Layout, scrolling, and overlays](references/layout-scrolling-and-overlays.md) | Flex constraints, reachability, virtualized content, popup and scrollbar composition |
| [WASM and embedding](references/wasm-and-embedding.md) | Browser builds, clocks, fonts, assets, focus and touch behavior |
| [Verification and review](references/verification-and-review.md) | Tests, accessibility, target checks, or reviewing existing gpui-ai usage |

## Source priority

When sources disagree, use this order:

1. The public source and rustdoc at the application's locked revision.
2. A compiled example from that same revision.
3. The generated component index shipped with that revision.
4. Narrative website and README examples.

This order protects an older consumer from a newer skill and protects every
consumer from an attractive example that no longer compiles.

## Completion standard

Before handing work back, confirm that the result:

- uses the real API at the locked revision;
- keeps durable state and time in the application;
- preserves stable identity through replacement and reordering;
- handles typed intent and updates the visible controlled value;
- works by keyboard and exposes meaningful semantic state;
- remains useful with reduced motion, another theme, increased rem size, and a
  constrained host;
- compiles for every native or WASM target the application claims to support.
