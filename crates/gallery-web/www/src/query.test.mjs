import assert from 'node:assert/strict';
import test from 'node:test';
import { parseEmbedOptions } from './query.js';

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
