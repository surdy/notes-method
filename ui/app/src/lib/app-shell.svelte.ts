import { createAppShell as createCoreAppShell, type AppShellCallbacks } from './app-shell-core.ts';
import { listVaults } from './api';
import { registerHotkeys } from './hotkeys';
import { connectSSE } from './sse';
import { saveQueue } from './save-queue.ts';
import { tabStore } from './tab-store.svelte';
import { vaultStore } from './stores.svelte';

export { classifyAppShellEvent, type AppShellEvent } from './app-shell-core.ts';
export type { AppShellCallbacks } from './app-shell-core.ts';

export function createAppShell(callbacks: AppShellCallbacks) {
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
