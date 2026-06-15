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
 * vault when one is selected. An optional `section` deep-links to a specific
 * settings section (e.g. `connection`).
 */
export function settingsRoute(base: string, vault: string, section?: string): string {
  const route = `${base}/settings`;
  const params: string[] = [];
  if (vault) params.push(`vault=${encodeURIComponent(vault)}`);
  if (section) params.push(`section=${encodeURIComponent(section)}`);
  return params.length > 0 ? `${route}?${params.join('&')}` : route;
}

export interface DropdownAnchor {
  bottom: number;
  left: number;
}

export interface DropdownPosition {
  top: number;
  left: number;
}

/**
 * Compute the viewport-relative coordinates for the vault dropdown, anchored
 * just below its trigger. The menu uses `position: fixed` with these
 * coordinates so it escapes the workspace chrome's `overflow: hidden`, which
 * would otherwise clip it (the dropdown appeared hidden behind the sidebar).
 * The left edge is clamped so the menu stays within the viewport.
 */
export function vaultDropdownPosition(
  trigger: DropdownAnchor,
  viewportWidth: number,
  menuWidth = 220
): DropdownPosition {
  const gap = 4;
  const margin = 8;
  const top = trigger.bottom + gap;
  const maxLeft = viewportWidth - menuWidth - margin;
  const left = Math.max(margin, Math.min(trigger.left, maxLeft));
  return { top, left };
}
