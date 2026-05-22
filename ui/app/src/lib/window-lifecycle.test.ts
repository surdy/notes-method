import { describe, expect, it, vi } from 'vitest';
import { attachWindowCloseConfirm, resolveTauri, type TauriAdapter } from './window-lifecycle';

function makeTauri(): {
  adapter: TauriAdapter;
  fire: (payload?: unknown) => Promise<void>;
  invoke: ReturnType<typeof vi.fn>;
  unlisten: ReturnType<typeof vi.fn>;
} {
  let handler: ((payload: unknown) => void) | null = null;
  const unlisten = vi.fn();
  const invoke = vi.fn(async () => undefined);
  const adapter: TauriAdapter = {
    listen: async (_event, h) => {
      handler = h;
      return unlisten;
    },
    invoke
  };
  return {
    adapter,
    invoke,
    unlisten,
    fire: async (payload) => {
      handler?.(payload);
      await Promise.resolve();
      await Promise.resolve();
    }
  };
}

describe('attachWindowCloseConfirm', () => {
  it('invokes confirm_window_close with allow:true when no tabs are dirty', async () => {
    const { adapter, fire, invoke } = makeTauri();
    await attachWindowCloseConfirm({
      hasDirtyWork: () => false,
      confirmDiscard: () => false,
      tauri: adapter
    });

    await fire();

    expect(invoke).toHaveBeenCalledWith('confirm_window_close', { allow: true });
  });

  it('asks the user when tabs are dirty and forwards the answer', async () => {
    const { adapter, fire, invoke } = makeTauri();
    const confirmDiscard = vi.fn(async () => false);

    await attachWindowCloseConfirm({
      hasDirtyWork: () => true,
      confirmDiscard,
      tauri: adapter
    });

    await fire();

    expect(confirmDiscard).toHaveBeenCalled();
    expect(invoke).toHaveBeenCalledWith('confirm_window_close', { allow: false });
  });

  it('invokes with allow:true when user discards dirty work', async () => {
    const { adapter, fire, invoke } = makeTauri();
    await attachWindowCloseConfirm({
      hasDirtyWork: () => true,
      confirmDiscard: () => true,
      tauri: adapter
    });

    await fire();

    expect(invoke).toHaveBeenCalledWith('confirm_window_close', { allow: true });
  });

  it('returns an unlisten teardown function', async () => {
    const { adapter, unlisten } = makeTauri();
    const teardown = await attachWindowCloseConfirm({
      hasDirtyWork: () => false,
      confirmDiscard: () => true,
      tauri: adapter
    });

    teardown();

    expect(unlisten).toHaveBeenCalled();
  });

  it('is a no-op outside of Tauri (no adapter available)', async () => {
    const teardown = await attachWindowCloseConfirm({
      hasDirtyWork: () => true,
      confirmDiscard: () => false,
      tauri: null
    });

    // Should not throw, and should return a callable teardown.
    expect(typeof teardown).toBe('function');
    teardown();
  });

  it('swallows invoke errors without throwing into the event loop', async () => {
    const { adapter, fire, invoke } = makeTauri();
    invoke.mockRejectedValueOnce(new Error('boom'));
    const warnSpy = vi.spyOn(console, 'warn').mockImplementation(() => {});

    await attachWindowCloseConfirm({
      hasDirtyWork: () => false,
      confirmDiscard: () => true,
      tauri: adapter
    });

    await fire();

    expect(warnSpy).toHaveBeenCalled();
    warnSpy.mockRestore();
  });

  it('ignores close requests for a different Tauri window label', async () => {
    const { adapter, fire, invoke } = makeTauri();
    adapter.currentLabel = 'main:alpha-123';

    await attachWindowCloseConfirm({
      hasDirtyWork: () => false,
      confirmDiscard: () => true,
      tauri: adapter
    });

    await fire('main:beta-456');

    expect(invoke).not.toHaveBeenCalled();
  });

  it('handles close requests matching the current Tauri window label', async () => {
    const { adapter, fire, invoke } = makeTauri();
    adapter.currentLabel = 'main:alpha-123';

    await attachWindowCloseConfirm({
      hasDirtyWork: () => false,
      confirmDiscard: () => true,
      tauri: adapter
    });

    await fire('main:alpha-123');

    expect(invoke).toHaveBeenCalledWith('confirm_window_close', { allow: true });
  });
});

describe('resolveTauri', () => {
  it('uses the global listener and exposes the current WebviewWindow label', async () => {
    const globalListen = vi.fn(async () => vi.fn());
    const invoke = vi.fn(async () => undefined);
    vi.stubGlobal('window', {
      __TAURI__: {
        core: { invoke },
        event: { listen: globalListen },
        webviewWindow: {
          getCurrentWebviewWindow: () => ({
            label: 'main:alpha-123'
          })
        }
      }
    });

    const adapter = resolveTauri();
    await adapter?.listen('notesmith://close-requested', () => {});

    expect(adapter?.currentLabel).toBe('main:alpha-123');
    expect(globalListen).toHaveBeenCalledWith(
      'notesmith://close-requested',
      expect.any(Function)
    );

    vi.unstubAllGlobals();
  });
});
