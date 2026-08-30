import assert from 'node:assert/strict';
import test from 'node:test';
import { shareTheVariant } from './variant.js';

// The failure these pin down cost a CI run and reproduced one round in five in
// a real browser: a demo opened at ?variant=populated, the page set a different
// state, and a quarter of a second later the address put its own state back.
// The address had never actually been applied — populated is where that story
// opens anyway — so the applier was still trying, and won. Held still here,
// poll by poll, where the browser gate can only sample one timing.

/**
 * A story that registers its switcher as it draws, and not before.
 *
 * The lag is the point: `set_story_variant` reaches the story immediately, but
 * `story_variant` reads the switcher registered by the last frame, so a state
 * just applied is only reported a redraw later.
 */
function stubStory(states = ['populated', 'empty']) {
  const asked = [];
  let current;
  let reported;
  return {
    asked,
    /** A frame. The switcher registers, reporting the state the story is in. */
    draw() {
      if (!states.length) return;
      current ??= states[0];
      reported = current;
    },
    wasm: {
      set_story_variant(id) {
        asked.push(id);
        if (current === undefined || !states.includes(id)) return false;
        current = id;
        return true;
      },
      story_variant: () => reported,
    },
  };
}

/** A window whose interval is driven by hand, and a host that records posts. */
function stubWindow() {
  const posted = [];
  let tick;
  const win = {
    parent: { postMessage: (data) => posted.push(data) },
    location: { origin: 'https://gallery.test' },
    setInterval: (callback) => {
      tick = callback;
      return 7;
    },
    clearInterval: () => {
      tick = undefined;
    },
  };
  return { win, posted, poll: () => tick?.() };
}

test('the address waits for a switcher to exist, then applies once', () => {
  const story = stubStory();
  const { win, poll } = stubWindow();
  shareTheVariant('records-table', 'empty', story.wasm, win);

  assert.deepEqual(story.asked, ['empty'], 'asking early is free, and lands on nobody');
  assert.equal(story.wasm.story_variant(), undefined, 'a story that has not drawn reports nothing');

  story.draw();
  poll();
  assert.deepEqual(story.asked, ['empty', 'empty'], 'the story has drawn, so this one lands');

  poll();
  poll();
  assert.deepEqual(story.asked, ['empty', 'empty'], 'the address gets one chance, not four a second');
});

test('a state set deliberately outranks the address', () => {
  // The regression. The address names the state this story opens in anyway, so
  // it is never applied successfully and the applier keeps trying — and the
  // page sets a different state in the gap between two polls.
  const story = stubStory();
  const { win, poll } = stubWindow();
  const addressIsSpent = shareTheVariant('records-table', 'populated', story.wasm, win);
  story.draw();

  addressIsSpent();
  story.wasm.set_story_variant('empty');
  poll();
  poll();

  assert.deepEqual(
    story.asked,
    ['populated', 'empty'],
    'the address must not be applied over a state chosen after it was read',
  );
});

test('an address naming nothing asks for nothing', () => {
  const story = stubStory();
  const { win, poll } = stubWindow();
  shareTheVariant('records-table', undefined, story.wasm, win);

  story.draw();
  poll();
  assert.deepEqual(story.asked, [], 'no state was named, so none is imposed');
});

test('the address lapses when no switcher ever registers', () => {
  // A story with no states never registers one, so nothing but the deadline
  // can end the asking — and without it a demo would call into WebAssembly
  // four times a second for the life of the page.
  const story = stubStory([]);
  const { win, poll } = stubWindow();
  shareTheVariant('orbs', 'populated', story.wasm, win, 0);

  poll();
  const asked = story.asked.length;
  poll();
  poll();
  assert.equal(story.asked.length, asked, 'a lapsed address stops asking');
});

test('every change reaches the host, and nothing else does', () => {
  const story = stubStory();
  const { win, posted, poll } = stubWindow();
  shareTheVariant('records-table', undefined, story.wasm, win);

  story.draw();
  poll();
  poll();
  story.wasm.set_story_variant('empty');
  poll();
  story.draw();
  poll();

  assert.deepEqual(
    posted.map(({ variant }) => variant),
    [null, 'populated', 'empty'],
    'one message per change, including the opening "no state yet"',
  );
  assert.deepEqual(
    posted.map(({ type, story: named }) => `${type}:${named}`),
    ['gpui-ai-variant:records-table', 'gpui-ai-variant:records-table', 'gpui-ai-variant:records-table'],
  );
});
