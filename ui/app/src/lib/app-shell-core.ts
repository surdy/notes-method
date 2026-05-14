import { API_BASE } from './api/core.ts';
import { OPEN_QUICK_SWITCHER_EVENT } from './command-events.ts';
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
	onExternalNoteChange: (path: string) => void;
	onTaskUpdated: () => void;
	onSidebarConfigChanged: () => void;
	onVaultConfigChanged: () => void;
	onConfigError: (error: string) => void;
	commands: () => CommandLike[];
}

export interface AppShellEvent {
	refreshNotes: boolean;
	externalNotePath: string | null;
	taskUpdated: boolean;
	sidebarConfigChanged: boolean;
	vaultConfigChanged: boolean;
	configError: string | null;
}

export interface AppShellVaultStore {
	currentVault: string;
	loadNotes: () => Promise<void> | void;
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

	return {
		refreshNotes,
		externalNotePath:
			event.type === 'note.updated' || event.type === 'note.created' ? event.path : null,
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
		{ key: 'k', meta: true, action: callbacks.onOpenCommandPalette },
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
	let handleOpenQuickSwitcher: EventListener | null = null;
	let availableVaults: string[] | null = null;
	let lastResyncTime = 0;

	async function performResync() {
		try {
			const response = await fetch(`${API_BASE}/api/status`);
			if (!response.ok) {
				return;
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
			callbacks.onExternalNoteChange(appEvent.externalNotePath);
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
		sseConnection = dependencies.connectSSE(vault, handleEvent, performResync);
	}

	async function refreshVaultRegistrations() {
		const registrations = await dependencies.listVaults();
		const vaultNames = registrations.map((vault) => vault.name);
		availableVaults?.splice(0, availableVaults.length, ...vaultNames);

		if (vaultNames.includes(dependencies.vaultStore.currentVault)) {
			return;
		}

		const nextVault =
			registrations.find((vault) => vault.is_default)?.name ?? vaultNames[0] ?? '';
		if (!nextVault) {
			return;
		}

		dependencies.vaultStore.currentVault = nextVault;
		await Promise.resolve(dependencies.vaultStore.loadNotes());
		callbacks.onNotesChanged();
		connectToVault(nextVault);
	}

	async function init(vaultParam: string | null, vaults: string[]) {
		teardown();
		availableVaults = vaults;

		handleOpenQuickSwitcher = () => callbacks.onOpenQuickSwitcher();
		dependencies.targetWindow.addEventListener(
			OPEN_QUICK_SWITCHER_EVENT,
			handleOpenQuickSwitcher as EventListener
		);
		unregisterHotkeys = dependencies.registerHotkeys(
			buildHotkeys(callbacks, dependencies.tabStore, logger)
		);
		removeVisibilityListener = dependencies.addVisibilityListener?.(resyncIfNeeded) ?? (() => {});

		try {
			const registrations = await dependencies.listVaults();
			vaults.splice(0, vaults.length, ...registrations.map((vault) => vault.name));
			const defaultVault =
				registrations.find((vault) => vault.is_default)?.name ?? registrations[0]?.name ?? 'work';
			const vault = vaultParam ?? defaultVault;
			dependencies.vaultStore.currentVault = vault;
			dependencies.tabStore.restoreTabs();
			await dependencies.vaultStore.loadNotes();

			connectToVault(vault);
		} catch (error) {
			logger.error('Failed to initialize Notesmith app shell', error);
		}
	}

	function teardown() {
		if (handleOpenQuickSwitcher) {
			dependencies.targetWindow.removeEventListener(
				OPEN_QUICK_SWITCHER_EVENT,
				handleOpenQuickSwitcher as EventListener
			);
			handleOpenQuickSwitcher = null;
		}

		sseConnection?.close();
		sseConnection = null;

		unregisterHotkeys();
		unregisterHotkeys = () => {};
		removeVisibilityListener();
		removeVisibilityListener = () => {};
		lastResyncTime = 0;
		availableVaults = null;
	}

	return { init, teardown };
}
