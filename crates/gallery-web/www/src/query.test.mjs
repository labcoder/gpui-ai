import assert from 'node:assert/strict';
import test from 'node:test';
import { parseEmbedOptions, parseThemeEvent, parseThemeMessage, themeMessage } from './query.js';

test('parses a story and explicit dark theme', () => {
  assert.deepEqual(parseEmbedOptions('?story=streaming-text&theme=dark'), {
    story: 'streaming-text',
    theme: 'dark',
  });
});

test('parses explicit light theme', () => {
  assert.deepEqual(parseEmbedOptions('?story=thinking&theme=light'), {
    story: 'thinking',
    theme: 'light',
  });
});

test('parses the explicit contrast review theme', () => {
  assert.deepEqual(parseEmbedOptions('?story=loading&theme=contrast'), {
    story: 'loading',
    theme: 'contrast',
  });
});

test('omits empty story and lets system theme decide', () => {
  assert.deepEqual(parseEmbedOptions('?story=&theme=system'), {
    story: undefined,
    theme: undefined,
  });
});

test('accepts all named live theme messages and rejects legacy noise', () => {
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'light' }), 'light');
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'dark' }), 'dark');
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'contrast' }), 'contrast');
  assert.equal(parseThemeMessage({ type: 'gpui-ai-theme', theme: 'neon' }), undefined);
  assert.equal(parseThemeMessage({ type: 'other', theme: 'dark' }), undefined);
});

test('theme events require the expected parent and exact origin', () => {
  const parent = {};
  assert.equal(parseThemeEvent({ source: parent, origin: 'https://site.test', data: themeMessage('contrast') }, parent, 'https://site.test'), 'contrast');
  assert.equal(parseThemeEvent({ source: {}, origin: 'https://site.test', data: themeMessage('dark') }, parent, 'https://site.test'), undefined);
  assert.equal(parseThemeEvent({ source: parent, origin: 'https://evil.test', data: themeMessage('dark') }, parent, 'https://site.test'), undefined);
  assert.deepEqual(themeMessage('light'), { type: 'gpui-ai-theme', theme: 'light' });
});
