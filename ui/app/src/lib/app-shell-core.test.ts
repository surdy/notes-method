import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { createAppShell } from './app-shell-core.ts';
import type {
	AppShellCallbacks,
	AppShellDependencies,
	AppShellVaultStore
} from './app-shell-core.ts';

function makeVaultStore() {
	const loadNotes = vi.fn().mockResolvedValue(undefined);
	const store = { currentVault: '', loadNotes } as unknown as AppShellVaultStore & {
		loadNotes: typeof loadNotes;
	};
	return store;
}

function makeCallbacks(): AppShellCallbacks {
	return {
		onOpenCommandPalette: vi.fn(),
		onOpenQuickSwitcher: vi.fn(),
		onToggleView: vi.fn(),
		onToggleRightRail: vi.fn(),
		onOpenSettings: vi.fn(),
		onNotesChanged: vi.fn(),
		onExternalNoteChange: vi.fn(),
		onTaskUpdated: vi.fn(),
		onSidebarConfigChanged: vi.fn(),
		onVaultConfigChanged: vi.fn(),
		onConfigError: vi.fn(),
		commands: () => []
	};
}

function makeDeps(
	overrides: Partial<AppShellDependencies> & { vaultStore: ReturnType<typeof makeVaultStore> }
): AppShellDependencies {
	return {
		connectSSE: vi.fn().mockReturnValue({ close: vi.fn() }),
		listVaults: vi.fn().mockResolvedValue([]),
		registerHotkeys: vi.fn().mockReturnValue(() => {}),
		tabStore: { restoreTabs: vi.fn(), closeActiveTab: vi.fn(), reopenLastTab: vi.fn() },
		targetWindow: {
			addEventListener: vi.fn(),
			removeEventListener: vi.fn()
		} as unknown as Window,
		logger: { error: vi.fn() },
		...overrides
	};
}

describe('createAppShell — init with no vaults', () => {
	it('does not call loadNotes when vault list is empty', async () => {
		const vaultStore = makeVaultStore();
		const deps = makeDeps({
			vaultStore,
			listVaults: vi.fn().mockResolvedValue([])
		});
		const shell = createAppShell(makeCallbacks(), deps);

		await shell.init(null, []);

		expect(vaultStore.loadNotes).not.toHaveBeenCalled();
	});

	it('does not call connectSSE when vault list is empty', async () => {
		const vaultStore = makeVaultStore();
		const deps = makeDeps({
			vaultStore,
			listVaults: vi.fn().mockResolvedValue([])
		});
		const shell = createAppShell(makeCallbacks(), deps);

		await shell.init(null, []);

		expect(deps.connectSSE).not.toHaveBeenCalled();
	});

	it('calls loadNotes when a vault is registered', async () => {
		const vaultStore = makeVaultStore();
		const deps = makeDeps({
			vaultStore,
			listVaults: vi
				.fn()
				.mockResolvedValue([{ name: 'personal', is_default: true }])
		});
		const shell = createAppShell(makeCallbacks(), deps);

		await shell.init(null, []);

		expect(vaultStore.loadNotes).toHaveBeenCalledOnce();
		expect(deps.connectSSE).toHaveBeenCalledWith(
			'personal',
			expect.any(Function),
			expect.any(Function)
		);
	});
});
