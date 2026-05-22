// @ts-nocheck
import test from 'node:test';
import assert from 'node:assert/strict';

function stubSvelteRunes() {
	Object.defineProperty(globalThis, '$state', {
		value: <T>(value: T) => value,
		configurable: true,
		writable: true
	});
}

function createWindowStub() {
	const listeners = new Map<string, EventListenerOrEventListenerObject>();

	return {
		window: {
			addEventListener(type: string, handler: EventListenerOrEventListenerObject) {
				listeners.set(type, handler);
			},
			removeEventListener(type: string, handler: EventListenerOrEventListenerObject) {
				if (listeners.get(type) === handler) {
					listeners.delete(type);
				}
			}
		} as unknown as Window,
		dispatch(type: string) {
			const event = new Event(type);
			const handler = listeners.get(type);
			if (typeof handler === 'function') {
				handler(event);
			} else {
				handler?.handleEvent(event);
			}
		}
	};
}

function createVaultStoreStub() {
	return {
		currentVault: '',
		loadNotesCalls: 0,
		async loadNotes() {
			this.loadNotesCalls += 1;
		}
	};
}

function createTabStoreStub() {
	return {
		restoreTabsCalls: 0,
		closeActiveTabCalls: 0,
		reopenLastTabCalls: 0,
		restoreTabs() {
			this.restoreTabsCalls += 1;
		},
		closeActiveTab() {
			this.closeActiveTabCalls += 1;
		},
		reopenLastTab() {
			this.reopenLastTabCalls += 1;
		}
	};
}

function createCallbacks() {
	const calls = {
		openCommandPalette: 0,
		openQuickSwitcher: 0,
		toggleView: 0,
		toggleRightRail: 0,
		openSettings: 0,
		notesChanged: 0,
		externalNoteChanges: [] as Array<{ path: string; hash?: string }>,
		taskUpdated: 0,
		sidebarConfigChanged: 0,
		vaultConfigChanged: 0,
		configErrors: [] as string[],
		executedCommands: [] as string[]
	};

	return {
		calls,
		callbacks: {
			onOpenCommandPalette: () => {
				calls.openCommandPalette += 1;
			},
			onOpenQuickSwitcher: () => {
				calls.openQuickSwitcher += 1;
			},
			onToggleView: () => {
				calls.toggleView += 1;
			},
			onToggleRightRail: () => {
				calls.toggleRightRail += 1;
			},
			onOpenSettings: () => {
				calls.openSettings += 1;
			},
			onNotesChanged: () => {
				calls.notesChanged += 1;
			},
			onExternalNoteChange: (path: string, hash?: string) => {
				calls.externalNoteChanges.push({ path, hash });
			},
			onTaskUpdated: () => {
				calls.taskUpdated += 1;
			},
			onSidebarConfigChanged: () => {
				calls.sidebarConfigChanged += 1;
			},
			onVaultConfigChanged: () => {
				calls.vaultConfigChanged += 1;
			},
			onConfigError: (error: string) => {
				calls.configErrors.push(error);
			},
			commands: () => [
				{
					id: 'new-note',
					execute: () => {
						calls.executedCommands.push('new-note');
					}
				},
				{
					id: 'capture',
					execute: () => {
						calls.executedCommands.push('capture');
					}
				}
			]
		}
	};
}

async function settleAsyncWork(turns = 4) {
	for (let turn = 0; turn < turns; turn += 1) {
		await Promise.resolve();
	}
}

function okResponse(): Response {
	return {
		ok: true,
		headers: new Headers()
	} as Response;
}

test('createAppShell bootstraps the page shell and tears it down cleanly', { concurrency: false }, async () => {
	stubSvelteRunes();

	const windowStub = createWindowStub();
	const vaultStore = createVaultStoreStub();
	const tabStore = createTabStoreStub();
	const { calls, callbacks } = createCallbacks();

	let hotkeys: Array<{ key: string; meta?: boolean; shift?: boolean; action: () => void }> = [];
	let unregisterCalls = 0;
	let closeCalls = 0;

	const { createAppShell } = await import('../src/lib/app-shell-core.ts');

	const shell = createAppShell(callbacks, {
		connectSSE: () =>
			({
				close() {
					closeCalls += 1;
				}
			}) as EventSource,
		listVaults: async () => [
			{ name: 'work', is_default: true },
			{ name: 'home', is_default: false }
		],
		registerHotkeys: (registeredHotkeys) => {
			hotkeys = registeredHotkeys;
			return () => {
				unregisterCalls += 1;
			};
		},
		vaultStore,
		tabStore,
		targetWindow: windowStub.window
	});

	const vaults: string[] = [];
	await shell.init('work', vaults);

	assert.deepEqual(vaults, ['work', 'home']);
	assert.equal(vaultStore.currentVault, 'work');
	assert.equal(tabStore.restoreTabsCalls, 1);
	assert.equal(vaultStore.loadNotesCalls, 1);
	assert.equal(hotkeys.length, 15);

	hotkeys.find((candidate) => candidate.key === 'k' && candidate.meta)?.action();
	hotkeys.find((candidate) => candidate.key === 'n' && candidate.meta && !candidate.shift)?.action();
	hotkeys.find((candidate) => candidate.key === '\\' && candidate.meta)?.action();
	hotkeys.find((candidate) => candidate.key === ',' && candidate.meta)?.action();

	assert.equal(calls.openCommandPalette, 1);
	assert.deepEqual(calls.executedCommands, ['new-note']);
	assert.equal(calls.toggleRightRail, 1);
	assert.equal(calls.openSettings, 1);

	hotkeys.find((candidate) => candidate.key === 'p' && candidate.meta && !candidate.shift)?.action();
	assert.equal(calls.openQuickSwitcher, 1);

	hotkeys.find((candidate) => candidate.key === 'p' && candidate.meta && candidate.shift)?.action();
	assert.equal(calls.openCommandPalette, 2);

	shell.teardown();

	assert.equal(closeCalls, 1);
	assert.equal(unregisterCalls, 1);
});

test('createAppShell dispatches SSE events to the right page callbacks', { concurrency: false }, async () => {
	stubSvelteRunes();

	const windowStub = createWindowStub();
	const vaultStore = createVaultStoreStub();
	const tabStore = createTabStoreStub();
	const { calls, callbacks } = createCallbacks();

	let onEvent: ((event: { type: string; path: string; config?: { key: 'sidebar' | 'vault'; status: 'changed' | 'removed' | 'error'; error?: string } }) => void) | undefined;
	let onReconnect: (() => void) | undefined;
	const originalFetch = globalThis.fetch;
	const fetchCalls: string[] = [];
	globalThis.fetch = (async (input: string | URL | Request) => {
		fetchCalls.push(String(input));
		return okResponse();
	}) as typeof fetch;

	try {
		const { createAppShell } = await import('../src/lib/app-shell-core.ts');

		const shell = createAppShell(callbacks, {
			connectSSE: (_vault, handleEvent, handleReconnect) => {
				onEvent = handleEvent as typeof onEvent;
				onReconnect = handleReconnect;
				return { close() {} } as EventSource;
			},
			listVaults: async () => [{ name: 'work', is_default: true }],
			registerHotkeys: () => () => {},
			vaultStore,
			tabStore,
			targetWindow: windowStub.window
		});

		await shell.init('work', []);

		onEvent?.({
			vault: 'work',
			type: 'note.updated',
			path: 'Inbox/Refactor.md',
			timestamp: new Date().toISOString(),
			hash: 'cafebabe'
		});
		await Promise.resolve();

		assert.equal(vaultStore.loadNotesCalls, 2);
		assert.equal(calls.notesChanged, 1);
		assert.deepEqual(calls.externalNoteChanges, [
			{ path: 'Inbox/Refactor.md', hash: 'cafebabe' }
		]);

		onEvent?.({
			vault: 'work',
			type: 'task.updated',
			path: 'Inbox/Refactor.md',
			timestamp: new Date().toISOString()
		});
		assert.equal(calls.taskUpdated, 1);

		onEvent?.({
			vault: 'work',
			type: 'config.changed',
			path: '',
			timestamp: new Date().toISOString(),
			config: { key: 'sidebar', status: 'changed' }
		});
		assert.equal(calls.sidebarConfigChanged, 1);

		onEvent?.({
			vault: 'work',
			type: 'config.changed',
			path: '',
			timestamp: new Date().toISOString(),
			config: { key: 'sidebar', status: 'error', error: 'invalid section' }
		});
		onEvent?.({
			vault: 'work',
			type: 'config.changed',
			path: '',
			timestamp: new Date().toISOString(),
			config: { key: 'vault', status: 'changed' }
		});
		onEvent?.({
			vault: 'work',
			type: 'config.changed',
			path: '',
			timestamp: new Date().toISOString(),
			config: { key: 'vault', status: 'error', error: 'invalid root' }
		});

		assert.equal(calls.vaultConfigChanged, 1);
		assert.deepEqual(calls.configErrors, [
			'Sidebar config error: invalid section',
			'Vault config error: invalid root'
		]);

		onReconnect?.();
		await settleAsyncWork();
		assert.deepEqual(fetchCalls, ['/api/status']);
		assert.equal(vaultStore.loadNotesCalls, 3);
		assert.equal(calls.notesChanged, 2);
		assert.equal(calls.taskUpdated, 2);
		assert.equal(calls.sidebarConfigChanged, 2);
		assert.equal(calls.vaultConfigChanged, 2);
	} finally {
		globalThis.fetch = originalFetch;
	}
});

test('createAppShell resyncs through wake listener when daemon is healthy', { concurrency: false }, async () => {
	stubSvelteRunes();

	const windowStub = createWindowStub();
	const vaultStore = createVaultStoreStub();
	const tabStore = createTabStoreStub();
	const { calls, callbacks } = createCallbacks();

	let wakeListener: (() => void) | undefined;
	const originalFetch = globalThis.fetch;
	const fetchCalls: string[] = [];
	globalThis.fetch = (async (input: string | URL | Request) => {
		fetchCalls.push(String(input));
		return okResponse();
	}) as typeof fetch;

	try {
		const { createAppShell } = await import('../src/lib/app-shell-core.ts');

		const shell = createAppShell(callbacks, {
			connectSSE: () => ({ close() {} }) as EventSource,
			listVaults: async () => [{ name: 'work', is_default: true }],
			registerHotkeys: () => () => {},
			vaultStore,
			tabStore,
			targetWindow: windowStub.window,
			addVisibilityListener: (callback) => {
				wakeListener = callback;
				return () => {
					wakeListener = undefined;
				};
			}
		});

		await shell.init('work', []);
		wakeListener?.();
		await settleAsyncWork();

		assert.deepEqual(fetchCalls, ['/api/status']);
		assert.equal(vaultStore.loadNotesCalls, 2);
		assert.equal(calls.notesChanged, 1);
		assert.equal(calls.taskUpdated, 1);
		assert.equal(calls.sidebarConfigChanged, 1);
		assert.equal(calls.vaultConfigChanged, 1);
	} finally {
		globalThis.fetch = originalFetch;
	}
});

test('createAppShell debounces wake resyncs and removes the listener on teardown', { concurrency: false }, async () => {
	stubSvelteRunes();

	const windowStub = createWindowStub();
	const vaultStore = createVaultStoreStub();
	const tabStore = createTabStoreStub();
	const { callbacks } = createCallbacks();

	let wakeListener: (() => void) | undefined;
	let cleanupCalls = 0;
	let now = 10_000;
	const originalDateNow = Date.now;
	const originalFetch = globalThis.fetch;
	const fetchCalls: string[] = [];

	Date.now = () => now;
	globalThis.fetch = (async (input: string | URL | Request) => {
		fetchCalls.push(String(input));
		return okResponse();
	}) as typeof fetch;

	try {
		const { createAppShell } = await import('../src/lib/app-shell-core.ts');

		const shell = createAppShell(callbacks, {
			connectSSE: () => ({ close() {} }) as EventSource,
			listVaults: async () => [{ name: 'work', is_default: true }],
			registerHotkeys: () => () => {},
			vaultStore,
			tabStore,
			targetWindow: windowStub.window,
			addVisibilityListener: (callback) => {
				wakeListener = callback;
				return () => {
					cleanupCalls += 1;
					wakeListener = undefined;
				};
			}
		});

		await shell.init('work', []);

		wakeListener?.();
		await settleAsyncWork();

		now = 12_000;
		wakeListener?.();
		await settleAsyncWork();

		now = 16_000;
		wakeListener?.();
		await settleAsyncWork();

		assert.deepEqual(fetchCalls, ['/api/status', '/api/status']);
		assert.equal(vaultStore.loadNotesCalls, 3);

		shell.teardown();
		assert.equal(cleanupCalls, 1);

		now = 22_000;
		wakeListener?.();
		await settleAsyncWork();

		assert.deepEqual(fetchCalls, ['/api/status', '/api/status']);
	} finally {
		Date.now = originalDateNow;
		globalThis.fetch = originalFetch;
	}
});

test('createAppShell refreshes vault registrations after vaults.changed events when not URL-pinned', { concurrency: false }, async () => {
	stubSvelteRunes();

	const windowStub = createWindowStub();
	const vaultStore = createVaultStoreStub();
	const tabStore = createTabStoreStub();
	const { callbacks } = createCallbacks();

	let onEvent:
		| ((event: {
				vault: string;
				type: string;
				path: string;
				timestamp: string;
		  }) => void)
		| undefined;
	let closeCalls = 0;
	let connectCalls = 0;
	let listVaultCalls = 0;

	const { createAppShell } = await import('../src/lib/app-shell-core.ts');

	const shell = createAppShell(callbacks, {
		connectSSE: (_vault, handleEvent) => {
			connectCalls += 1;
			onEvent = handleEvent as typeof onEvent;
			return {
				close() {
					closeCalls += 1;
				}
			} as EventSource;
		},
		listVaults: async () => {
			listVaultCalls += 1;
			if (listVaultCalls === 1) {
				return [{ name: 'work', is_default: true }];
			}
			return [{ name: 'home', is_default: true }];
		},
		registerHotkeys: () => () => {},
		vaultStore,
		tabStore,
		targetWindow: windowStub.window
	});

	const vaults: string[] = [];
	// No URL pin → not sticky.
	await shell.init(null, vaults);
	assert.deepEqual(vaults, ['work']);
	assert.equal(vaultStore.currentVault, 'work');

	onEvent?.({
		vault: 'work',
		type: 'vaults.changed',
		path: '',
		timestamp: new Date().toISOString()
	});
	await Promise.resolve();
	await Promise.resolve();

	assert.equal(listVaultCalls, 2);
	assert.deepEqual(vaults, ['home']);
	assert.equal(vaultStore.currentVault, 'home');
	assert.equal(vaultStore.loadNotesCalls, 2);
	assert.equal(connectCalls, 2);
	assert.equal(closeCalls, 1);
});

test('createAppShell keeps URL-pinned vault sticky even when it disappears from the registry', { concurrency: false }, async () => {
	stubSvelteRunes();

	const windowStub = createWindowStub();
	const vaultStore = createVaultStoreStub();
	const tabStore = createTabStoreStub();
	const { callbacks } = createCallbacks();

	let onEvent:
		| ((event: {
				vault: string;
				type: string;
				path: string;
				timestamp: string;
		  }) => void)
		| undefined;
	let closeCalls = 0;
	let connectCalls = 0;
	let listVaultCalls = 0;

	const { createAppShell } = await import('../src/lib/app-shell-core.ts');

	const shell = createAppShell(callbacks, {
		connectSSE: (_vault, handleEvent) => {
			connectCalls += 1;
			onEvent = handleEvent as typeof onEvent;
			return {
				close() {
					closeCalls += 1;
				}
			} as EventSource;
		},
		listVaults: async () => {
			listVaultCalls += 1;
			if (listVaultCalls === 1) {
				return [{ name: 'work', is_default: true }];
			}
			return [{ name: 'home', is_default: true }];
		},
		registerHotkeys: () => () => {},
		vaultStore,
		tabStore,
		targetWindow: windowStub.window
	});

	const vaults: string[] = [];
	// URL-pinned to 'work' → sticky.
	await shell.init('work', vaults);
	assert.equal(vaultStore.currentVault, 'work');

	onEvent?.({
		vault: 'work',
		type: 'vaults.changed',
		path: '',
		timestamp: new Date().toISOString()
	});
	await Promise.resolve();
	await Promise.resolve();

	// Listed updated, but currentVault stays sticky.
	assert.equal(listVaultCalls, 2);
	assert.equal(vaultStore.currentVault, 'work');
	assert.equal(connectCalls, 1);
	assert.equal(closeCalls, 0);
});

test('createAppShell flushes queued saves after SSE reconnects', { concurrency: false }, async () => {
	stubSvelteRunes();

	const windowStub = createWindowStub();
	const vaultStore = createVaultStoreStub();
	const tabStore = createTabStoreStub();
	const { callbacks } = createCallbacks();

	let onReconnect: (() => void) | undefined;
	let flushCalls = 0;
	const originalFetch = globalThis.fetch;
	globalThis.fetch = (async () => okResponse()) as typeof fetch;

	try {
		const { createAppShell } = await import('../src/lib/app-shell-core.ts');
		const shell = createAppShell(callbacks, {
			connectSSE: (_vault, _handleEvent, handleReconnect) => {
				onReconnect = handleReconnect;
				return { close() {} } as EventSource;
			},
			listVaults: async () => [{ name: 'work', is_default: true }],
			registerHotkeys: () => () => {},
			vaultStore,
			tabStore,
			targetWindow: windowStub.window,
			flushQueuedSaves: async () => {
				flushCalls += 1;
			}
		});

		await shell.init('work', []);
		onReconnect?.();
		await settleAsyncWork();

		assert.equal(flushCalls, 1);
	} finally {
		globalThis.fetch = originalFetch;
	}
});

test('classifyAppShellEvent surfaces hash for note.updated and note.created events', async () => {
const { classifyAppShellEvent } = await import('../src/lib/app-shell-core.ts');

const updated = classifyAppShellEvent({
vault: 'work',
type: 'note.updated',
path: 'Inbox/Refactor.md',
timestamp: new Date().toISOString(),
hash: 'cafebabe'
});
assert.equal(updated.externalNotePath, 'Inbox/Refactor.md');
assert.equal(updated.externalNoteHash, 'cafebabe');
assert.equal(updated.refreshNotes, true);

const created = classifyAppShellEvent({
vault: 'work',
type: 'note.created',
path: 'Inbox/New.md',
timestamp: new Date().toISOString(),
hash: 'deadbeef'
});
assert.equal(created.externalNotePath, 'Inbox/New.md');
assert.equal(created.externalNoteHash, 'deadbeef');
});

test('classifyAppShellEvent leaves hash null when event payload omits it', async () => {
const { classifyAppShellEvent } = await import('../src/lib/app-shell-core.ts');

const updated = classifyAppShellEvent({
vault: 'work',
type: 'note.updated',
path: 'Inbox/Refactor.md',
timestamp: new Date().toISOString()
});
assert.equal(updated.externalNotePath, 'Inbox/Refactor.md');
assert.equal(updated.externalNoteHash, null);
});

test('classifyAppShellEvent ignores hash for non-note events', async () => {
const { classifyAppShellEvent } = await import('../src/lib/app-shell-core.ts');

const deleted = classifyAppShellEvent({
vault: 'work',
type: 'note.deleted',
path: 'Inbox/Old.md',
timestamp: new Date().toISOString()
});
assert.equal(deleted.externalNotePath, null);
assert.equal(deleted.externalNoteHash, null);
});
