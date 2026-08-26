// Route-scoped access to the generated code payloads.
//
// Every page used to bundle every other page's code: `data.ts` statically
// imported the full highlighted corpus and the raw snippets, ~313 KB that the
// catalog, themes, and docs routes never read. The generator now emits one
// chunk per component plus one for the documentation samples
// (`site/generated/code/`), and this module loads exactly the chunk the
// current route needs — awaited once before render, so the pre-rendered HTML
// stays complete and hydration never sees a page missing its code.
//
// The accessors stay synchronous on purpose. The site renders with
// `renderToString` and hydrates markup that already contains the code, so the
// data must be present before React runs; a Suspense boundary here would
// trade a one-chunk await for a hydration mismatch. `main.tsx` and
// `prerender.tsx` are the two callers of `preloadCodeFor`, and there is no
// client-side router — navigation is a page load, which loads its own chunk.

import { install, type CodeSample, type CodeToken } from "./data";
import type { Route } from "./routes";

interface CodeChunk {
  readonly raw?: Readonly<Record<string, string>>;
  readonly lines?: Readonly<Record<string, readonly (readonly CodeToken[])[]>>;
  readonly samples?: Readonly<Record<string, CodeSample>>;
}

const chunks = import.meta.glob("../generated/code/*.json");
const loaded = new Map<string, CodeChunk>();

async function load(name: string): Promise<void> {
  if (loaded.has(name)) return;
  const loader = chunks[`../generated/code/${name}.json`];
  // An unknown slug has no chunk and renders no code, which the panel already
  // treats as "nothing to show" — the same answer the old lookup gave.
  if (!loader) return;
  const module = (await loader()) as { default: CodeChunk };
  loaded.set(name, module.default);
}

/** Loads the code the given route renders; resolves immediately for the rest. */
export async function preloadCodeFor(route: Route): Promise<void> {
  if (route.kind === "component" && route.slug) return load(route.slug);
  // Both documentation kinds render hand-written samples from one shared chunk.
  if (route.kind === "docs" || route.kind === "doc") return load("samples");
}

/** The copyable Rust for one story variant, cut from the gallery's source. */
export function snippet(slug: string, variant = "default"): string | undefined {
  return loaded.get(slug)?.raw?.[variant];
}

/** The same snippet, split into theme-paintable tokens at build time. */
export function highlighted(
  slug: string,
  variant = "default",
): readonly (readonly CodeToken[])[] | undefined {
  return loaded.get(slug)?.lines?.[variant];
}

/**
 * One of the documentation samples in `site/content/samples/`.
 *
 * Throws rather than rendering nothing, exactly as before the split: a page
 * asking for a sample that is not there has lost a code block, and a silent
 * gap in prose that reads "as in:" is worse than a build that stops. The
 * documentation routes preload the samples chunk, so at render time absence
 * means a missing file, not a missing await.
 */
export function sample(name: string): CodeSample {
  // The install lines are eager data (the home page paints them first-screen),
  // but documentation prose refers to them by name like any other sample.
  if (name === "install") return install;
  const found = loaded.get("samples")?.samples?.[name];
  if (!found) throw new Error(`site/content/samples has no ${name}`);
  return found;
}
