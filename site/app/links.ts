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
export function demoSrc(story: string, theme?: string): string {
  const query = new URLSearchParams({ story });
  if (theme) query.set("theme", theme);
  return `${href("/gallery/embed.html")}?${query.toString()}`;
}

/** The component's implementation on GitHub, at the released tag. */
export function sourceHref(component: Component, repository: string, version: string): string {
  return `${repository}/blob/v${version}/${component.source}`;
}
