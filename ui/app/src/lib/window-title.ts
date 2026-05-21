/**
 * Push `<vault> — <note>` to the Tauri window title via the
 * `set_window_title` command. In non-Tauri contexts (dev browser, tests)
 * this is a no-op.
 */

interface TauriRuntime {
  core?: {
    invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
  };
}

export interface WindowTitleAdapter {
  invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
}

export function formatWindowTitle(vault: string, noteTitle: string | null): string {
  const trimmedVault = vault.trim();
  const trimmedNote = noteTitle?.trim() ?? '';
  if (!trimmedVault) return trimmedNote;
  if (!trimmedNote) return trimmedVault;
  return `${trimmedVault} — ${trimmedNote}`;
}

export async function pushWindowTitle(
  vault: string,
  noteTitle: string | null,
  adapter: WindowTitleAdapter | null = resolveTauri()
): Promise<void> {
  if (!adapter) return;
  const title = formatWindowTitle(vault, noteTitle);
  try {
    await adapter.invoke('set_window_title', { title });
  } catch (error) {
    console.warn('set_window_title failed', error);
  }
}

function resolveTauri(): WindowTitleAdapter | null {
  if (typeof window === 'undefined') return null;
  const t = (window as unknown as { __TAURI__?: TauriRuntime }).__TAURI__;
  if (!t?.core?.invoke) return null;
  return { invoke: (cmd, args) => t.core!.invoke(cmd, args) };
}
