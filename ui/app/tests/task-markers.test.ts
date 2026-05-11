import test from 'node:test';
import assert from 'node:assert/strict';

import { taskMarkerToStatus } from '../src/lib/editor/task-markers.ts';

test('taskMarkerToStatus supports all Notesmith task markers', () => {
	assert.equal(taskMarkerToStatus(' '), 'todo');
	assert.equal(taskMarkerToStatus('/'), 'in_progress');
	assert.equal(taskMarkerToStatus('b'), 'blocked');
	assert.equal(taskMarkerToStatus('w'), 'waiting');
	assert.equal(taskMarkerToStatus('h'), 'on_hold');
	assert.equal(taskMarkerToStatus('x'), 'done');
	assert.equal(taskMarkerToStatus('X'), 'done');
	assert.equal(taskMarkerToStatus('-'), 'cancelled');
});

test('taskMarkerToStatus rejects unknown task markers', () => {
	assert.equal(taskMarkerToStatus('?'), null);
	assert.equal(taskMarkerToStatus('z'), null);
});
