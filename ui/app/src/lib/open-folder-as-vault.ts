/**
 * Pure validation + Tauri-bridge helpers for the "Open Folder as Vault" flow.
 *
 * Keeping these out of the Svelte component lets them be unit-tested with
 * Vitest without spinning up a renderer.
 */

export interface VaultNameError {
  message: string;
}

/**
 * Validate a candidate display name against the existing vault registry.
 * Returns the trimmed name on success, or a structured error.
 *
 * Mirrors `validate_vault_display_name` in `crates/notesmith-tauri/src/vault_menu.rs`
 * so the user gets immediate feedback before the round-trip.
 */
export function validateVaultName(
  candidate: string,
  existing: readonly string[]
): { ok: true; value: string } | { ok: false; error: VaultNameError } {
  const trimmed = candidate.trim();
  if (!trimmed) {
    return { ok: false, error: { message: 'Vault name cannot be empty.' } };
  }
  if (trimmed.startsWith('.')) {
    return {
      ok: false,
      error: { message: 'Vault name cannot start with a dot.' }
    };
  }
  if (trimmed.includes('/') || trimmed.includes('\\')) {
    return {
      ok: false,
      error: { message: 'Vault name cannot contain path separators.' }
    };
  }
  if (existing.some((name) => name === trimmed)) {
    return {
      ok: false,
      error: { message: `A vault named "${trimmed}" already exists.` }
    };
  }
  return { ok: true, value: trimmed };
}

/**
 * Derive a default display name from a folder path. Picks the last non-empty
 * segment and trims any leading dots so the result passes `validateVaultName`.
 */
export function defaultNameFromPath(path: string): string {
  const cleaned = path.replace(/[\\/]+$/, '');
  const parts = cleaned.split(/[\\/]/);
  const last = parts[parts.length - 1] ?? '';
  return last.replace(/^\.+/, '').trim();
}

export interface TauriBridge {
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
}

export function resolveTauri(): TauriBridge | null {
  if (typeof window === 'undefined') return null;
  const t = (window as unknown as {
    __TAURI__?: { core?: { invoke?: (cmd: string, args: Record<string, unknown>) => Promise<unknown> } };
  }).__TAURI__;
  if (!t?.core?.invoke) return null;
  const inv = t.core.invoke;
  return { invoke: (cmd, args) => inv(cmd, args ?? {}) };
}
