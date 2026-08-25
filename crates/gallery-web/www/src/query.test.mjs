import assert from 'node:assert/strict';
import test from 'node:test';
import {
  parseEmbedOptions,
  parseMotion,
  parseVariant,
  parseVariantMessage,
  parseStatusMessage,
  parseThemeEvent,
  parseThemeMessage,
  parseWheelMessage,
  statusMessage,
  themeMessage,
  wheelMessage,
} from './query.js';

test('parses a story and explicit dark theme', () => {
  assert.deepEqual(parseEmbedOptions('?story=streaming-text&theme=dark'), {
    story: 'streaming-text',
    theme: 'dark',
    motion: undefined,
    variant: undefined,
  });
});

test('parses explicit light theme', () => {
  assert.deepEqual(parseEmbedOptions('?story=thinking&theme=light'), {
    story: 'thinking',
    theme: 'light',
    motion: undefined,
    variant: undefined,
  });
});

test('parses the explicit contrast review theme', () => {
  assert.deepEqual(parseEmbedOptions('?story=loading&theme=contrast'), {
    story: 'loading',
    theme: 'contrast',
    motion: undefined,
    variant: undefined,
  });
});

test('omits empty story and lets system theme decide', () => {
  assert.deepEqual(parseEmbedOptions('?story=&theme=system'), {
    story: undefined,
    theme: undefined,
    motion: undefined,
    variant: undefined,
  });
});

test('parses a bundled theme the host has no list for', () => {
  // themes/ drives the registry, so the host must not gatekeep names.
  assert.deepEqual(parseEmbedOptions('?story=approval&theme=graphite'), {
    story: 'approval',
    theme: 'graphite',
    motion: undefined,
    variant: undefined,
  });
});

test('accepts any well-formed theme name and rejects malformed ones', () => {
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'light' }), 'light');
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'dark' }), 'dark');
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'contrast' }), 'contrast');
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'tokyo-night' }), 'tokyo-night');
  // The gallery rejects names it does not know; the host only checks shape.
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'Neon!' }), undefined);
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: '-leading-dash' }), undefined);
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'system' }), undefined);
  assert.equal(parseThemeMessage({ type: 'other', theme: 'dark' }), undefined);
});

test('theme events require the expected parent and exact origin', () => {
  const parent = {};
  assert.equal(parseThemeEvent({ source: parent, origin: 'https://site.test', data: themeMessage('contrast') }, parent, 'https://site.test'), 'contrast');
  assert.equal(parseThemeEvent({ source: {}, origin: 'https://site.test', data: themeMessage('dark') }, parent, 'https://site.test'), undefined);
  assert.equal(parseThemeEvent({ source: parent, origin: 'https://evil.test', data: themeMessage('dark') }, parent, 'https://site.test'), undefined);
  assert.deepEqual(themeMessage('light'), { type: 'gpui-ai-theme', theme: 'light' });
});

test('an embed reports both outcomes, and says which story it is', () => {
  assert.deepEqual(statusMessage('orbs', 'ready'), {
    type: 'gpui-ai-status',
    story: 'orbs',
    state: 'ready',
  });
  assert.deepEqual(parseStatusMessage(statusMessage('orbs', 'ready')), {
    story: 'orbs',
    state: 'ready',
  });
  // Failure is reported too. A host told only about success leaves its window
  // saying "Starting" over an example that has already drawn the reason it
  // will not run.
  assert.deepEqual(parseStatusMessage(statusMessage('orbs', 'failed')), {
    story: 'orbs',
    state: 'failed',
  });
});

test('an embed says when it takes the wheel and when it gives it back', () => {
  assert.deepEqual(wheelMessage('chat', true), {
    type: 'gpui-ai-wheel',
    story: 'chat',
    captured: true,
  });
  assert.deepEqual(parseWheelMessage(wheelMessage('chat', true)), { story: 'chat', captured: true });
  assert.deepEqual(parseWheelMessage(wheelMessage('chat', false)), {
    story: 'chat',
    captured: false,
  });
});

test('a wheel message that is not one is not mistaken for one', () => {
  for (const data of [
    undefined,
    null,
    {},
    { type: 'gpui-ai-status', story: 'chat', state: 'ready' },
    // `captured` decides who the wheel belongs to, so anything but a boolean
    // is a question this cannot answer.
    { type: 'gpui-ai-wheel', story: 'chat' },
    { type: 'gpui-ai-wheel', story: 'chat', captured: 'yes' },
    { type: 'gpui-ai-wheel', captured: true },
  ]) {
    assert.equal(parseWheelMessage(data), undefined, `${JSON.stringify(data)} must not parse`);
  }
});

test('a status message that is not one is not mistaken for one', () => {
  for (const data of [
    undefined,
    null,
    {},
    { type: 'gpui-ai-theme', theme: 'dark' },
    // A state nobody defined must not be passed through as if it were.
    { type: 'gpui-ai-status', story: 'orbs', state: 'running' },
    { type: 'gpui-ai-status', story: 'orbs' },
    // The story identifies which frame answered, so it has to be a string.
    { type: 'gpui-ai-status', state: 'ready' },
    { type: 'gpui-ai-status', story: 7, state: 'ready' },
  ]) {
    assert.equal(parseStatusMessage(data), undefined, `${JSON.stringify(data)} must not parse`);
  }
});

test('the address can pin the motion preference, or leave it to the machine', () => {
  // Pinned either way, so a link can show a reader what reduced motion does
  // without them having to change a system setting to find out.
  assert.equal(parseMotion('reduced'), true);
  assert.equal(parseMotion('full'), false);
  // Anything else means "ask the machine", which is the default and the only
  // answer that follows someone who changes their mind.
  assert.equal(parseMotion(null), undefined);
  assert.equal(parseMotion(''), undefined);
  assert.equal(parseMotion('yes'), undefined);
  assert.equal(parseMotion('REDUCED'), undefined);

  assert.deepEqual(parseEmbedOptions('?story=orbs&motion=reduced'), {
    story: 'orbs',
    theme: undefined,
    motion: true,
    variant: undefined,
  });
});

test('the address can name the state a story opens in', () => {
  // The gallery owns the list of states a story has, so the host only checks
  // the shape: gatekeeping a list it does not own is how the two drift apart.
  assert.equal(parseVariant('welcome'), 'welcome');
  assert.equal(parseVariant('populated'), 'populated');
  assert.equal(parseVariant(''), undefined);
  assert.equal(parseVariant(null), undefined);
  assert.equal(parseVariant('Welcome!'), undefined);
  assert.equal(parseVariant('-leading'), undefined);

  assert.deepEqual(parseEmbedOptions('?story=chat&variant=welcome'), {
    story: 'chat',
    theme: undefined,
    motion: undefined,
    variant: 'welcome',
  });
});

test('a story tells the page which state it is showing', () => {
  assert.deepEqual(parseVariantMessage({ type: 'gpui-ai-variant', story: 'chat', variant: 'welcome' }), {
    story: 'chat',
    variant: 'welcome',
  });
  // Null is the answer for a story with no states at all, and is not a
  // malformed message.
  assert.deepEqual(parseVariantMessage({ type: 'gpui-ai-variant', story: 'orbs', variant: null }), {
    story: 'orbs',
    variant: null,
  });
  assert.equal(parseVariantMessage({ type: 'gpui-ai-variant', story: 'chat', variant: 'Nope!' }), undefined);
  assert.equal(parseVariantMessage({ type: 'other', story: 'chat', variant: 'welcome' }), undefined);
});
