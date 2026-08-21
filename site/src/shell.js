import {
  copyFeedback,
  catalogMatches,
  hasWebGpu,
  normalizeTheme,
  persistTheme,
  readStoredTheme,
  resolveSpecimenBase,
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
    frame.dataset.galleryBase ||= resolveSpecimenBase(frame.dataset.src, window.location.href);
    const nextSource = specimenUrl(frame.dataset.galleryBase, frame.closest("[data-story]").dataset.story, theme);
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

document.querySelector('.skip-link[href="#content"]')?.addEventListener("click", (event) => {
  const content = document.getElementById("content");
  if (!content) return;
  event.preventDefault();
  content.focus();
  content.scrollIntoView();
  history.replaceState(null, "", `${location.pathname}${location.search}#content`);
});

const navToggle = document.querySelector("[data-nav-toggle]");
const navPanel = document.getElementById("site-nav-panel");
const navBackground = [...document.body.children].filter((item) => item !== navPanel);
let navReturnFocus;
const closeNav = () => {
  if (!navPanel || navPanel.hidden) return;
  navPanel.hidden = true;
  navBackground.forEach((item) => { item.removeAttribute("inert"); });
  navToggle?.setAttribute("aria-expanded", "false");
  document.body.classList.remove("nav-open");
  navReturnFocus?.focus();
};
const openNav = () => {
  if (!navPanel) return;
  navReturnFocus = navToggle;
  navPanel.hidden = false;
  navBackground.forEach((item) => { item.setAttribute("inert", ""); });
  navToggle?.setAttribute("aria-expanded", "true");
  document.body.classList.add("nav-open");
  navPanel.querySelector(".nav-drawer [data-nav-close]")?.focus();
};
navToggle?.addEventListener("click", openNav);
navPanel?.querySelectorAll("[data-nav-close]").forEach((button) => button.addEventListener("click", closeNav));
navPanel?.addEventListener("click", (event) => {
  if (event.target === navPanel) closeNav();
});
navPanel?.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    closeNav();
    return;
  }
  if (event.key !== "Tab") return;
  const focusable = [...navPanel.querySelectorAll("a[href], button:not([disabled])")].filter((item) => !item.hidden);
  const first = focusable[0];
  const last = focusable.at(-1);
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last?.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first?.focus();
  }
});

const catalogSearch = document.querySelector("[data-catalog-search]");
catalogSearch?.addEventListener("input", () => {
  let visible = 0;
  for (const item of document.querySelectorAll("[data-catalog-item]")) {
    const matches = catalogMatches({
      title: item.dataset.title,
      category: item.dataset.category,
      summary: item.dataset.summary,
    }, catalogSearch.value);
    item.hidden = !matches;
    if (matches) visible += 1;
  }
  for (const group of document.querySelectorAll(".catalog-group")) {
    group.hidden = !group.querySelector("[data-catalog-item]:not([hidden])");
  }
  const status = document.querySelector("[data-catalog-status]");
  if (status) status.textContent = `${visible} ${visible === 1 ? "component" : "components"}`;
});

const frames = [...document.querySelectorAll("[data-specimen-frame]")];
if (!hasWebGpu(navigator)) {
  for (const frame of frames) {
    frame.hidden = true;
    frame.parentElement.querySelector("[data-webgpu-fallback]").hidden = false;
    const reload = frame.closest("[data-story]").querySelector("[data-specimen-reload]");
    if (reload) reload.disabled = true;
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
