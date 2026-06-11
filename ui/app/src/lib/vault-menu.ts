import type { TauriBridge } from './open-folder-as-vault.ts';

/**
 * The in-app vault dropdown is browser-only: the desktop (Tauri) shell exposes
 * the same actions through its native OS menu and a window-per-vault model, so
 * the dropdown would only duplicate those affordances there. Browser mode is
 * detected by the absence of a Tauri bridge.
 */
export function isBrowserVaultMenu(bridge: TauriBridge | null | undefined): boolean {
  return bridge == null;
}

export interface VaultMenuEntry {
  name: string;
  isCurrent: boolean;
}

export interface VaultMenuModel {
  vaults: VaultMenuEntry[];
  hasOtherVaults: boolean;
}

export function buildVaultMenuModel(input: {
  vaults: string[];
  currentVault: string;
}): VaultMenuModel {
  const vaults = input.vaults.map((name) => ({
    name,
    isCurrent: name === input.currentVault
  }));
  return {
    vaults,
    hasOtherVaults: vaults.some((entry) => !entry.isCurrent)
  };
}

/**
 * Build the URL for switching the current browser tab to another vault by
 * setting the `vault` query parameter while preserving the rest of the URL.
 */
export function vaultSwitchUrl(currentHref: string, vault: string): string {
  const url = new URL(currentHref);
  url.searchParams.set('vault', vault);
  return url.toString();
}

/**
 * Build the settings route under the SvelteKit base path, scoped to the given
 * vault when one is selected.
 */
export function settingsRoute(base: string, vault: string): string {
  const route = `${base}/settings`;
  return vault ? `${route}?vault=${encodeURIComponent(vault)}` : route;
}
