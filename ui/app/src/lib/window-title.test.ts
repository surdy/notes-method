import { describe, it, expect, vi } from 'vitest';
import { formatWindowTitle, pushWindowTitle } from './window-title';

describe('formatWindowTitle', () => {
  it('shows note title with app name', () => {
    expect(formatWindowTitle('work', 'Inbox')).toBe('Inbox — Notesmith');
  });

  it('falls back to vault with app name when note is empty', () => {
    expect(formatWindowTitle('work', '')).toBe('work — Notesmith');
    expect(formatWindowTitle('work', null)).toBe('work — Notesmith');
    expect(formatWindowTitle('work', '   ')).toBe('work — Notesmith');
  });

  it('trims surrounding whitespace', () => {
    expect(formatWindowTitle('  work  ', '  My Note  ')).toBe('My Note — Notesmith');
  });

  it('returns app name when both are empty', () => {
    expect(formatWindowTitle('', null)).toBe('Notesmith');
  });
});

describe('pushWindowTitle', () => {
  it('invokes set_window_title with formatted title', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    await pushWindowTitle('work', 'Note', { invoke });
    expect(invoke).toHaveBeenCalledWith('set_window_title', { title: 'Note — Notesmith' });
  });

  it('is a no-op when adapter is null', async () => {
    await expect(pushWindowTitle('work', 'x', null)).resolves.toBeUndefined();
  });

  it('swallows invoke errors', async () => {
    const invoke = vi.fn().mockRejectedValue(new Error('nope'));
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    await expect(pushWindowTitle('work', 'x', { invoke })).resolves.toBeUndefined();
    warn.mockRestore();
  });
});
