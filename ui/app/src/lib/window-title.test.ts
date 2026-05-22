import { describe, it, expect, vi } from 'vitest';
import { formatWindowTitle, pushWindowTitle } from './window-title';

describe('formatWindowTitle', () => {
  it('joins vault and note with em dash', () => {
    expect(formatWindowTitle('work', 'Inbox')).toBe('work — Inbox');
  });

  it('falls back to vault alone when note is empty', () => {
    expect(formatWindowTitle('work', '')).toBe('work');
    expect(formatWindowTitle('work', null)).toBe('work');
    expect(formatWindowTitle('work', '   ')).toBe('work');
  });

  it('trims surrounding whitespace', () => {
    expect(formatWindowTitle('  work  ', '  My Note  ')).toBe('work — My Note');
  });

  it('returns empty when both are empty', () => {
    expect(formatWindowTitle('', null)).toBe('');
  });
});

describe('pushWindowTitle', () => {
  it('invokes set_window_title with formatted title', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined);
    await pushWindowTitle('work', 'Note', { invoke });
    expect(invoke).toHaveBeenCalledWith('set_window_title', { title: 'work — Note' });
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
