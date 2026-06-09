import { API_BASE, apiFetch } from './api/core.ts';
import type { Hotkey } from './hotkeys.ts';
import type { VaultEvent } from './sse.ts';

const RESYNC_DEBOUNCE_MS = 5000;

export interface CommandLike {
	id: string;
	execute: () => void | Promise<void>;
}

export interface AppShellCallbacks {
	onOpenCommandPalette: () => void;
	onOpenQuickSwitcher: () => void;
	onToggleView: () => void | Promise<void>;
	onToggleRightRail: () => void;
	onOpenSettings: () => void;
	onNotesChanged: () => void;
	onExternalNoteChange: (path: string, hash?: string) => void;
	onTaskUpdated: () => void;
	onSidebarConfigChanged: () => void;
	onVaultConfigChanged: () => void;
	onConfigError: (error: string) => void;
	commands: () => CommandLike[];
}

export interface AppShellEvent {
	refreshNotes: boolean;
	externalNotePath: string | null;
	externalNoteHash: string | null;
	taskUpdated: boolean;
	sidebarConfigChanged: boolean;
	vaultConfigChanged: boolean;
	configError: string | null;
}

export interface AppShellVaultStore {
	currentVault: string;
	notes: { length: number; splice: (start: number, deleteCount: number) => unknown };
	loadNotes: () => Promise<void> | void;
	clearError: () => void;
}

export interface AppShellTabStore {
	restoreTabs: () => void;
	closeActiveTab: () => void;
	reopenLastTab: () => void;
}

export interface AppShellWindowTarget {
	addEventListener: Window['addEventListener'];
	removeEventListener: Window['removeEventListener'];
}

export interface AppShellEventSource {
	close: () => void;
}

export interface AppShellVaultRegistration {
	name: string;
	is_default: boolean;
}

export interface AppShellDependencies {
	connectSSE: (
		vault: string,
		onEvent: (event: VaultEvent) => void,
		onReconnect?: () => void
	) => AppShellEventSource;
	listVaults: () => Promise<AppShellVaultRegistration[]>;
	registerHotkeys: (hotkeys: Hotkey[]) => () => void;
	vaultStore: AppShellVaultStore;
	tabStore: AppShellTabStore;
	targetWindow: AppShellWindowTarget;
	addVisibilityListener?: (callback: () => void) => () => void;
	flushQueuedSaves?: () => Promise<void> | void;
	logger?: Pick<Console, 'error'>;
}

export function classifyAppShellEvent(event: VaultEvent): AppShellEvent {
	const refreshNotes =
		event.type.startsWith('note.') ||
		event.type === 'note.captured' ||
		event.type === 'daily.created' ||
		event.type === 'cache.rebuilt';

	let sidebarConfigChanged = false;
	let vaultConfigChanged = false;
	let configError: string | null = null;

	if (event.type.startsWith('config.')) {
		if (event.config?.key === 'sidebar') {
			if (event.config.status === 'error') {
				configError = `Sidebar config error: ${event.config.error ?? 'unknown error'}`;
			} else {
				sidebarConfigChanged = true;
			}
		}

		if (event.config?.key === 'vault') {
			if (event.config.status === 'error') {
				configError = `Vault config error: ${event.config.error ?? 'unknown error'}`;
			} else {
				vaultConfigChanged = true;
			}
		}
	}

	const isExternalNoteEvent = event.type === 'note.updated' || event.type === 'note.created';

	return {
		refreshNotes,
		externalNotePath: isExternalNoteEvent ? event.path : null,
		externalNoteHash: isExternalNoteEvent ? (event.hash ?? null) : null,
		taskUpdated: event.type === 'task.updated',
		sidebarConfigChanged,
		vaultConfigChanged,
		configError
	};
}

function buildHotkeys(
	callbacks: AppShellCallbacks,
	tabStore: AppShellTabStore,
	logger: Pick<Console, 'error'>
): Hotkey[] {
	function runCommand(commandId: string) {
		const command = callbacks.commands().find((candidate) => candidate.id === commandId);
		if (command) {
			void Promise.resolve(command.execute()).catch((error) => {
				logger.error(`Failed to execute command: ${commandId}`, error);
			});
		}
	}

	return [
		{ key: 'p', meta: true, action: callbacks.onOpenCommandPalette },
		{ key: 'o', meta: true, action: callbacks.onOpenQuickSwitcher },
		{ key: 'w', meta: true, action: () => tabStore.closeActiveTab() },
		{ key: 'n', meta: true, action: () => runCommand('new-note') },
		{ key: 'd', meta: true, action: () => runCommand('open-daily') },
		{ key: 'a', meta: true, shift: true, action: () => runCommand('archive-current') },
		{ key: 'n', meta: true, shift: true, action: () => runCommand('capture') },
		{ key: 's', meta: true, action: () => {} },
		{ key: 'e', meta: true, action: () => void callbacks.onToggleView() },
		{ key: 't', meta: true, shift: true, action: () => tabStore.reopenLastTab() },
		{ key: '\\', meta: true, action: callbacks.onToggleRightRail },
		{ key: 'f', meta: true, shift: true, action: callbacks.onOpenQuickSwitcher },
		{ key: ',', meta: true, action: callbacks.onOpenSettings }
	];
}

export function createAppShell(callbacks: AppShellCallbacks, dependencies: AppShellDependencies) {
	const logger = dependencies.logger ?? console;

	let sseConnection: AppShellEventSource | null = null;
	let unregisterHotkeys = () => {};
	let removeVisibilityListener = () => {};
	let availableVaults: string[] | null = null;
	let lastResyncTime = 0;
	let urlPinnedVault = false;

	async function performResync(flushQueuedSaves = false) {
		try {
			const response = await apiFetch(`${API_BASE}/api/status`);
			if (!response.ok) {
				return;
			}

			if (flushQueuedSaves) {
				await Promise.resolve(dependencies.flushQueuedSaves?.());
			}

			await Promise.resolve(dependencies.vaultStore.loadNotes());
			callbacks.onNotesChanged();
			callbacks.onTaskUpdated();
			callbacks.onSidebarConfigChanged();
			callbacks.onVaultConfigChanged();
		} catch {}
	}

	function resyncIfNeeded() {
		const now = Date.now();
		if (now - lastResyncTime < RESYNC_DEBOUNCE_MS) {
			return;
		}

		lastResyncTime = now;
		void performResync();
	}

	function handleEvent(event: VaultEvent) {
		if (event.type === 'vaults.changed') {
			void refreshVaultRegistrations();
			return;
		}

		const appEvent = classifyAppShellEvent(event);

		if (appEvent.refreshNotes) {
			void Promise.resolve(dependencies.vaultStore.loadNotes()).finally(() => {
				callbacks.onNotesChanged();
			});
		}

		if (appEvent.externalNotePath) {
			callbacks.onExternalNoteChange(
				appEvent.externalNotePath,
				appEvent.externalNoteHash ?? undefined
			);
		}

		if (!appEvent.refreshNotes && appEvent.taskUpdated) {
			callbacks.onTaskUpdated();
		}

		if (appEvent.sidebarConfigChanged) {
			callbacks.onSidebarConfigChanged();
		}

		if (appEvent.vaultConfigChanged) {
			callbacks.onVaultConfigChanged();
		}

		if (appEvent.configError) {
			callbacks.onConfigError(appEvent.configError);
		}
	}

	function connectToVault(vault: string) {
		sseConnection?.close();
		sseConnection = dependencies.connectSSE(vault, handleEvent, () => performResync(true));
	}

	async function refreshVaultRegistrations() {
		const registrations = await dependencies.listVaults();
		const vaultNames = registrations.map((vault) => vault.name);
		availableVaults?.splice(0, availableVaults.length, ...vaultNames);

		if (vaultNames.includes(dependencies.vaultStore.currentVault)) {
			return;
		}

		// Current vault is gone. If no vaults remain, drop into the empty
		// state so the user isn't shown stale notes from a deleted vault
		// (and isn't bombarded with 404 errors when the UI tries to load
		// notes for a vault that no longer exists).
		if (vaultNames.length === 0) {
			sseConnection?.close();
			sseConnection = null;
			dependencies.vaultStore.currentVault = '';
			dependencies.vaultStore.notes.splice(0, dependencies.vaultStore.notes.length);
			dependencies.vaultStore.clearError();
			callbacks.onNotesChanged();
			return;
		}

		// Sticky window: once a vault was URL-pinned for this window, never
		// auto-swap it away unless the vault was deleted. If it *was*
		// deleted, fall through so we pick a replacement instead of leaving
		// the window in a broken "vault not found" state.
		const pinnedAndStillExists =
			urlPinnedVault && vaultNames.includes(dependencies.vaultStore.currentVault);
		if (urlPinnedVault && pinnedAndStillExists) {
			return;
		}

		const nextVault =
			registrations.find((vault) => vault.is_default)?.name ?? vaultNames[0] ?? '';
		if (!nextVault) {
			return;
		}

		dependencies.vaultStore.currentVault = nextVault;
		dependencies.vaultStore.clearError();
		await Promise.resolve(dependencies.vaultStore.loadNotes());
		callbacks.onNotesChanged();
		connectToVault(nextVault);
	}

	async function init(vaultParam: string | null, vaults: string[]) {
		teardown();
		availableVaults = vaults;
		urlPinnedVault = vaultParam !== null && vaultParam !== '';

		unregisterHotkeys = dependencies.registerHotkeys(
			buildHotkeys(callbacks, dependencies.tabStore, logger)
		);
		removeVisibilityListener = dependencies.addVisibilityListener?.(resyncIfNeeded) ?? (() => {});

		try {
			const registrations = await dependencies.listVaults();
			vaults.splice(0, vaults.length, ...registrations.map((vault) => vault.name));
			const defaultVault =
				registrations.find((vault) => vault.is_default)?.name ?? registrations[0]?.name ?? '';
			const vault = vaultParam ?? defaultVault;

			// No vaults registered yet — stay on the empty state; don't issue
			// a notes request that would return 404 and show a spurious error.
			if (!vault) {
				return;
			}

			dependencies.vaultStore.currentVault = vault;
			dependencies.tabStore.restoreTabs();
			await dependencies.vaultStore.loadNotes();

			connectToVault(vault);
		} catch (error) {
			logger.error('Failed to initialize Notesmith app shell', error);
		}
	}

	function teardown() {
		sseConnection?.close();
		sseConnection = null;

		unregisterHotkeys();
		unregisterHotkeys = () => {};
		removeVisibilityListener();
		removeVisibilityListener = () => {};
		lastResyncTime = 0;
		availableVaults = null;
		urlPinnedVault = false;
	}

	return { init, teardown };
}
