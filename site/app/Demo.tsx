import { useEffect, useRef, useState } from "react";
import { demoSrc } from "./links";

/**
 * A story running in the shared WASM gallery, inside a window frame.
 *
 * The frame is sized from the height the story reports in `catalog.json`, which
 * the gallery measures from its own laid-out bounds. That is what keeps a
 * three-chip row from sitting in the same tall box as a data table.
 *
 * Nothing is fetched until the frame nears the viewport: the whole catalog is
 * one binary, and a visitor reading prose should not pay for it. The pre-render
 * therefore emits no `src` at all — only `data-src`, which the observer
 * promotes. S-06 replaces the placeholder with a poster and adds the toolbar,
 * the per-demo theme override, and the no-WebGPU card.
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
  const [src, setSrc] = useState<string>();

  useEffect(() => {
    const element = frame.current;
    if (!element || src) return;
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
  }, [story, src]);

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
          {src ? (
            <iframe src={src} title={title} loading="lazy" />
          ) : (
            <p className="demo-placeholder">Loading the live demo…</p>
          )}
        </div>
      </div>
      {caption ? <figcaption>{caption}</figcaption> : null}
    </figure>
  );
}
