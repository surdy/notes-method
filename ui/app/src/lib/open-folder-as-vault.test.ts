import { describe, it, expect } from 'vitest';
import { validateVaultName, defaultNameFromPath, vaultRegistrationMode } from './open-folder-as-vault';

describe('validateVaultName', () => {
  it('accepts a clean unique name', () => {
    expect(validateVaultName('Personal', ['Work'])).toEqual({
      ok: true,
      value: 'Personal'
    });
  });

  it('trims whitespace', () => {
    expect(validateVaultName('  Notes  ', [])).toEqual({
      ok: true,
      value: 'Notes'
    });
  });

  it('rejects empty / whitespace-only', () => {
    const r = validateVaultName('   ', []);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.message).toMatch(/empty/i);
  });

  it('rejects names starting with a dot', () => {
    const r = validateVaultName('.hidden', []);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.message).toMatch(/dot/i);
  });

  it('rejects path separators', () => {
    expect(validateVaultName('a/b', []).ok).toBe(false);
    expect(validateVaultName('a\\b', []).ok).toBe(false);
  });

  it('rejects duplicates (case-sensitive, matching backend)', () => {
    const r = validateVaultName('Work', ['Work', 'Home']);
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error.message).toMatch(/already exists/i);
  });

  it('allows distinct case (case-sensitive)', () => {
    expect(validateVaultName('work', ['Work']).ok).toBe(true);
  });
});

describe('defaultNameFromPath', () => {
  it('uses the last path segment', () => {
    expect(defaultNameFromPath('/Users/x/Notes')).toBe('Notes');
    expect(defaultNameFromPath('C:\\Users\\x\\Notes')).toBe('Notes');
  });

  it('strips trailing slashes', () => {
    expect(defaultNameFromPath('/Users/x/Notes/')).toBe('Notes');
    expect(defaultNameFromPath('/Users/x/Notes///')).toBe('Notes');
  });

  it('strips leading dots from hidden folders', () => {
    expect(defaultNameFromPath('/Users/x/.vault')).toBe('vault');
  });

  it('returns empty when path is just separators', () => {
    expect(defaultNameFromPath('///')).toBe('');
  });
});

describe('vaultRegistrationMode', () => {
  it('uses local registration when no API base is configured', () => {
    expect(vaultRegistrationMode('')).toBe('local');
  });

  it('uses remote registration when an API base is configured', () => {
    expect(vaultRegistrationMode('https://notesmith.clusterfault.com')).toBe('remote');
  });
});
