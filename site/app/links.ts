// Every URL the site emits, in one place.
//
// The site is served from a project page, so every internal link carries the
// repository name. Vite substitutes `BASE_URL` at build time in both the client
// bundle and the pre-render, so the server and the browser agree.

import type { Component } from "./data";

const BASE: string = import.meta.env.BASE_URL;

/**
 * Turns a route path into a link.
 *
 * Routes are stored base-relative and always end in a slash, which is the
 * directory the pre-render wrote `index.html` into. Linking the directory
 * rather than the file keeps the URL bar clean and keeps the client's route
 * lookup in agreement with the markup it hydrates.
 */
export function href(path: string): string {
  return `${BASE.replace(/\/$/, "")}${path}`;
}

/** Where the shared WASM gallery lives, relative to the site root. */
export function demoSrc(story: string, theme?: string, variant?: string): string {
  const query = new URLSearchParams({ story });
  if (theme) query.set("theme", theme);
  // Only when there is one to name. Five of the thirty-four stories offer
  // states; the rest would carry an empty parameter for nothing.
  if (variant) query.set("variant", variant);
  return `${href("/gallery/embed.html")}?${query.toString()}`;
}

/**
 * The still frame captured from a story, for a reader who is not running it.
 *
 * By mode, not by theme. There are 45 themes and 35 stories; a poster each
 * would be 1,575 files rendered on every build to make a placeholder slightly
 * more accurate. `site/scripts/capture-posters.mjs` renders these two, and
 * `Demo` only shows one where its colours cannot contradict what is about to
 * be drawn over them.
 */
export function posterSrc(story: string, mode: string): string {
  return href(`/posters/${story}-${mode}.webp`);
}

/** The width every poster is captured at, and the width they were measured at. */
export const POSTER_WIDTH = 900;

/**
 * The rustdoc page for a component's type.
 *
 * Rustdoc lays items out by module, and a component's module is the file the
 * catalog already records as its source. `catalog.test.mjs` holds the two
 * assumptions this makes — that the source path is a crate module and that the
 * type is a struct — so an item that stops matching fails the gate rather than
 * shipping a dead link.
 */
export function apiHref(component: Component): string {
  const module = component.source.replace(/^.*\//, "").replace(/\.rs$/, "");
  return href(`/api/gpui_ai/${module}/struct.${component.api}.html`);
}

/**
 * The component's implementation on GitHub.
 *
 * Pinned to `main`, not to the last tag: the summaries, snippets, and measured
 * heights on this page are generated from the tree the site was built from, so
 * a tag link would open code that does not match what is described here.
 */
export function sourceHref(component: Component, repository: string): string {
  return `${repository}/blob/main/${component.source}`;
}
