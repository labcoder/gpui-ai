import { useCallback, useEffect, useId, useRef, useState } from "react";
import { themeGroups, themes } from "./data";
import { demoSrc } from "./links";
import { tellFrame, useTheme } from "./theme";

/** The override value meaning "whatever the page is showing". */
const FOLLOW = "follow";

/**
 * A story running in the shared WASM gallery, inside a window frame.
 *
 * The frame is sized from the height the story reports in `catalog.json`, which
 * the gallery measures from its own laid-out bounds. That is what keeps a
 * three-chip row from sitting in the same tall box as a data table.
 *
 * Four states, and the site owns all of them. Idle: the frame is drawn and
 * nothing has been fetched, because the whole catalog is one binary and a
 * visitor reading prose should not pay for it. Starting: the iframe exists and
 * the title bar says so. Running. And no WebGPU, where the honest thing is to
 * say so and never start a seventeen megabyte download that cannot be used.
 *
 * Starting is said in the title bar, and nowhere else. The embed used to paint
 * a card in the middle of this window announcing itself, over a page of its
 * own painted the theme's background — so opening a page meant watching the
 * window turn black, put up a card, and take it away again. The frame is
 * transparent now and this window's own surface is what shows until there are
 * pixels; the strip that already names the demo is where a window says it is
 * still opening. The embed reports `ready` or `failed` when it knows.
 *
 * The idle state deliberately does not say "loading" and does not shimmer. A
 * frame two viewports down is not loading, will not load, and may never load;
 * animating it would be a prettier version of the same untruth. Starting is
 * different: something really is happening, and the dot says so.
 *
 * There are no variant tabs here. The story draws its own switcher inside the
 * canvas, and a second one on the outside would need a variant parameter
 * plumbed through the embed and the Rust registry only to compete with the
 * control already on screen.
 */
export function Demo({
  story,
  title,
  height,
  caption,
}: {
  readonly story: string;
  readonly title: string;
  readonly height: number;
  readonly caption?: string;
}) {
  const frame = useRef<HTMLDivElement>(null);
  const iframe = useRef<HTMLIFrameElement>(null);
  const [src, setSrc] = useState<string>();
  const [reloads, setReloads] = useState(0);
  const [status, setStatus] = useState<"starting" | "ready" | "failed">("starting");
  const [scrolls, setScrolls] = useState(false);
  const [override, setOverride] = useState<string>(FOLLOW);
  const [linkState, setLinkState] = useState<"idle" | "copied" | "failed">("idle");
  const overrideId = useId();

  const site = useTheme();
  const effective = override === FOLLOW ? site.applied : override;
  const painted = themes.find((candidate) => candidate.slug === effective);

  // Every frame the embed replaces starts over. Reload is the case that
  // matters: it swaps the iframe for a new one, and a window that kept saying
  // "ready" would be describing a document that no longer exists.
  useEffect(() => {
    setStatus("starting");
    setScrolls(false);
  }, [src, reloads]);

  useEffect(() => {
    if (!src) return;
    const onMessage = (event: MessageEvent) => {
      // Same origin, and this component's own frame — a page shows three of
      // these at once on /themes/, and they must not answer for each other.
      if (event.origin !== window.location.origin) return;
      if (event.source !== iframe.current?.contentWindow) return;
      const message = event.data as {
        type?: unknown;
        story?: unknown;
        state?: unknown;
        captured?: unknown;
      } | null;
      if (message?.story !== story) return;
      if (message.type === "gpui-ai-wheel" && typeof message.captured === "boolean") {
        setScrolls(message.captured);
        return;
      }
      if (message.type !== "gpui-ai-status") return;
      if (message.state === "ready" || message.state === "failed") setStatus(message.state);
    };
    window.addEventListener("message", onMessage);

    // A message sent before this listener existed is gone, and the title bar
    // would say "Starting" over a demo that had already started. The embed
    // leaves the same answer on its own document, which is same-origin and
    // readable, so ask rather than rely on having been listening in time.
    try {
      const already = iframe.current?.contentDocument?.body?.dataset;
      if (already?.ready !== undefined) setStatus("ready");
      else if (already?.failed !== undefined) setStatus("failed");
    } catch {
      // A document that is not there to be read yet has not started either.
    }

    return () => window.removeEventListener("message", onMessage);
  }, [src, story, reloads]);

  // Asked after mount, never during render: `navigator` does not exist in the
  // pre-render, and the answer would differ between the two anyway.
  //
  // Undefined until it is known, rather than assuming yes. Starting at `true`
  // left one commit in which the effect below could promote the frame before
  // the answer arrived, which would have downloaded the binary on exactly the
  // machine the card exists to spare. And the question is whether
  // `navigator.gpu` is *there*, not whether the property name exists: a
  // browser that defines the getter and returns nothing answers `in` with yes.
  const [webgpu, setWebgpu] = useState<boolean>();
  useEffect(
    // Typed in by hand: the DOM library this project builds against predates
    // WebGPU, so `navigator.gpu` is not declared anywhere it can see.
    () => setWebgpu(Boolean((navigator as Navigator & { gpu?: unknown }).gpu)),
    [],
  );

  const starting = Boolean(src) && status === "starting" && webgpu !== false;

  useEffect(() => {
    const element = frame.current;
    if (!element || src || webgpu !== true) return;
    if (typeof IntersectionObserver !== "function") {
      setSrc(demoSrc(story));
      return;
    }
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) {
          setSrc(demoSrc(story));
          observer.disconnect();
        }
      },
      // One viewport of lead time, so the demo is running by the time it
      // arrives rather than starting when it is already being looked at.
      { rootMargin: "100% 0px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  }, [story, src, webgpu]);

  // Each frame is told its theme by the component that owns it, and by nothing
  // else. The theme travels as a message rather than in the URL, so a frame
  // already running changes theme without being torn down and rebuilt — and so
  // the address stays the story's own, which is what Pop out and Copy link
  // hand over.
  const tellThisFrame = useCallback(() => {
    if (iframe.current) tellFrame(iframe.current, effective);
  }, [effective]);

  useEffect(tellThisFrame, [tellThisFrame]);

  const reload = useCallback(() => setReloads((count) => count + 1), []);

  const copyLink = useCallback(async () => {
    const url = new URL(window.location.href);
    url.hash = "";
    if (override !== FOLLOW) url.searchParams.set("theme", override);
    try {
      await navigator.clipboard.writeText(url.toString());
      setLinkState("copied");
    } catch {
      // A refused clipboard is a real outcome. Saying nothing leaves a visitor
      // pasting whatever was there before and wondering why.
      setLinkState("failed");
    }
    window.setTimeout(() => setLinkState("idle"), 2_400);
  }, [override]);

  return (
    <figure className="demo">
      <div className="demo-window">
        <div className="demo-titlebar">
          <span className="demo-dots" aria-hidden="true">
            <i />
            <i />
            <i />
          </span>
          <span className="demo-title">{title}</span>
          {starting ? (
            <span className="demo-starting" role="status" data-demo-starting>
              <i aria-hidden="true" />
              Starting
            </span>
          ) : null}
          {!starting && scrolls ? (
            <span className="demo-scrolls" role="status" data-demo-scrolls>
              Scrolls here
            </span>
          ) : null}
        </div>
        <div
          className="demo-body"
          data-specimen-frame=""
          data-story={story}
          data-src={demoSrc(story)}
          ref={frame}
          style={{
            ["--demo-height" as string]: `${height}px`,
            // What the canvas is about to paint, taken from the same theme
            // JSON the gallery loads. The frame is transparent until it draws,
            // so this is what fills the window in the meantime — the demo's
            // own background rather than a neutral one that has to change.
            //
            // Only once there is a frame. Idle and no-WebGPU are the site
            // speaking, and their text is coloured from the site's tokens; a
            // demo overridden to another theme would leave that text standing
            // on a background it was never checked against.
            ...(src && painted
              ? { ["--demo-surface" as string]: painted.tokens["--ai-background"] }
              : {}),
          }}
        >
          {webgpu === false ? (
            <div className="demo-unavailable" data-webgpu-fallback>
              <strong>This demo needs WebGPU</strong>
              <p>
                Your browser cannot draw the component, so nothing has been downloaded. Everything
                else on this page — the code, the events, the source — is the same either way.
              </p>
            </div>
          ) : src ? (
            // Told again on load, because a frame that has only just been
            // created has no listener yet — which is exactly the state Reload
            // leaves it in.
            <iframe
              key={reloads}
              ref={iframe}
              src={src}
              title={title}
              loading="lazy"
              onLoad={tellThisFrame}
            />
          ) : (
            <p className="demo-idle">Starts when it scrolls into view</p>
          )}
        </div>
        <div className="demo-toolbar">
          <label htmlFor={overrideId}>Theme</label>
          <select
            id={overrideId}
            value={override}
            onChange={(event) => setOverride(event.target.value)}
          >
            <option value={FOLLOW}>Follow site</option>
            {themeGroups.map((group) => (
              <optgroup key={group.id} label={group.label}>
                {group.themes.map((theme) => (
                  <option key={theme.slug} value={theme.slug}>
                    {theme.label}
                  </option>
                ))}
              </optgroup>
            ))}
          </select>
          <button type="button" data-specimen-reload onClick={reload} disabled={!src}>
            Reload
          </button>
          {/* Always the effective theme, not just an override. Popped out
              there is no page around the frame to follow, and an embed told
              nothing guesses from the viewer's own light/dark preference — so
              "Follow site" used to open the demo in a theme the site was not
              showing. */}
          <a data-specimen-open href={demoSrc(story, effective)}>
            Pop out
          </a>
          <button type="button" data-specimen-link onClick={copyLink}>
            Copy link
          </button>
          <span className="copy-status" role="status" aria-live="polite">
            {linkState === "copied" ? "Link copied" : null}
            {linkState === "failed" ? "Your browser would not let the page copy it" : null}
          </span>
        </div>
      </div>
      <Readout theme={effective} following={override === FOLLOW} />
      {caption ? <figcaption>{caption}</figcaption> : null}
    </figure>
  );
}

/**
 * What the frame is painted from, in the frame's own terms.
 *
 * These numbers are not a description of the demo; they are its input. They
 * come from the same theme JSON the gallery loads, so a visitor comparing two
 * themes can see what actually differs rather than guessing from the picture.
 */
function Readout({ theme, following }: { readonly theme: string; readonly following: boolean }) {
  const entry = themes.find((candidate) => candidate.slug === theme);
  if (!entry) return null;

  const name = entry.label.toUpperCase();
  const mode = entry.mode.toUpperCase();
  const parts = [
    name,
    // "DARK · DARK" says one thing twice. Themes called Light and Dark carry
    // their mode in the name already; Ember Dusk does not.
    ...(name.includes(mode) ? [] : [mode]),
    `RADIUS ${entry.radius}/${entry.radiusLg}`,
    `BASE ${entry.fontSize}PX`,
    entry.shadow ? "SHADOW" : "FLAT",
  ];
  if (!following) parts.push("OVERRIDDEN");

  return (
    <p className="demo-readout" data-readout={theme}>
      {parts.join(" · ")}
    </p>
  );
}
