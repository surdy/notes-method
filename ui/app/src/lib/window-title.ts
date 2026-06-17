/**
 * Push `<note> — Notesmith` to the Tauri window title via the
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

const APP_NAME = 'Notesmith';

export function formatWindowTitle(
  _vault: string,
  _noteTitle: string | null,
  serverSuffix: string | null = null
): string {
  return serverSuffix ? `${APP_NAME} — ${serverSuffix}` : APP_NAME;
}

export async function pushWindowTitle(
  vault: string,
  noteTitle: string | null,
  serverSuffix: string | null = null,
  adapter: WindowTitleAdapter | null = resolveTauri()
): Promise<void> {
  if (!adapter) return;
  const title = formatWindowTitle(vault, noteTitle, serverSuffix);
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
