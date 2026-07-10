import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { chooseRegisteredVault, createAppShell } from './app-shell-core.ts';
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
		onFocusSidebarFilter: vi.fn(),
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

	it('falls back to the remote default when URL vault is not registered', async () => {
		const vaultStore = makeVaultStore();
		const deps = makeDeps({
			vaultStore,
			listVaults: vi.fn().mockResolvedValue([
				{ name: 'remote-work', is_default: true },
				{ name: 'remote-personal', is_default: false }
			])
		});
		const shell = createAppShell(makeCallbacks(), deps);
		const vaults: string[] = [];

		await shell.init('local-only', vaults);

		expect(vaultStore.currentVault).toBe('remote-work');
		expect(vaultStore.loadNotes).toHaveBeenCalledOnce();
		expect(deps.connectSSE).toHaveBeenCalledWith(
			'remote-work',
			expect.any(Function),
			expect.any(Function)
		);
		expect(vaults).toEqual(['remote-work', 'remote-personal']);
	});
});

describe('chooseRegisteredVault', () => {
	it('keeps a URL-pinned vault only when it is registered', () => {
		expect(
			chooseRegisteredVault('remote-personal', [
				{ name: 'remote-work', is_default: true },
				{ name: 'remote-personal', is_default: false }
			])
		).toEqual({ vault: 'remote-personal', pinned: true });
	});

	it('falls back to default when the URL-pinned vault is stale', () => {
		expect(
			chooseRegisteredVault('local-only', [
				{ name: 'remote-work', is_default: true },
				{ name: 'remote-personal', is_default: false }
			])
		).toEqual({ vault: 'remote-work', pinned: false });
	});
});
