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

export type VaultRegistrationMode = 'local' | 'remote';

export function vaultRegistrationMode(apiBase: string): VaultRegistrationMode {
  return apiBase.trim() ? 'remote' : 'local';
}

export function shouldUseNativeVaultRegistration(
  apiBase: string,
  bridge: TauriBridge | null | undefined
): boolean {
  return vaultRegistrationMode(apiBase) === 'remote' && bridge != null;
}

/**
 * Title + hint copy for the Add Vault surface, naming the **target server** for
 * remote registration so the user always knows which daemon receives the vault
 * (ADR 0017 Phase D). For a local registration the copy refers to a local
 * folder; for remote it names the server when known, falling back to a generic
 * label otherwise.
 */
export function vaultTargetCopy(
  mode: VaultRegistrationMode,
  serverName: string | null
): { title: string; hint: string } {
  if (mode === 'local') {
    return {
      title: 'Open Folder as Vault',
      hint: 'Choose a local folder to register as a new vault.'
    };
  }
  const trimmed = serverName?.trim() ?? '';
  const where = trimmed ? trimmed : 'the remote Notesmith server';
  return {
    title: trimmed ? `Add Vault on ${trimmed}` : 'Add Remote Vault',
    hint: `Enter the folder path as seen by ${where}.`
  };
}

export function messageFromUnknownError(error: unknown, fallback: string): string {
  if (typeof error === 'string') return error;
  if (error instanceof Error) return error.message;
  return fallback;
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

/** The "Start from" choice, as offered in the new-vault modal. */
export interface KitChoice {
  id: string;
  label: string;
  description: string;
}

/** Sentinel for "no kit" — an empty vault, which is the default. */
export const NO_KIT = '';

/**
 * Build the "Start from" options: an empty vault first, then whatever kits the
 * daemon ships.
 *
 * Empty leads deliberately. Applying a kit later is safe and idempotent
 * (`notesmith kit apply`), whereas scaffolding a folder the user only meant to
 * register leaves files they have to clean up — so the reversible choice is the
 * default.
 */
export function kitChoices(
  kits: readonly { id: string; description: string }[]
): KitChoice[] {
  return [
    {
      id: NO_KIT,
      label: 'Empty vault',
      description: 'Register the folder as-is.'
    },
    ...kits.map((kit) => ({
      id: kit.id,
      label: kitLabel(kit.id),
      description: kit.description
    }))
  ];
}

/** `work-notes` → `Work Notes`, so ids stay machine-facing. */
export function kitLabel(id: string): string {
  return id
    .split(/[-_]/)
    .filter(Boolean)
    .map((word) => word[0].toUpperCase() + word.slice(1))
    .join(' ');
}

/** The value to send as `kit`, or undefined for an empty vault. */
export function kitForRequest(selected: string): string | undefined {
  const trimmed = selected.trim();
  return trimmed === NO_KIT ? undefined : trimmed;
}
