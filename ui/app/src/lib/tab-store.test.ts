import { describe, it, expect } from 'vitest';
import { tabStorageKey } from './tab-storage-key';

describe('tabStorageKey', () => {
  it('returns null when vault is empty (avoids cross-window clobber)', () => {
    expect(tabStorageKey('')).toBeNull();
  });

  it('namespaces by vault name', () => {
    expect(tabStorageKey('work')).toBe('notesmith:tabs:work');
    expect(tabStorageKey('home')).toBe('notesmith:tabs:home');
  });

  it('produces distinct keys for different vaults so they never collide', () => {
    expect(tabStorageKey('work')).not.toBe(tabStorageKey('home'));
  });
});
