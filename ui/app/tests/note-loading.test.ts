import test from 'node:test';
import assert from 'node:assert/strict';

import { shouldLoadSelectedNote } from '../src/lib/note-loading.ts';

test('selected note loads when it is neither current nor already loading', () => {
	assert.equal(
		shouldLoadSelectedNote({
			selectedPath: 'Customers/Acme.md',
			currentPath: null,
			loadingPath: null
		}),
		true
	);
});

test('selected note does not reload while the same path is already loading', () => {
	assert.equal(
		shouldLoadSelectedNote({
			selectedPath: 'Customers/Acme.md',
			currentPath: null,
			loadingPath: 'Customers/Acme.md'
		}),
		false
	);
});

test('selected note does not reload after currentPath is set before editor view creation', () => {
	assert.equal(
		shouldLoadSelectedNote({
			selectedPath: 'Customers/Acme.md',
			currentPath: 'Customers/Acme.md',
			loadingPath: 'Customers/Acme.md'
		}),
		false
	);
});

test('previously selected note reloads if another in-flight load destroyed its editor', () => {
	assert.equal(
		shouldLoadSelectedNote({
			selectedPath: 'Customers/Acme.md',
			currentPath: null,
			loadingPath: 'Customers/Globex.md'
		}),
		true
	);
});

test('cleared selection does not trigger a load', () => {
	assert.equal(
		shouldLoadSelectedNote({
			selectedPath: null,
			currentPath: 'Customers/Acme.md',
			loadingPath: null
		}),
		false
	);
});
