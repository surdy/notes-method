import test from 'node:test';
import assert from 'node:assert/strict';

import { fuzzyFilter, fuzzyMatch } from '../src/lib/fuzzy.ts';

test('fuzzyMatch returns null when query characters are missing', () => {
  assert.equal(fuzzyMatch('xyz', 'Command Palette'), null);
});

test('fuzzyMatch records ordered character indices', () => {
  const match = fuzzyMatch('cp', 'Command Palette');
  assert.ok(match);
  assert.deepEqual(match.highlights, [0, 8]);
});

test('fuzzyFilter prefers stronger consecutive matches', () => {
  const matches = fuzzyFilter('no', ['Notes', 'Navigation Open'], (item) => item);
  assert.deepEqual(matches.map((match) => match.item), ['Notes', 'Navigation Open']);
  assert.ok(matches[0].score > matches[1].score);
});

test('fuzzyFilter prefers earlier boundary matches over later matches', () => {
  const matches = fuzzyFilter('dn', ["Today's Daily Note", 'Open Today Daily Note'], (item) => item);
  assert.deepEqual(matches.map((match) => match.item), ["Today's Daily Note", 'Open Today Daily Note']);
});
