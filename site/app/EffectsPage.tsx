import { Demo } from "./Demo";
import { decorations, effects } from "./data";
import { href } from "./links";

/**
 * Everything an application paints into a component rather than around it.
 *
 * This used to be one page called Extensions, with all thirteen decorations
 * behind a single switcher and four of them described in a hand-written list
 * that had gone stale. Nothing in the catalogue, the home page or the search
 * index pointed at it: you had to already know the word.
 *
 * So it is a section now, and each decoration is a page — generated from the
 * gallery's own list, so one added in Rust becomes a page here without anyone
 * writing it down twice.
 */
export function EffectsPage() {
  return (
    <div className="shell">
      <p className="eyebrow">Effects</p>
      <h1>What you paint into a component</h1>
      <p className="lede">
        Every framed component carries two slots — one under its content and one over it — and a
        motion channel to drive them from. gpui-ai ships none of these effects: they are the
        gallery&rsquo;s, written the way an application would write them, to show what the slot is
        for.
      </p>

      <Demo
        story={effects.story}
        title={effects.windowTitle}
        height={effects.height}
        caption="The same card under each decoration. The switcher inside the frame is the story's own — and every state it offers has a page here, with the Rust that makes it."
      />

      <section aria-labelledby="decorations">
        <h2 id="decorations">Decorations</h2>
        <p className="lede">
          Thirteen, from a photograph laid straight under the words to a field of colour with light
          sweeping over the frame.
        </p>
        <nav className="doc-index" aria-label="Decorations">
          {decorations.map((decoration) => (
            <a key={decoration.slug} href={href(`/effects/${decoration.slug}/`)}>
              <strong>{decoration.label}</strong>
              <span>{decoration.note}</span>
            </a>
          ))}
        </nav>
      </section>

      <section aria-labelledby="rules">
        <h2 id="rules">Two rules, and why they are rules</h2>
        <div className="caveat">
          <p>
            <strong>A layer that reaches the edge carries the frame&rsquo;s radius itself.</strong>{" "}
            GPUI&rsquo;s content mask is a rectangle, so nothing can clip a subtree to a corner on
            your behalf. <code>decoration::frame_radius(cx)</code> is the radius to round by. A
            layer that never reaches a corner — scattered dots, a ring in the middle — needs
            nothing. Note that <code>ObjectFit::Cover</code> hands a sprite bounds larger than the
            element, which puts those radii on corners that are off screen: a covered photograph
            has to be cropped to the frame rather than fitted into it.
          </p>
          <p>
            <strong>A decoration never changes the size of what it decorates.</strong> The frame
            measures its content, and both layers are painted into the space that measurement
            produced. The over layer passes pointer input through, so a veil never costs the
            component a click.
          </p>
        </div>
      </section>

      <section aria-labelledby="motion">
        <h2 id="motion">Motion</h2>
        <p>
          <code>decoration::animated</code> drives a layer from a looping 0…1 and stops it when the
          frame scrolls out of view or the reader has asked for less motion — so an
          application&rsquo;s own effect answers to the same preference every animation in this
          library does, without the application arranging it.{" "}
          <code>decoration::toward</code> eases towards a value the application already has, for an
          effect driven by a press or a pointer rather than a clock.
        </p>
        <p>
          <a href={href("/guides/accessibility-and-motion/")}>
            How the motion preference is resolved
          </a>
        </p>
      </section>
    </div>
  );
}
