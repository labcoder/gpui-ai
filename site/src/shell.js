import {
  copyFeedback,
  hasWebGpu,
  normalizeTheme,
  persistTheme,
  readStoredTheme,
  specimenOverdrawMargin,
  specimenTransition,
  specimenUrl,
  withTheme,
} from "./runtime.js";

document.documentElement.dataset.enhanced = "true";
const systemDark = window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false;
let storage;
try { storage = window.localStorage; } catch { storage = undefined; }
let activeTheme = normalizeTheme(window.location.search, systemDark, readStoredTheme(storage));

function applyTheme(theme, updateUrl = true, store = false) {
  activeTheme = theme;
  document.documentElement.dataset.theme = theme;
  for (const button of document.querySelectorAll("[data-theme-choice]")) {
    button.setAttribute("aria-pressed", String(button.dataset.themeChoice === theme));
  }
  if (updateUrl) history.replaceState(null, "", withTheme(window.location.href, theme));
  if (store) persistTheme(storage, theme);
  for (const link of document.querySelectorAll("a[href]")) {
    const url = new URL(link.href, window.location.href);
    if (url.origin === window.location.origin && !url.hash) link.href = withTheme(url.href, theme);
  }
  for (const frame of document.querySelectorAll("[data-specimen-frame]")) {
    const nextSource = specimenUrl("../../gallery/embed.html", frame.closest("[data-story]").dataset.story, theme);
    frame.dataset.src = nextSource;
    const open = frame.closest("[data-story]").querySelector("[data-specimen-open]");
    if (open) open.href = nextSource;
    if (frame.hasAttribute("src")) {
      frame.contentWindow?.postMessage({ type: "mighty-gpui-theme", theme }, window.location.origin);
    }
  }
}

for (const button of document.querySelectorAll("[data-theme-choice]")) {
  button.addEventListener("click", () => applyTheme(button.dataset.themeChoice, true, true));
}

applyTheme(activeTheme, !new URLSearchParams(window.location.search).has("theme"));

const frames = [...document.querySelectorAll("[data-specimen-frame]")];
if (!hasWebGpu(navigator)) {
  for (const frame of frames) {
    frame.hidden = true;
    frame.parentElement.querySelector("[data-webgpu-fallback]").hidden = false;
    frame.closest("[data-story]").querySelector("[data-specimen-reload]").disabled = true;
  }
} else {
  const load = (frame) => {
    if (!frame.hasAttribute("src")) frame.src = frame.dataset.src;
  };
  if ("IntersectionObserver" in window) {
    const observer = new IntersectionObserver((entries) => {
      for (const entry of entries) {
        const frame = entry.target;
        const proximity = entry.isIntersecting ? "near" : "far";
        const action = specimenTransition(proximity, frame.hasAttribute("src"));
        if (action === "load") load(frame);
        if (action === "unload") frame.removeAttribute("src");
      }
    }, { rootMargin: specimenOverdrawMargin });
    frames.forEach((frame) => observer.observe(frame));
  } else {
    frames.forEach(load);
  }
}

for (const button of document.querySelectorAll("[data-specimen-reload]")) {
  button.addEventListener("click", () => {
    const frame = button.closest("[data-story]").querySelector("[data-specimen-frame]");
    frame.removeAttribute("src");
    frame.src = frame.dataset.src;
  });
}

for (const button of document.querySelectorAll("[data-copy]")) {
  button.addEventListener("click", async () => {
    const code = button.closest(".code-panel")?.querySelector("code")?.textContent ?? "";
    const status = document.getElementById(button.getAttribute("aria-describedby"));
    try {
      if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(code);
      const feedback = copyFeedback(true);
      button.textContent = feedback.button;
      status.textContent = feedback.status;
    } catch {
      const feedback = copyFeedback(false);
      button.textContent = feedback.button;
      status.textContent = feedback.status;
    }
  });
}
