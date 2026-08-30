// Which state a multi-state story is showing, and the one chance the address
// gets to choose it.
//
// Its own module so it can be tested: the alternative is exercising it through
// main.js, which starts the gallery on import — and the mistake this guards
// against is a race that only shows up in a browser one run in five.

import { variantMessage } from './query.js';

/**
 * How often the running story is asked which state it is showing.
 *
 * Polled rather than pushed, for the same reason the size is: a reader changes
 * this a handful of times and a callback per layout would be a message a frame.
 *
 * On a timer rather than an animation frame, and unlike the size this one never
 * stops — a reader can press the switcher at any moment, so there is no quiet
 * period after which the answer cannot change. Four times a second is far more
 * than a press needs; sixty would be a call into WebAssembly every frame for
 * the life of the page, on every demo at once.
 */
const VARIANT_POLL_MS = 250;

/**
 * Opens the story in the state the address asked for, and reports every change.
 *
 * A story draws its own switcher inside the canvas, so the page around it
 * cannot see which state is showing — and until now a link to a demo always
 * opened where the story opens rather than where the sender was. The address
 * can now name one, and the page is told whenever it changes, whether that was
 * the address or the reader pressing the switcher.
 *
 * Returns the way to spend the address early. Anyone setting a state
 * deliberately outranks it, because the address was read once, before the story
 * existed; without this a state set between two polls is undone by the next.
 *
 * @param {string | undefined} story
 * @param {string | undefined} wanted the state the address named, if any
 * @param {{ set_story_variant: (id: string) => boolean, story_variant: () => string | undefined }} wasm
 * @param {Window} win
 * @param {number} giveUpAfterMs how long a story gets to draw before the address lapses
 * @returns {() => void}
 */
export function shareTheVariant(story, wanted, wasm, win = window, giveUpAfterMs = 8000) {
  // A switcher registers itself as it draws, and `run` returns before the first
  // frame — so asking now would be asking nobody. Kept up until the story has
  // drawn, then dropped, because the address names the state a demo opens in
  // and not the state it stays in: a story that has drawn has also been on
  // screen, and applying the address after that overwrites a state chosen more
  // recently than the address was read. A story with no states never registers
  // a switcher, so for that one only the timeout ends the asking.
  let asking = Boolean(wanted);
  const startedAt = performance.now();
  let last;
  let polling;

  const check = () => {
    let variant;
    try {
      if (asking) wasm.set_story_variant(wanted);
      variant = wasm.story_variant() ?? null;
      // Any state to report means the switcher has registered, which is the
      // one moment the address gets. The state just applied is only reported a
      // frame later, so this reads the state before it — either way the story
      // has drawn, which is all this asks.
      if (asking && (variant !== null || performance.now() - startedAt > giveUpAfterMs)) {
        asking = false;
      }
    } catch {
      // A module that has gone away has no state to report.
      win.clearInterval(polling);
      return;
    }
    if (variant !== last) {
      last = variant;
      if (win.parent !== win) {
        win.parent.postMessage(variantMessage(story, variant), win.location.origin);
      }
    }
  };

  polling = win.setInterval(check, VARIANT_POLL_MS);
  // Asked straight away too, so a link naming a state does not wait a beat.
  check();
  return () => {
    asking = false;
  };
}
