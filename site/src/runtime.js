export const themes = Object.freeze(["light", "dark", "contrast"]);
export const themeStorageKey = "gpui-ai-theme";
export const specimenOverdrawMargin = "400px 0px";

export function normalizeTheme(search, systemDark = false, storedTheme) {
  const requested = new URLSearchParams(search).get("theme");
  if (themes.includes(requested)) return requested;
  if (themes.includes(storedTheme)) return storedTheme;
  return systemDark ? "dark" : "light";
}

export function readStoredTheme(storage) {
  try {
    const theme = storage?.getItem(themeStorageKey);
    return themes.includes(theme) ? theme : undefined;
  } catch {
    return undefined;
  }
}

export function persistTheme(storage, theme) {
  try {
    if (!themes.includes(theme)) return false;
    storage?.setItem(themeStorageKey, theme);
    return Boolean(storage);
  } catch {
    return false;
  }
}

export function withTheme(href, theme) {
  const url = new URL(href);
  url.searchParams.set("theme", theme);
  return url.href;
}

export function specimenUrl(base, story, theme) {
  const params = new URLSearchParams({ story, theme });
  return `${base}?${params}`;
}

export function resolveSpecimenBase(source, pageUrl) {
  const url = new URL(source, pageUrl);
  url.search = "";
  url.hash = "";
  return url.href;
}

export function specimenTransition(proximity, isLoaded) {
  if (proximity === "near" && !isLoaded) return "load";
  if (proximity === "far" && isLoaded) return "unload";
  return "idle";
}

export function copyFeedback(copied) {
  return copied
    ? { button: "Copied", status: "Rust example copied to the clipboard." }
    : { button: "Copy", status: "Could not copy automatically. Select the code and copy it manually." };
}

export function catalogMatches(item, query) {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return true;
  return [item.title, item.category, item.summary]
    .join(" ")
    .toLocaleLowerCase()
    .includes(normalized);
}
