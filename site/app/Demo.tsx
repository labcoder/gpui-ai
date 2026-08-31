import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import { themeGroups, themes } from "./data";
import { distanceAway, dropSeat, wantSeat } from "./frames.mjs";
import { demoSrc, posterSrc, POSTER_WIDTH } from "./links";
import { tellFrame, useTheme } from "./theme";

/** The override value meaning "whatever the page is showing". */
const FOLLOW = "follow";

/**
 * The tallest a demo may report itself to be.
 *
 * The number arrives from another document and becomes a length on this page,
 * so it is checked before it is used. The tallest story in the catalog is
 * about a thousand pixels and the narrowest frame roughly doubles the worst of
 * them; this is well clear of both and well short of a page nobody can scroll.
 */
const MAX_DEMO_HEIGHT = 6_000;

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
 * Running is not a one-way door. A demo scrolled more than a viewport away
 * goes back to idle and its frame is destroyed, taking the WASM instance and
 * the WebGPU surface with it — a reader who scrolls a long catalog page would
 * otherwise accumulate one of each per demo passed. `frames.mjs` decides which
 * demos may run; this component only says how far away it is and does as it is
 * told. Coming back restarts the story from the beginning, which is what a
 * story that is not being watched is worth.
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
 * Where there is no live render to contradict, a poster stands in — a still of
 * this story captured from the real gallery. Without WebGPU it is the only
 * thing that reader will ever see of the component, so it shows under every
 * theme. Before a run it shows only under Light and Dark, the two themes a
 * poster is captured in: a Nord Frost page would otherwise put up a
 * neutral-grey still and swap it for a blue one a moment later, which reads as
 * a glitch rather than a preview. Under the other 43 themes the window fills
 * with `--demo-surface`, the exact colour the canvas is about to paint.
 *
 * It is unmounted the moment the demo draws. A decoded 900x990 bitmap is about
 * three and a half megabytes of RGBA, and a page holding one per demo would
 * cost more than the frames it was meant to spare.
 *
 * The frame's height starts as the number the gallery measured and becomes the
 * tallest the story reports. Those differ everywhere the frame is narrower
 * than the width the catalog was measured at: a story's height is a function
 * of its width and not a step function — prose rewraps a line at a time — so
 * on a phone, a tablet, or a half-width window the reserved height is too
 * short and the story used to scroll inside its own canvas instead of being
 * shown.
 *
 * The tallest rather than the latest, because a story arrives: a task list
 * grows a row at a time, a search shows three results where it showed none,
 * and a frame that tracked the current height would rise and fall under the
 * reader for as long as the demo ran. Growing once and staying is what the
 * catalog's own numbers already promise — they are measured the same way.
 *
 * The measurement is thrown away when the frame changes width, because that is
 * the one thing that makes a settled answer wrong, and kept across everything
 * else.
 *
 * The reserved number is still what holds the space before anything runs,
 * because it is the only height available then, and it is what a reader with
 * no WebGPU keeps.
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
  variants,
  variant: asked,
  onVariant,
}: {
  readonly story: string;
  readonly title: string;
  readonly height: number;
  readonly caption?: string;
  /**
   * The states this story offers, as a row of buttons above the frame.
   *
   * Given, the demo switches the running story rather than reloading it: the
   * host answers `set_story_variant`, so a seventeen-megabyte WebAssembly
   * instance stays up and the card changes underneath. Omitted, the demo is
   * whatever state the story opens in, which is what most pages want.
   */
  readonly variants?: readonly { readonly id: string; readonly label: string }[] | undefined;
  /** The state to show. Controlled by the page when it has one to name. */
  readonly variant?: string | undefined;
  /** Told when the reader picks a state, so the page's code can follow it. */
  readonly onVariant?: ((id: string) => void) | undefined;
}) {
  const frame = useRef<HTMLDivElement>(null);
  const iframe = useRef<HTMLIFrameElement>(null);
  const [running, setRunning] = useState(false);
  const [reloads, setReloads] = useState(0);
  const [status, setStatus] = useState<"starting" | "ready" | "failed">("starting");
  const [scrolls, setScrolls] = useState(false);
  const [measured, setMeasured] = useState<number>();
  const [growsSmoothly, setGrowsSmoothly] = useState(false);
  const [variant, setVariant] = useState<string>();
  const [wanted, setWanted] = useState<string>();
  const [override, setOverride] = useState<string>(FOLLOW);
  const [linkState, setLinkState] = useState<"idle" | "copied" | "failed">("idle");
  const overrideId = useId();

  // Derived, not stored: what the frame points at is a function of whether it
  // is allowed to run, and keeping a second copy in state is how the two come
  // to disagree.
  const src = running ? demoSrc(story, undefined, wanted) : undefined;

  const site = useTheme();
  const effective = override === FOLLOW ? site.applied : override;
  const painted = themes.find((candidate) => candidate.slug === effective);
  // The two themes posters are captured in. Under anything else a poster is
  // the right shape in the wrong colours, which is worse than no poster at all
  // for the instant before the real thing draws.
  const neutral = effective === "light" || effective === "dark";

  // Every frame the embed replaces starts over. Reload is the case that
  // matters: it swaps the iframe for a new one, and a window that kept saying
  // "ready" would be describing a document that no longer exists.
  useEffect(() => {
    setStatus("starting");
    setScrolls(false);
    // A frame that has gone takes its measurement with it. Keeping the last
    // one would size an empty window from a story that is no longer in it.
    setMeasured(undefined);
  }, [src, reloads]);

  // A story that is still arriving reports a new height every few frames —
  // a chat gains a message, streamed prose wraps one more line — and the
  // frame jumped to each one. Growing the frame over the same span the
  // content took to arrive turns a staircase into a rise.
  //
  // Not on the first measurement, though: that one is the reserved height
  // giving way to the real one, and animating it would make every demo on
  // the page look like it was unfurling. The flag is set a frame after the
  // first height lands, so the first jump is instant and everything after
  // it is smooth, and it resets whenever the frame does.
  const awaitingFirstHeight = measured === undefined;
  useEffect(() => {
    if (awaitingFirstHeight) {
      setGrowsSmoothly(false);
      return;
    }
    const settle = requestAnimationFrame(() => setGrowsSmoothly(true));
    return () => cancelAnimationFrame(settle);
  }, [awaitingFirstHeight]);

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
        height?: unknown;
        variant?: unknown;
      } | null;
      if (message?.story !== story) return;
      if (message.type === "gpui-ai-wheel" && typeof message.captured === "boolean") {
        setScrolls(message.captured);
        return;
      }
      if (message.type === "gpui-ai-variant") {
        // Null means a story with no states, which is most of them.
        setVariant(typeof message.variant === "string" ? message.variant : undefined);
        return;
      }
      if (message.type === "gpui-ai-size" && typeof message.height === "number") {
        // Bounded, because this sets a length on the page: a frame told to be
        // a hundred thousand pixels tall would push the rest of the site off
        // the bottom of the document.
        const reported = Math.round(message.height);
        if (Number.isFinite(reported) && reported > 0 && reported <= MAX_DEMO_HEIGHT) {
          setMeasured((tallest) => Math.max(tallest ?? 0, reported));
        }
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

  // A story laid out at one width has nothing to say about another, and the
  // height it reported is the one thing here that a resize invalidates. Width
  // only: the frame's own height is what this sets, and watching that would be
  // a loop.
  useEffect(() => {
    const element = frame.current;
    if (!element || typeof ResizeObserver !== "function") return;
    let width = element.getBoundingClientRect().width;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const next = entry.contentRect.width;
        if (Math.abs(next - width) < 1) continue;
        width = next;
        setMeasured(undefined);
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

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

  // The state a link asked for, read after mount rather than during render:
  // the pre-render has no address to read, and a frame that differed between
  // the two would be thrown away and rebuilt. The demo does not start until
  // after mount either, so this is known in time to be the state it opens in.
  //
  // Which of the two wins depends on whether the reader can move. With a
  // switcher, the address wins at mount and the page is told, so a shared link
  // opens on its state *and* the code beneath it follows — then the buttons
  // take over. Without one, the page's own state wins, because a page with no
  // switcher is about that state and a stale link should not repaint it as
  // another.
  const switchable = Boolean(variants && variants.length > 0);
  useEffect(() => {
    const fromLink = new URL(window.location.href).searchParams.get("variant");
    if (switchable && fromLink) {
      setWanted(fromLink);
      onVariant?.(fromLink);
      return;
    }
    if (asked) {
      setWanted(asked);
      return;
    }
    if (fromLink) setWanted(fromLink);
    // Once, at mount: after that the buttons and the page own this between
    // them, and re-reading the address would undo a reader's choice.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Switching a story that is already up: the host answers this, and the
  // alternative is rebuilding the whole instance to change one card.
  const show = useCallback(
    (id: string) => {
      setWanted(id);
      onVariant?.(id);
      const running = iframe.current?.contentWindow as
        | (Window & { gpuiAi?: { setVariant?: (id: string) => boolean } })
        | null
        | undefined;
      try {
        running?.gpuiAi?.setVariant?.(id);
      } catch {
        // Same-origin, so this only throws for a frame that is already gone;
        // `wanted` has it, and a frame that starts later opens on it.
      }
    },
    [onVariant],
  );

  // This demo's claim on one of the machine's live frames. Identity has to
  // outlast a render, or the governor would be tracking a new demo every time
  // React re-ran this component.
  const seat = useMemo(() => ({ live: setRunning }), []);

  useEffect(() => {
    const element = frame.current;
    if (!element || webgpu !== true) return;
    if (typeof IntersectionObserver !== "function") {
      // No way to tell where anything is, so this demo always wants a seat and
      // the governor's limit is the only thing bounding the page.
      wantSeat(seat, 0);
      return () => dropSeat(seat);
    }
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          // Intersecting here means within one viewport, because of the margin
          // below — so this is both "start it before it arrives" and "stop it
          // once it is that far behind", in one signal.
          if (entry.isIntersecting) {
            wantSeat(seat, distanceAway(entry.boundingClientRect, window.innerHeight));
          } else {
            dropSeat(seat);
          }
        }
      },
      // One viewport of lead time, so the demo is running by the time it
      // arrives rather than starting when it is already being looked at.
      { rootMargin: "100% 0px" },
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      dropSeat(seat);
    };
  }, [seat, webgpu]);

  // Each frame is told its theme by the component that owns it, and by nothing
  // else. The theme travels as a message rather than in the URL, so a frame
  // already running changes theme without being torn down and rebuilt — and so
  // the address stays the story's own, which is what Pop out and Copy link
  // hand over.
  const tellThisFrame = useCallback(() => {
    if (iframe.current) tellFrame(iframe.current, effective);
  }, [effective]);

  useEffect(tellThisFrame, [tellThisFrame]);

  /**
   * Puts the story back to where it opened.
   *
   * Asks the running demo first. Replacing the frame does the same thing by
   * tearing down a seventeen-megabyte WebAssembly instance and building
   * another one, to reach a state the story gets back to in a frame — so that
   * is the fallback, for a demo that has not started or has failed.
   */
  const reload = useCallback(() => {
    const running = iframe.current?.contentWindow as
      | (Window & { gpuiAi?: { reset?: () => boolean } })
      | null
      | undefined;
    try {
      if (running?.gpuiAi?.reset?.()) {
        setStatus("ready");
        return;
      }
    } catch {
      // Same-origin, so this only throws for a frame that is already gone.
    }
    setReloads((count) => count + 1);
  }, []);

  const copyLink = useCallback(async () => {
    const url = new URL(window.location.href);
    url.hash = "";
    if (override !== FOLLOW) url.searchParams.set("theme", override);
    // The state the reader switched to, which they did inside the canvas where
    // this page cannot see it. Without it a shared link opens the story where
    // the story opens, which is not where the sender was.
    if (variant) url.searchParams.set("variant", variant);
    try {
      await navigator.clipboard.writeText(url.toString());
      setLinkState("copied");
    } catch {
      // A refused clipboard is a real outcome. Saying nothing leaves a visitor
      // pasting whatever was there before and wondering why.
      setLinkState("failed");
    }
    window.setTimeout(() => setLinkState("idle"), 2_400);
  }, [override, variant]);

  // What the row shows as pressed: the reader's choice, then the page's, then
  // whatever the running story reports about itself.
  const showing = wanted ?? variant;

  return (
    <figure className="demo">
      {variants && variants.length > 0 ? (
        <div className="demo-states" role="group" aria-label="States">
          {variants.map((state) => (
            <button
              key={state.id}
              type="button"
              aria-pressed={showing === state.id}
              onClick={() => show(state.id)}
            >
              {state.label}
            </button>
          ))}
        </div>
      ) : null}
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
          {...(growsSmoothly ? { "data-grows-smoothly": "" } : {})}
          data-src={demoSrc(story)}
          {...(webgpu === false ? { "data-poster-only": "" } : {})}
          ref={frame}
          style={{
            // What the story says it is, once it has said so. Until then, and
            // for a reader who will never run it, the measured reservation.
            ["--demo-height" as string]: `${measured ?? height}px`,
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
            <>
              {painted ? (
                <Poster
                  story={story}
                  mode={painted.mode}
                  height={height}
                  alt={`${title}, rendered`}
                />
              ) : null}
              <div className="demo-unavailable" data-webgpu-fallback>
                <strong>This demo needs WebGPU</strong>
                <p>
                  Your browser cannot draw the component, so nothing has been downloaded. Everything
                  else on this page — the code, the events, the source — is the same either way.
                </p>
              </div>
            </>
          ) : src ? (
            <>
              {starting && neutral && painted ? (
                <Poster story={story} mode={painted.mode} height={height} />
              ) : null}
              {/* Told again on load, because a frame that has only just been
                  created has no listener yet — which is exactly the state
                  Reload leaves it in. */}
              <iframe
                key={reloads}
                ref={iframe}
                src={src}
                title={title}
                loading="lazy"
                onLoad={tellThisFrame}
              />
            </>
          ) : (
            <>
              {neutral && painted ? (
                <Poster story={story} mode={painted.mode} height={height} />
              ) : null}
              <p className="demo-idle">Starts when it scrolls into view</p>
            </>
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
          <button type="button" data-specimen-reload onClick={reload} disabled={!running}>
            Reset
          </button>
          {/* Always the effective theme, not just an override. Popped out
              there is no page around the frame to follow, and an embed told
              nothing guesses from the viewer's own light/dark preference — so
              "Follow site" used to open the demo in a theme the site was not
              showing. */}
          <a data-specimen-open href={demoSrc(story, effective, variant)}>
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
 * A still of the story, captured from the gallery at build time.
 *
 * Sized from the same measured height the frame is, and at the width the
 * capture ran at, so it reserves exactly the space the live demo will take and
 * a reader never watches the page jump when one replaces the other.
 *
 * `alt` is given only where the poster is the component: without WebGPU there
 * will never be a live render, so the picture carries the meaning. Standing in
 * for a demo that is about to appear it is decoration, and a screen reader
 * announcing "Chat, rendered" about a placeholder would be describing
 * something that is already gone.
 */
function Poster({
  story,
  mode,
  height,
  alt,
}: {
  readonly story: string;
  readonly mode: string;
  readonly height: number;
  readonly alt?: string;
}) {
  return (
    <img
      className="demo-poster"
      data-demo-poster={story}
      src={posterSrc(story, mode)}
      alt={alt ?? ""}
      width={POSTER_WIDTH}
      height={height}
      decoding="async"
      {...(alt === undefined ? { "aria-hidden": true } : {})}
    />
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
