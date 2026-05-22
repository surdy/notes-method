/**
 * Window-lifecycle bridge between the Tauri shell and the SvelteKit webview.
 *
 * The Tauri close handler (see `crates/notesmith-tauri/src/main.rs`) prevents
 * the OS close when the red button is clicked and emits a
 * `notesmith://close-requested` event. The frontend decides whether any
 * tabs are dirty, asks the user to confirm if so, and reports the answer
 * back via the `confirm_window_close` command.
 *
 * When the page is not running inside Tauri (e.g. dev mode in a browser),
 * `attachWindowCloseConfirm` becomes a no-op so the webview still works.
 */

const CLOSE_REQUESTED_EVENT = 'notesmith://close-requested';

export interface CloseConfirmDeps {
  /** Should return true when the user has any unsaved work. */
  hasDirtyWork: () => boolean;
  /** Show a confirmation dialog; resolves to true when the user wants to discard and close. */
  confirmDiscard: () => Promise<boolean> | boolean;
  /** Override for tests. Returns true if `__TAURI__` is available. */
  tauri?: TauriAdapter | null;
}

export interface TauriAdapter {
  currentLabel?: string;
  listen: (event: string, handler: (payload: unknown) => void) => Promise<() => void>;
  invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
}

/**
 * Wire the `notesmith://close-requested` listener and return a teardown fn.
 */
export async function attachWindowCloseConfirm(
  deps: CloseConfirmDeps
): Promise<() => void> {
  const tauri = deps.tauri ?? resolveTauri();
  if (!tauri) {
    return () => {};
  }

  const unlisten = await tauri.listen(CLOSE_REQUESTED_EVENT, async (payload) => {
    if (typeof payload === 'string' && tauri.currentLabel && payload !== tauri.currentLabel) {
      return;
    }
    const allow = deps.hasDirtyWork() ? await deps.confirmDiscard() : true;
    try {
      await tauri.invoke('confirm_window_close', { allow });
    } catch (error) {
      console.warn('confirm_window_close failed', error);
    }
  });

  return unlisten;
}

export function resolveTauri(): TauriAdapter | null {
  if (typeof window === 'undefined') {
    return null;
  }
  const t = (window as unknown as { __TAURI__?: TauriRuntime }).__TAURI__;
  if (!t?.core?.invoke) {
    return null;
  }
  const invoke = t.core.invoke;

  if (!t.event?.listen) {
    return null;
  }

  const currentLabel =
    t.webviewWindow?.getCurrentWebviewWindow?.().label ??
    t.window?.getCurrentWindow?.().label ??
    (window as unknown as { __TAURI_INTERNALS__?: TauriInternals }).__TAURI_INTERNALS__?.metadata
      ?.currentWindow?.label;

  return {
    currentLabel,
    listen: (event, handler) =>
      t.event!.listen(event, (envelope: { payload: unknown }) => handler(envelope.payload)),
    invoke: (cmd, args) => invoke(cmd, args ?? {})
  };
}

interface TauriRuntime {
  event?: {
    listen: (
      event: string,
      handler: (envelope: { payload: unknown }) => void
    ) => Promise<() => void>;
  };
  core?: {
    invoke: (cmd: string, args: Record<string, unknown>) => Promise<unknown>;
  };
  window?: {
    getCurrentWindow?: () => TauriCurrentTarget;
  };
  webviewWindow?: {
    getCurrentWebviewWindow?: () => TauriCurrentTarget;
  };
}

interface TauriCurrentTarget {
  label?: string;
}

interface TauriInternals {
  metadata?: {
    currentWindow?: {
      label?: string;
    };
  };
}
