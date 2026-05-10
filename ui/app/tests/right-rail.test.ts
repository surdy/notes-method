import test from 'node:test';
import assert from 'node:assert/strict';

import type { NoteSummary } from '../src/lib/api.ts';
import {
	buildBacklinksQuery,
	buildOutgoingLinksQuery,
	buildRailMetadata,
	escapeSqlLiteral,
	isDashboardNote
} from '../src/lib/right-rail.ts';

const note: NoteSummary = {
	path: "Customers/O'Brien/Account Info.md",
	title: 'Account Info',
	type: 'account',
	customer: "O'Brien",
	date: '2026-05-10',
	archived: false
};

test('escapeSqlLiteral doubles single quotes for SQL string literals', () => {
	assert.equal(escapeSqlLiteral("Customers/O'Brien/Account Info.md"), "Customers/O''Brien/Account Info.md");
});

test('right rail queries escape note paths before interpolating them', () => {
	assert.equal(
		buildBacklinksQuery(note.path),
		"SELECT source_path, source_title FROM v_backlinks WHERE target_path = 'Customers/O''Brien/Account Info.md' ORDER BY source_title"
	);
	assert.equal(
		buildOutgoingLinksQuery(note.path),
		"SELECT target_path, target FROM v_backlinks WHERE source_path = 'Customers/O''Brien/Account Info.md' ORDER BY target"
	);
});

test('buildRailMetadata merges summary fields with frontmatter tags', () => {
	assert.deepEqual(
		buildRailMetadata(note, {
			type: 'dashboard',
			date: '2026-05-11',
			tags: ['alpha', 42, 'beta']
		}),
		{
			type: 'dashboard',
			customer: "O'Brien",
			date: '2026-05-11',
			tags: ['alpha', 'beta']
		}
	);
});

test('isDashboardNote checks frontmatter type', () => {
	assert.equal(isDashboardNote({ type: 'dashboard' }), true);
	assert.equal(isDashboardNote({ type: 'meeting' }), false);
	assert.equal(isDashboardNote(null), false);
});
