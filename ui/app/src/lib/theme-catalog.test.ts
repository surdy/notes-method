import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const themeCatalogPath = fileURLToPath(new URL('../styles/theme-catalog.json', import.meta.url));
const paletteKeys = ['bg', 'fg', 'black', 'red', 'green', 'yellow', 'blue', 'magenta', 'cyan', 'white'];

describe('theme catalog', () => {
it('defines the sample theme engine palette catalog', () => {
const themeCatalog = JSON.parse(readFileSync(themeCatalogPath, 'utf8')) as Array<{
name: string;
display_name: string;
author: string;
tone: 'dark' | 'light';
split_surface: boolean;
palette: Record<string, string>;
tags: string[];
}>;

expect(themeCatalog).toHaveLength(3);
expect(new Set(themeCatalog.map((theme) => theme.name)).size).toBe(themeCatalog.length);
expect(themeCatalog.map((theme) => theme.name)).toEqual([
'tokyo-night',
'catppuccin-latte',
'manuscript'
]);
expect(themeCatalog.some((theme) => theme.name === 'manuscript' && theme.split_surface)).toBe(true);

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
}
});
});
