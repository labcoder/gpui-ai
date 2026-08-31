import { Demo } from "./Demo";
import { hero } from "./data";
import { href } from "./links";

/**
 * The compositions, rather than the parts.
 *
 * Three gallery stories that are deliberately not components and had nowhere
 * to be: the guided conversation the home page runs, a docked workspace that
 * proves gpui-ai composes with gpui-component's own layout, and the instrument
 * the motion policy is tuned with. They were reachable only by typing a story
 * slug into the embed's query string.
 *
 * Heights are stated here rather than measured, because none of these is in
 * the component index and nothing measures them; the demo refines each one
 * from its own first frame the way every other demo on the site does.
 */
const COMPOSITIONS = [
  {
    story: "dock-composition",
    title: "Dock composition",
    height: 620,
    summary:
      "A workspace: gpui-component's dock carrying gpui-ai panels, resized and rearranged.",
    note: "The proof that these components are ordinary GPUI elements. Nothing here knows it is inside a dock, and the dock does not know what it is holding.",
  },
  {
    story: "motion-lab",
    title: "Motion lab",
    height: 560,
    summary: "Every motion token in the library, on one clock, side by side.",
    note: "An instrument rather than a demo. It exists so a change to a spring can be seen against every other spring instead of on its own.",
  },
  {
    story: "themes-trio",
    title: "Themes side by side",
    height: 420,
    summary: "One component under three themes at once, painted from the same snapshot.",
    note: "What a theme actually changes, and what it does not. The layout is identical in all three; only the tokens differ.",
  },
] as const;

export function ShowcasePage() {
  return (
    <div className="shell">
      <p className="eyebrow">Showcase</p>
      <h1>Whole compositions</h1>
      <p className="lede">
        A component page shows one surface on its own, which is the right way to judge it and the
        wrong way to imagine an application. These are the assembled things: a conversation that
        runs itself, a docked workspace, and the instrument the motion policy is tuned with.
      </p>

      <section aria-labelledby="guided">
        <h2 id="guided">A conversation, running</h2>
        <p className="lede">
          Ask it something or take a suggestion, and watch the tool calls and the streamed answer
          arrive. Every surface in it is a component from the catalogue.
        </p>
        <Demo story={hero.slug} title={hero.windowTitle} height={hero.height ?? 520} />
      </section>

      {COMPOSITIONS.map((composition) => (
        <section key={composition.story} aria-labelledby={composition.story}>
          <h2 id={composition.story}>{composition.title}</h2>
          <p className="lede">{composition.summary}</p>
          <Demo
            story={composition.story}
            title={`${composition.title} — gpui-ai`}
            height={composition.height}
            caption={composition.note}
          />
        </section>
      ))}

      <section aria-labelledby="build-one">
        <h2 id="build-one">Build one</h2>
        <p>
          Everything above is in the gallery&rsquo;s own source, and the smallest complete
          application is eighty lines. <a href={href("/start/")}>Start</a> has the dependency and
          the window; <a href={href("/components/")}>Components</a> has the parts;{" "}
          <a href={href("/effects/")}>Effects</a> has what you paint into them.
        </p>
      </section>
    </div>
  );
}
