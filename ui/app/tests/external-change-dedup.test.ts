import test from 'node:test';
import assert from 'node:assert/strict';

import { createExternalChangeDedup } from '../src/lib/editor/external-change-dedup.ts';

function dedupForHash(initial: string | null) {
	let hash = initial;
	const dedup = createExternalChangeDedup(() => hash);
	return {
		dedup,
		setHash(next: string | null) {
			hash = next;
		}
	};
}

test('matching hash on currentHash suppresses the event after a save settles', () => {
	const { dedup, setHash } = dedupForHash('A');

	const echoBefore = dedup.handle({ path: 'a.md', hash: 'A' }, true);
	assert.deepEqual(echoBefore, { kind: 'suppress' });

	dedup.beginSave();
	setHash('B');
	const drained = dedup.recordSavedHash('B');
	assert.deepEqual(drained, []);

	const echoAfter = dedup.handle({ path: 'a.md', hash: 'B' }, false);
	assert.deepEqual(echoAfter, { kind: 'suppress' });
});

test('event arriving before the save response is buffered and suppressed once the response hash matches', () => {
	const { dedup, setHash } = dedupForHash('A');

	dedup.beginSave();

	// SSE arrives before the HTTP save response — we don't yet know the new hash.
	const racing = dedup.handle({ path: 'a.md', hash: 'B' }, true);
	assert.deepEqual(racing, { kind: 'buffered' });

	setHash('B');
	const drained = dedup.recordSavedHash('B');
	assert.deepEqual(drained, [{ kind: 'suppress' }]);
});

test('events buffered during a save are surfaced as conflicts if their hash does not match the save response', () => {
	const { dedup, setHash } = dedupForHash('A');

	dedup.beginSave();

	// Somebody else wrote hash X to disk while we were saving B.
	const racing = dedup.handle({ path: 'a.md', hash: 'X' }, true);
	assert.deepEqual(racing, { kind: 'buffered' });

	setHash('B');
	const drained = dedup.recordSavedHash('B');
	// After a successful save the editor is no longer dirty for what we just
	// saved, so the non-matching event should ask for a reload, not a banner.
	assert.deepEqual(drained, [{ kind: 'reload' }]);
});

test('a delayed, coalesced echo with a previously-saved hash is still suppressed', () => {
	const { dedup, setHash } = dedupForHash('A');

	dedup.beginSave();
	setHash('B');
	dedup.recordSavedHash('B');

	// User keeps typing; a second save lands.
	dedup.beginSave();
	setHash('C');
	dedup.recordSavedHash('C');

	// Watcher belatedly fires a duplicate event for save B.
	const lateEcho = dedup.handle({ path: 'a.md', hash: 'B' }, true);
	assert.deepEqual(lateEcho, { kind: 'suppress' });
});

test('a truly external write with an unknown hash on a dirty editor surfaces a conflict', () => {
	const { dedup } = dedupForHash('A');

	const outcome = dedup.handle({ path: 'a.md', hash: 'Z' }, true);
	assert.deepEqual(outcome, { kind: 'conflict' });
});

test('a truly external write with an unknown hash on a clean editor triggers a reload', () => {
	const { dedup } = dedupForHash('A');

	const outcome = dedup.handle({ path: 'a.md', hash: 'Z' }, false);
	assert.deepEqual(outcome, { kind: 'reload' });
});

test('events without a hash fall back to dirty-state classification', () => {
	const { dedup } = dedupForHash('A');

	assert.deepEqual(dedup.handle({ path: 'a.md', hash: undefined }, true), {
		kind: 'conflict'
	});
	assert.deepEqual(dedup.handle({ path: 'a.md', hash: undefined }, false), {
		kind: 'reload'
	});
});

test('rememberHash lets a side-channel write (e.g. task toggle) suppress its watcher echo', () => {
	const { dedup, setHash } = dedupForHash('A');

	// Caller performed a task toggle; server returned hash T.
	dedup.rememberHash('T');
	setHash('T');

	const echo = dedup.handle({ path: 'a.md', hash: 'T' }, false);
	assert.deepEqual(echo, { kind: 'suppress' });
});

test('the hash history is bounded so stale hashes do not block real warnings forever', () => {
	const { dedup } = dedupForHash(null);
	const capacity = 4;
	const small = createExternalChangeDedup(() => null, { historyCapacity: capacity });

	small.rememberHash('h1');
	small.rememberHash('h2');
	small.rememberHash('h3');
	small.rememberHash('h4');
	small.rememberHash('h5');

	// h1 has been evicted by the ring being bounded to 4 entries.
	assert.deepEqual(small.handle({ path: 'a.md', hash: 'h1' }, true), { kind: 'conflict' });
	assert.deepEqual(small.handle({ path: 'a.md', hash: 'h5' }, true), { kind: 'suppress' });
	// And the original helper using default capacity still works.
	dedup.rememberHash('x');
	assert.deepEqual(dedup.handle({ path: 'a.md', hash: 'x' }, true), { kind: 'suppress' });
});

test('cancelSave drains buffered events and surfaces them as conflicts on a still-dirty editor', () => {
	const { dedup } = dedupForHash('A');

	dedup.beginSave();
	dedup.handle({ path: 'a.md', hash: 'X' }, true);
	dedup.handle({ path: 'a.md', hash: 'Y' }, true);

	const drained = dedup.cancelSave();
	assert.deepEqual(drained, [{ kind: 'conflict' }, { kind: 'conflict' }]);
});

test('reset clears all buffered state', () => {
	const { dedup } = dedupForHash('A');

	dedup.beginSave();
	dedup.handle({ path: 'a.md', hash: 'X' }, true);
	dedup.rememberHash('B');

	dedup.reset();

	// No buffered events should remain — recording a new save returns no outcomes.
	dedup.beginSave();
	assert.deepEqual(dedup.recordSavedHash('Z'), []);
	// B was forgotten by reset; an event with hash B is no longer suppressed.
	assert.deepEqual(dedup.handle({ path: 'a.md', hash: 'B' }, false), { kind: 'reload' });
});
