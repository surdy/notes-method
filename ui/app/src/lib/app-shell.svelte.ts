import { createAppShell as createCoreAppShell, type AppShellCallbacks } from './app-shell-core.ts';
import { listVaults } from './api';
import { registerHotkeys } from './hotkeys';
import { connectSSE } from './sse';
import { saveQueue } from './save-queue.ts';
import { tabStore } from './tab-store.svelte';
import { vaultStore } from './stores.svelte';
import { attachWindowCloseConfirm } from './window-lifecycle.ts';

export { classifyAppShellEvent, type AppShellEvent } from './app-shell-core.ts';
export type { AppShellCallbacks } from './app-shell-core.ts';

export function createAppShell(callbacks: AppShellCallbacks) {
	// Register the Tauri close-confirm bridge once per window mount. The
	// promise resolves to an unlisten fn; we don't currently expose a tear-
	// down hook because the listener should live for the lifetime of the
	// window. In tests the adapter resolves to null and this is a no-op.
	void attachWindowCloseConfirm({
		hasDirtyWork: () => tabStore.tabs.some((tab) => tab.dirty),
		confirmDiscard: () =>
			typeof window !== 'undefined' &&
			typeof window.confirm === 'function'
				? window.confirm(
						'You have unsaved changes in this vault. Close the window and discard them?'
					)
				: true
	});

	return createCoreAppShell(callbacks, {
		connectSSE,
		listVaults,
		registerHotkeys,
		vaultStore,
		tabStore,
		targetWindow: window,
		addVisibilityListener: (callback) => {
			const handleVisibilityChange = () => {
				if (document.visibilityState === 'visible') {
					callback();
				}
			};
			const handleWake = () => callback();

			document.addEventListener('visibilitychange', handleVisibilityChange);
			window.addEventListener('notesmith://wake', handleWake);

			return () => {
				document.removeEventListener('visibilitychange', handleVisibilityChange);
				window.removeEventListener('notesmith://wake', handleWake);
			};
		},
		flushQueuedSaves: () => saveQueue.flushOnReconnect(),
		logger: console
	});
}
