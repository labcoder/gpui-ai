import { Demo } from "./Demo";
import { href } from "./links";

/**
 * The height the decorations story measures at the demo width.
 *
 * Not read from the catalog: this story is deliberately absent from it,
 * because it documents no component. The number is the one the gallery's own
 * height test reports, and the demo's measured report refines it after the
 * first frame like every other demo on the site.
 */
const DECORATIONS_HEIGHT = 420;

/**
 * What each decoration in the demo is, for readers who cannot run it.
 *
 * The demo is a switcher; without WebGPU it is a still frame of whichever
 * state it opened on, so the four are also written down.
 */
const DECORATIONS = [
  {
    name: "Cross-hatch",
    how: "One GPUI pattern fill",
    note: "No per-frame cost, no image, resolution-independent. The whole effect is a background.",
  },
  {
    name: "Halftone",
    how: "A grid of dots on the motion channel",
    note: "Ninety-six dots whose size follows a wave. It stops when the panel scrolls out of view.",
  },
  {
    name: "Ripple",
    how: "Rings driven by a press",
    note: "The library eases the value; the rings are the application's own drawing.",
  },
  {
    name: "Veil",
    how: "A gradient over the content",
    note: "The over layer rather than the under one, passing every click through to what it covers.",
  },
] as const;

/**
 * Extensions: the parts of gpui-ai that are not components.
 *
 * A component has an API, a page, and a name you can look up. What is on this
 * page does not — it is a slot, a channel, and what an application decides to
 * put in them. Filing that under Components would say it was one more thing to
 * choose between, when it is a way of changing everything already chosen.
 */
export function ExtensionsPage() {
  return (
    <div className="shell">
      <h1>Extensions</h1>
      <p className="lede">
        Not components. These are the points where an application reaches into
        the library and changes what it looks like — a slot to paint into, a
        motion channel to drive it from. gpui-ai ships none of the effects
        below: every one is written in the gallery, the way you would write
        your own.
      </p>

      <section aria-labelledby="decorations">
        <h2 id="decorations">Decorations</h2>
        <p className="lede">
          Every framed component takes two layers: one under its content and
          over its own background, one over the content. Both are clipped to
          the component's shape, and neither takes any part in its layout, so a
          decoration cannot move the thing it decorates.
        </p>

        <Demo
          story="decorations"
          title="Decorations — gpui-ai"
          height={DECORATIONS_HEIGHT}
        />

        <ul className="chips">
          {DECORATIONS.map((decoration) => (
            <li className="chip" key={decoration.name}>
              {decoration.name}
            </li>
          ))}
        </ul>

        <dl className="extension-notes">
          {DECORATIONS.map((decoration) => (
            <div key={decoration.name}>
              <dt>
                {decoration.name}
                <span className="how">{decoration.how}</span>
              </dt>
              <dd>{decoration.note}</dd>
            </div>
          ))}
        </dl>
      </section>

      <section aria-labelledby="motion">
        <h2 id="motion">Motion that stops itself</h2>
        <p className="lede">
          An animated decoration goes through the library's own channel rather
          than a clock of its own. It costs nothing while its panel is scrolled
          away, and it holds still for a reader who has asked for less motion —
          without the application arranging either. That is the whole argument
          for the channel existing: an effect on a private timer would do
          neither.
        </p>
      </section>

      <section aria-labelledby="reach">
        <h2 id="reach">How far this reaches</h2>
        <p className="lede">
          A layer is an element, so anything that can be one can be a
          decoration: a patterned background, an image — animated GIF and WebP
          included — a canvas painting arbitrary geometry, or a webview. GPUI
          has no hook for injecting a shader into its own pipeline, but an
          application can render frames in its own offscreen context and hand
          them over as an image, which is the same thing by a longer road.
        </p>
        <p>
          <a href={href("/docs/")}>The documentation</a> covers what an
          application owns and what the library does.
        </p>
      </section>
    </div>
  );
}
