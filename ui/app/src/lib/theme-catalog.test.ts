import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const themeCatalogPath = fileURLToPath(new URL('../styles/theme-catalog.json', import.meta.url));
const paletteKeys = ['bg', 'fg', 'black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'];

describe('theme catalog', () => {
it('defines exactly the three selected Notesmith themes', () => {
const themeCatalog = JSON.parse(readFileSync(themeCatalogPath, 'utf8')) as Array<{
name: string;
display_name: string;
description: string;
tone: 'dark' | 'light';
split_surface: boolean;
palette: Record<string, string>;
editor_palette?: Record<string, string>;
semantic?: Record<string, string>;
editor_semantic?: Record<string, string>;
}>;

expect(themeCatalog.map(({ name, display_name, tone, split_surface }) => ({
name,
display_name,
tone,
split_surface
}))).toEqual([
{ name: 'dark', display_name: 'Dark', tone: 'dark', split_surface: false },
{ name: 'light', display_name: 'Light', tone: 'light', split_surface: false },
{ name: 'split', display_name: 'Split', tone: 'dark', split_surface: true }
]);
expect(new Set(themeCatalog.map((theme) => theme.name)).size).toBe(themeCatalog.length);
expect(themeCatalog.find((theme) => theme.name === 'dark')?.palette).toMatchObject({
bg: '#111316',
fg: '#f0f2f4',
blue: '#79a7ff'
});
expect(themeCatalog.find((theme) => theme.name === 'dark')?.semantic).toMatchObject({
bg_secondary: '#15171a',
bg_elevated: '#1b1e23',
bg_panel: '#17191d',
border_default: '#2b2f35'
});
expect(themeCatalog.find((theme) => theme.name === 'light')?.palette).toMatchObject({
bg: '#ffffff',
fg: '#20242a',
blue: '#356fd6'
});
expect(themeCatalog.find((theme) => theme.name === 'light')?.semantic).toMatchObject({
bg_secondary: '#f7f8fa',
bg_elevated: '#f4f5f7',
bg_panel: '#f4f5f7',
border_default: '#d9dde3'
});
expect(themeCatalog.find((theme) => theme.name === 'split')?.editor_palette).toMatchObject({
bg: '#fbfbfa',
fg: '#252a31',
blue: '#4f83dc'
});
expect(themeCatalog.find((theme) => theme.name === 'split')?.semantic).toMatchObject({
bg_secondary: '#1d2024',
bg_elevated: '#272b31',
bg_panel: '#22252a',
border_default: '#363b43'
});
expect(themeCatalog.find((theme) => theme.name === 'split')?.editor_semantic).toMatchObject({
bg_secondary: '#f3f4f5',
bg_elevated: '#f3f4f5',
bg_panel: '#fbfbfa',
border_default: '#d9dde1'
});

for (const theme of themeCatalog) {
expect(theme.name).toMatch(/^[a-z0-9-]+$/);
expect(theme.display_name.length).toBeGreaterThan(0);
expect(theme.description.length).toBeGreaterThan(0);
expect(['dark', 'light']).toContain(theme.tone);
expect(typeof theme.split_surface).toBe('boolean');
expect(Object.keys(theme.palette).sort()).toEqual([...paletteKeys].sort());

for (const key of paletteKeys) {
expect(theme.palette[key]).toMatch(/^#[0-9a-fA-F]{6}$/);
}

if (theme.split_surface) {
expect(Object.keys(theme.editor_palette ?? {}).sort()).toEqual([...paletteKeys].sort());
for (const key of paletteKeys) {
expect(theme.editor_palette?.[key]).toMatch(/^#[0-9a-fA-F]{6}$/);
}
} else {
expect(theme.editor_palette).toBeUndefined();
expect(theme.editor_semantic).toBeUndefined();
}

expect(Object.keys(theme.semantic ?? {}).sort()).toEqual([
'bg_active',
'bg_default',
'bg_elevated',
'bg_hover',
'bg_input',
'bg_panel',
'bg_secondary',
'bg_surface',
'border_default',
'border_input',
'border_strong',
'border_subtle'
]);
}
});
});
