import test from 'node:test';
import assert from 'node:assert/strict';

import { findSqlBlocks, isNotesmithSqlFenceInfo } from '../src/lib/editor/sql-blocks-helpers.ts';

test('isNotesmithSqlFenceInfo accepts notesmith fences with or without explicit sql', () => {
	assert.equal(isNotesmithSqlFenceInfo('notesmith'), true);
	assert.equal(isNotesmithSqlFenceInfo('notesmith sql'), true);
	assert.equal(isNotesmithSqlFenceInfo('notesmith   sql'), true);
	assert.equal(isNotesmithSqlFenceInfo('sql'), false);
	assert.equal(isNotesmithSqlFenceInfo('notesmith javascript'), false);
});

test('findSqlBlocks extracts SQL from notesmith fences and ignores other code blocks', () => {
	const doc = [
		'# Dashboard',
		'```notesmith sql',
		'select title, status',
		'from v_tasks',
		'```',
		'```ts',
		'console.log("ignore me");',
		'```',
		'```notesmith',
		'select path from v_notes',
		'```'
	].join('\n');

	const blocks = findSqlBlocks(doc);

	assert.deepEqual(
		blocks.map((block) => block.sql),
		['select title, status\nfrom v_tasks', 'select path from v_notes']
	);
	assert.equal(blocks[0]?.blockEnd, doc.indexOf('```ts') - 1);
	assert.equal(blocks[1]?.blockEnd, doc.length);
});
