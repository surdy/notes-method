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
author: string;
tone: 'dark' | 'light';
split_surface: boolean;
palette: Record<string, string>;
editor_palette?: Record<string, string>;
tags: string[];
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
expect(themeCatalog.find((theme) => theme.name === 'light')?.palette).toMatchObject({
bg: '#ffffff',
fg: '#20242a',
blue: '#356fd6'
});
expect(themeCatalog.find((theme) => theme.name === 'split')?.editor_palette).toMatchObject({
bg: '#fbfbfa',
fg: '#252a31',
blue: '#4f83dc'
});

for (const theme of themeCatalog) {
expect(theme.name).toMatch(/^[a-z0-9-]+$/);
expect(theme.display_name.length).toBeGreaterThan(0);
expect(theme.author.length).toBeGreaterThan(0);
expect(['dark', 'light']).toContain(theme.tone);
expect(typeof theme.split_surface).toBe('boolean');
expect(theme.tags.length).toBeGreaterThan(0);
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
}
}
});
});
