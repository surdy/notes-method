import test from 'node:test';
import assert from 'node:assert/strict';
import { get } from 'svelte/store';

import type { NoteDetail, NoteSummary, WriteNoteResponse } from '../src/lib/api/notes.ts';
import type { QueuedSave } from '../src/lib/save-queue.ts';

type TimerCallback = () => void | Promise<void>;
type TimerHandle = ReturnType<typeof setTimeout> | number;

function createQueueAdapter(initial: Array<QueuedSave & { id?: number }> = []) {
	let nextId = 0;
	const records = initial.map((record) => ({
		...record,
		id: record.id ?? ++nextId
	}));

	return {
		async enqueue(save: QueuedSave) {
			const existing = records.find((record) => record.vault === save.vault && record.path === save.path);
			if (existing) {
				Object.assign(existing, save, { retryCount: 0 });
				return existing.id;
			}
			const record = { ...save, id: ++nextId };
			records.push(record);
			return record.id;
		},
		async dequeue(id: number) {
			const index = records.findIndex((record) => record.id === id);
			if (index >= 0) {
				records.splice(index, 1);
			}
		},
		async getAll() {
			return [...records].sort((left, right) => left.timestamp - right.timestamp);
		},
		async getCount() {
			return records.length;
		},
		async clear() {
			records.splice(0, records.length);
		},
		async get(id: number) {
			return records.find((record) => record.id === id);
		},
		async update(id: number, changes: Partial<QueuedSave>) {
			const record = records.find((entry) => entry.id === id);
			if (record) {
				Object.assign(record, changes);
			}
		},
		async removeByPath(vault: string, path: string) {
			const index = records.findIndex((record) => record.vault === vault && record.path === path);
			if (index >= 0) {
				records.splice(index, 1);
			}
		},
		records
	};
}

function createTimerHarness() {
	let nextId = 0;
	const timers = new Map<number, { callback: TimerCallback; delay: number }>();

	return {
		schedule(callback: TimerCallback, delay: number) {
			const id = ++nextId;
			timers.set(id, { callback, delay });
			return id;
		},
		cancel(id: TimerHandle) {
			timers.delete(Number(id));
		},
		async runNext(delay?: number) {
			const timer = [...timers.entries()].find(([, value]) => delay === undefined || value.delay === delay);
			assert.ok(timer, `expected a timer${delay === undefined ? '' : ` for ${delay}ms`}`);
			timers.delete(timer[0]);
			await timer[1].callback();
		},
		delays() {
			return [...timers.values()].map((timer) => timer.delay);
		}
	};
}

function createNoteDetail(hash = 'server-hash'): NoteDetail {
	return {
		path: 'Inbox/Queued.md',
		body: 'server content',
		frontmatter: null,
		raw_frontmatter: null,
		tasks: [],
		hash
	};
}

function createSummary(updated_at?: string): NoteSummary {
	return {
		path: 'Inbox/Queued.md',
		title: 'Queued',
		tags: [],
		updated_at
	};
}

test('save queues retryable failures and retries in the background', async () => {
	const queue = createQueueAdapter();
	const timers = createTimerHarness();
	const calls: string[] = [];
	let putNoteCalls = 0;

	const { SaveQueue } = await import('../src/lib/save-queue.ts');
	const saveQueue = new SaveQueue({
		putNote: async (_vault, path, content) => {
			putNoteCalls += 1;
			calls.push(`${path}:${content}`);
			if (putNoteCalls === 1) {
				throw new TypeError('fetch failed');
			}
			return { path, hash: 'hash-2' } satisfies WriteNoteResponse;
		},
		getNote: async () => createNoteDetail(),
		listNotes: async () => [createSummary()],
		queue,
		confirmConflict: async () => 'overwrite',
		schedule: timers.schedule,
		cancel: timers.cancel,
		autoInit: false
	});

	const result = await saveQueue.save('work', 'Inbox/Queued.md', 'draft body', {
		fallbackHash: 'hash-1'
	});

	assert.deepEqual(result, { path: 'Inbox/Queued.md', hash: 'hash-1' });
	assert.equal(get(saveQueue.saveState), 'failed-retrying');
	assert.equal(get(saveQueue.queuedCount), 1);
	assert.deepEqual(timers.delays(), [1000]);

	await timers.runNext(1000);

	assert.deepEqual(calls, ['Inbox/Queued.md:draft body', 'Inbox/Queued.md:draft body']);
	assert.equal(get(saveQueue.saveState), 'saved');
	assert.equal(get(saveQueue.queuedCount), 0);

	timers.runNext(2000);
	assert.equal(get(saveQueue.saveState), 'idle');
});

test('flushOnReconnect lets the user keep the newer server version', async () => {
	const queue = createQueueAdapter([
		{
			vault: 'work',
			path: 'Inbox/Queued.md',
			content: 'offline draft',
			timestamp: 1_000,
			retryCount: 3
		}
	]);
	const timers = createTimerHarness();
	let putCalls = 0;
	let promptCount = 0;

	const { SaveQueue } = await import('../src/lib/save-queue.ts');
	const saveQueue = new SaveQueue({
		putNote: async (_vault, path) => {
			putCalls += 1;
			return { path, hash: 'hash-2' };
		},
		getNote: async () => createNoteDetail('server-hash'),
		listNotes: async () => [createSummary('1970-01-01T00:00:02.000Z')],
		queue,
		confirmConflict: async () => {
			promptCount += 1;
			return 'keep-server';
		},
		schedule: timers.schedule,
		cancel: timers.cancel,
		autoInit: false
	});

	await saveQueue.refreshQueuedCount();
	await saveQueue.flushOnReconnect();

	assert.equal(promptCount, 1);
	assert.equal(putCalls, 0);
	assert.equal(get(saveQueue.queuedCount), 0);
	assert.equal(queue.records.length, 0);
	assert.equal(get(saveQueue.saveState), 'idle');
});
