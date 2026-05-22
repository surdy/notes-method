import test from 'node:test';
import assert from 'node:assert/strict';

import { get } from 'svelte/store';

class FakeEventSource {
	static instances: FakeEventSource[] = [];

	url: string;
	onmessage: ((event: MessageEvent<string>) => void) | null = null;
	onopen: (() => void) | null = null;
	onerror: (() => void) | null = null;
	listeners = new Map<string, Array<(event: MessageEvent<string>) => void>>();
	closeCalls = 0;

	constructor(url: string) {
		this.url = url;
		FakeEventSource.instances.push(this);
	}

	addEventListener(type: string, listener: EventListener) {
		const handlers = this.listeners.get(type) ?? [];
		handlers.push(listener as (event: MessageEvent<string>) => void);
		this.listeners.set(type, handlers);
	}

	close() {
		this.closeCalls += 1;
	}

	emitOpen() {
		this.onopen?.();
	}

	emitError() {
		this.onerror?.();
	}

	emitNamed(type: string, payload: Record<string, unknown>, lastEventId = '') {
		const event = {
			data: JSON.stringify(payload),
			lastEventId
		} as MessageEvent<string>;
		for (const listener of this.listeners.get(type) ?? []) {
			listener(event);
		}
	}
}

test('connectSSE reconnects with exponential backoff and replays from the last event id', async () => {
	const originalEventSource = globalThis.EventSource;
	const originalSetTimeout = globalThis.setTimeout;
	const originalClearTimeout = globalThis.clearTimeout;
	const timers: Array<{ fn: () => void; ms: number; cleared: boolean }> = [];
	const seenEvents: Array<{ type: string; path: string }> = [];
	let reconnects = 0;

	Object.defineProperty(globalThis, 'EventSource', {
		value: FakeEventSource,
		configurable: true,
		writable: true
	});
	Object.defineProperty(globalThis, 'setTimeout', {
		value: ((fn: () => void, ms?: number) => {
			const timer = { fn, ms: ms ?? 0, cleared: false };
			timers.push(timer);
			return timer;
		}) as unknown as typeof setTimeout,
		configurable: true,
		writable: true
	});
	Object.defineProperty(globalThis, 'clearTimeout', {
		value: ((timer?: { cleared?: boolean }) => {
			if (timer) {
				timer.cleared = true;
			}
		}) as unknown as typeof clearTimeout,
		configurable: true,
		writable: true
	});

	try {
		const { connectSSE, connectionState, daemonShuttingDown } = await import('../src/lib/sse.ts');
		connectionState.set('disconnected');
		daemonShuttingDown.set(false);
		FakeEventSource.instances.length = 0;

		const connection = connectSSE(
			'work',
			(event) => {
				seenEvents.push({ type: event.type, path: event.path });
			},
			() => {
				reconnects += 1;
			}
		);

		assert.equal(FakeEventSource.instances.length, 1);
		assert.equal(FakeEventSource.instances[0]?.url, '/api/v/work/events');

		FakeEventSource.instances[0]?.emitOpen();
		assert.equal(get(connectionState), 'connected');

		FakeEventSource.instances[0]?.emitNamed(
			'note.updated',
			{
				id: 5,
				vault: 'work',
				type: 'note.updated',
				path: 'Inbox/Refactor.md',
				timestamp: new Date().toISOString()
			},
			'5'
		);
		FakeEventSource.instances[0]?.emitNamed(
			'shutting_down',
			{
				vault: 'work',
				type: 'shutting_down',
				path: '',
				timestamp: new Date().toISOString()
			},
			'6'
		);

		assert.deepEqual(seenEvents, [
			{ type: 'note.updated', path: 'Inbox/Refactor.md' },
			{ type: 'shutting_down', path: '' }
		]);
		assert.equal(get(daemonShuttingDown), true);

		FakeEventSource.instances[0]?.emitError();
		assert.equal(get(connectionState), 'reconnecting');
		assert.equal(timers[0]?.ms, 1000);
		timers[0]?.fn();

		assert.equal(FakeEventSource.instances.length, 2);
		assert.equal(FakeEventSource.instances[1]?.url, '/api/v/work/events?last_event_id=6');

		FakeEventSource.instances[1]?.emitOpen();
		assert.equal(get(connectionState), 'connected');
		assert.equal(get(daemonShuttingDown), false);
		assert.equal(reconnects, 1);

		FakeEventSource.instances[1]?.emitError();
		assert.equal(timers[1]?.ms, 1000);

		connection.close();
		assert.equal(get(connectionState), 'disconnected');
		assert.equal(FakeEventSource.instances[1]?.closeCalls, 1);
		assert.equal(timers[1]?.cleared, true);
	} finally {
		Object.defineProperty(globalThis, 'EventSource', {
			value: originalEventSource,
			configurable: true,
			writable: true
		});
		Object.defineProperty(globalThis, 'setTimeout', {
			value: originalSetTimeout,
			configurable: true,
			writable: true
		});
		Object.defineProperty(globalThis, 'clearTimeout', {
			value: originalClearTimeout,
			configurable: true,
			writable: true
		});
		FakeEventSource.instances.length = 0;
	}
});
