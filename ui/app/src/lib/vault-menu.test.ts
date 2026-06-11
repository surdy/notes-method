import { describe, expect, it } from 'vitest';

import {
  buildVaultMenuModel,
  isBrowserVaultMenu,
  settingsRoute,
  vaultSwitchUrl
} from './vault-menu.ts';

describe('isBrowserVaultMenu', () => {
  it('is enabled in browser mode (no Tauri bridge)', () => {
    expect(isBrowserVaultMenu(null)).toBe(true);
    expect(isBrowserVaultMenu(undefined)).toBe(true);
  });

  it('is disabled when a Tauri bridge is present (desktop)', () => {
    expect(isBrowserVaultMenu({ invoke: async () => undefined })).toBe(false);
  });
});

describe('buildVaultMenuModel', () => {
  it('marks the current vault and preserves order', () => {
    const model = buildVaultMenuModel({
      vaults: ['learn', 'people', 'projects'],
      currentVault: 'people'
    });

    expect(model.vaults).toEqual([
      { name: 'learn', isCurrent: false },
      { name: 'people', isCurrent: true },
      { name: 'projects', isCurrent: false }
    ]);
    expect(model.hasOtherVaults).toBe(true);
  });

  it('reports no other vaults when only the current vault exists', () => {
    const model = buildVaultMenuModel({ vaults: ['solo'], currentVault: 'solo' });
    expect(model.vaults).toEqual([{ name: 'solo', isCurrent: true }]);
    expect(model.hasOtherVaults).toBe(false);
  });

  it('handles an empty vault list', () => {
    const model = buildVaultMenuModel({ vaults: [], currentVault: '' });
    expect(model.vaults).toEqual([]);
    expect(model.hasOtherVaults).toBe(false);
  });

  it('treats an unknown current vault as not-current for every entry', () => {
    const model = buildVaultMenuModel({
      vaults: ['learn', 'people'],
      currentVault: 'missing'
    });
    expect(model.vaults.every((v) => !v.isCurrent)).toBe(true);
    expect(model.hasOtherVaults).toBe(true);
  });
});

describe('vaultSwitchUrl', () => {
  it('sets the vault query param on the current location', () => {
    expect(vaultSwitchUrl('https://notes.example.com/app/?vault=learn', 'people')).toBe(
      'https://notes.example.com/app/?vault=people'
    );
  });

  it('adds the vault param when none is present', () => {
    expect(vaultSwitchUrl('https://notes.example.com/app/', 'projects')).toBe(
      'https://notes.example.com/app/?vault=projects'
    );
  });

  it('encodes vault names with spaces and special characters', () => {
    expect(vaultSwitchUrl('https://notes.example.com/app/', 'Work Vault')).toBe(
      'https://notes.example.com/app/?vault=Work+Vault'
    );
  });

  it('preserves other query params', () => {
    expect(
      vaultSwitchUrl('https://notes.example.com/app/?foo=bar&vault=learn', 'people')
    ).toBe('https://notes.example.com/app/?foo=bar&vault=people');
  });
});

describe('settingsRoute', () => {
  it('builds the settings route under the base with the vault param', () => {
    expect(settingsRoute('/app', 'projects')).toBe('/app/settings?vault=projects');
  });

  it('encodes vault names with spaces', () => {
    expect(settingsRoute('/app', 'Work Vault')).toBe('/app/settings?vault=Work%20Vault');
  });

  it('omits the vault param when there is no current vault', () => {
    expect(settingsRoute('/app', '')).toBe('/app/settings');
  });

  it('works with an empty base', () => {
    expect(settingsRoute('', 'learn')).toBe('/settings?vault=learn');
  });
});
