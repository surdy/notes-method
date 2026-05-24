import test from 'node:test';
import assert from 'node:assert/strict';

import type { NoteSummary } from '$lib/api';
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
	tags: ['account']
};

test('escapeSqlLiteral doubles single quotes for SQL string literals', () => {
	assert.equal(escapeSqlLiteral("Customers/O'Brien/Account Info.md"), "Customers/O''Brien/Account Info.md");
});

test('right rail queries escape note paths before interpolating them', () => {
	assert.equal(
		buildBacklinksQuery(note.path),
		"SELECT DISTINCT b.source_path, COALESCE(b.source_title, b.source_path) AS source_title FROM v_backlinks b WHERE b.target_path = 'Account Info' ORDER BY source_title"
	);
	assert.equal(
		buildBacklinksQuery("Customers/O'Brien/Bob's Plan.md"),
		"SELECT DISTINCT b.source_path, COALESCE(b.source_title, b.source_path) AS source_title FROM v_backlinks b WHERE b.target_path = 'Bob''s Plan' ORDER BY source_title"
	);
	assert.equal(
		buildOutgoingLinksQuery(note.path),
		"SELECT DISTINCT b.target_path, COALESCE(n.title, b.target_path) AS target FROM v_backlinks b LEFT JOIN v_notes n ON n.path = b.target_path WHERE b.source_path = 'Customers/O''Brien/Account Info.md' ORDER BY target"
	);
});

test('buildRailMetadata shows all public fields generically', () => {
	assert.deepEqual(
		buildRailMetadata(note, {
			type: 'dashboard',
			date: '2026-05-11',
			tags: ['alpha', 42, 'beta']
		}),
		{
			type: 'dashboard',
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
