const STORAGE_KEY_PREFIX = 'notesmith:tabs';

/**
 * Per-vault storage key for the tab state. Returns null when no vault is
 * selected so callers can skip the read/write (avoids cross-window clobber
 * of a stale, unscoped `notesmith:tabs` blob).
 */
export function tabStorageKey(vault: string): string | null {
  if (!vault) return null;
  return `${STORAGE_KEY_PREFIX}:${vault}`;
}
