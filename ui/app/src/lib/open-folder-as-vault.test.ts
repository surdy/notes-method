import { describe, it, expect } from 'vitest';
import {
  validateVaultName,
  defaultNameFromPath,
  vaultRegistrationMode,
  shouldUseNativeVaultRegistration,
  messageFromUnknownError,
  type TauriBridge
} from './open-folder-as-vault';

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

describe('shouldUseNativeVaultRegistration', () => {
  const bridge: TauriBridge = { invoke: async () => undefined };

  it('uses native registration for remote desktop vault writes', () => {
    expect(shouldUseNativeVaultRegistration('https://notesmith.clusterfault.com', bridge)).toBe(
      true
    );
  });

  it('uses fetch registration for remote browser vault writes', () => {
    expect(shouldUseNativeVaultRegistration('https://notesmith.clusterfault.com', null)).toBe(
      false
    );
  });

  it('keeps local desktop registration on the existing local command flow', () => {
    expect(shouldUseNativeVaultRegistration('', bridge)).toBe(false);
  });
});

describe('messageFromUnknownError', () => {
  it('surfaces string rejections from Tauri commands', () => {
    expect(messageFromUnknownError('Path does not exist', 'fallback')).toBe('Path does not exist');
  });

  it('surfaces Error messages', () => {
    expect(messageFromUnknownError(new Error('Network failed'), 'fallback')).toBe('Network failed');
  });

  it('uses the fallback for unknown rejection shapes', () => {
    expect(messageFromUnknownError({ message: 'not trusted' }, 'fallback')).toBe('fallback');
  });
});
