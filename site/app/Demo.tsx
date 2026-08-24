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
 * Four states, and the site owns three of them. Idle: the frame is drawn and
 * nothing has been fetched, because the whole catalog is one binary and a
 * visitor reading prose should not pay for it. Promoted: the iframe exists and
 * the embed shows its own loading card while the module arrives. Running. And
 * no WebGPU, where the honest thing is to say so and never start a seventeen
 * megabyte download that cannot be used.
 *
 * The idle state deliberately does not say "loading" and does not shimmer. A
 * frame two viewports down is not loading, will not load, and may never load;
 * animating it would be a prettier version of the same untruth.
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
  const [override, setOverride] = useState<string>(FOLLOW);
  const [linkState, setLinkState] = useState<"idle" | "copied" | "failed">("idle");
  const overrideId = useId();

  const site = useTheme();
  const effective = override === FOLLOW ? site.applied : override;

  // Asked after mount, never during render: `navigator` does not exist in the
  // pre-render, and the answer would differ between the two anyway.
  const [webgpu, setWebgpu] = useState(true);
  useEffect(() => setWebgpu("gpu" in navigator), []);

  useEffect(() => {
    const element = frame.current;
    if (!element || src || !webgpu) return;
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
        </div>
        <div
          className="demo-body"
          data-specimen-frame=""
          data-story={story}
          data-src={demoSrc(story)}
          ref={frame}
          style={{ ["--demo-height" as string]: `${height}px` }}
        >
          {!webgpu ? (
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
          <a data-specimen-open href={demoSrc(story, override === FOLLOW ? undefined : override)}>
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
