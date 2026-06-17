import { describe, it, expect, vi } from 'vitest';
import { formatWindowTitle, pushWindowTitle } from './window-title';

describe('formatWindowTitle', () => {
  it('returns plain Notesmith for a local window (no server suffix)', () => {
    expect(formatWindowTitle('work', 'Inbox')).toBe('Notesmith');
    expect(formatWindowTitle('work', null)).toBe('Notesmith');
    expect(formatWindowTitle('', null)).toBe('Notesmith');
  });

  it('appends the server name for a remote window', () => {
    expect(formatWindowTitle('work', 'Inbox', 'Memory Server')).toBe('Notesmith — Memory Server');
    expect(formatWindowTitle('', null, 'Memory Server')).toBe('Notesmith — Memory Server');
  });

  it('treats null/empty suffix as local', () => {
    expect(formatWindowTitle('work', null, null)).toBe('Notesmith');
    expect(formatWindowTitle('work', null, '')).toBe('Notesmith');
  });
});

describe('pushWindowTitle', () => {
  it('invokes set_window_title with the formatted local title', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    await pushWindowTitle('work', 'Note', null, { invoke });
    expect(invoke).toHaveBeenCalledWith('set_window_title', { title: 'Notesmith' });
  });

  it('invokes set_window_title with the server-suffixed title for a remote window', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    await pushWindowTitle('work', 'Note', 'Memory Server', { invoke });
    expect(invoke).toHaveBeenCalledWith('set_window_title', { title: 'Notesmith — Memory Server' });
  });

  it('is a no-op when adapter is null', async () => {
    await expect(pushWindowTitle('work', 'x', null, null)).resolves.toBeUndefined();
  });

  it('swallows invoke errors', async () => {
    const invoke = vi.fn().mockRejectedValue(new Error('nope'));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    await expect(pushWindowTitle('work', 'x', null, { invoke })).resolves.toBeUndefined();
    warn.mockRestore();
  });
});
