import { describe, expect, it } from 'vitest';

import type { ThemeEntry } from './theme.svelte';
import { filterThemes, splitThemesByTone } from './theme-picker';

const themes: ThemeEntry[] = [
	{
		name: 'tokyo-night',
		display_name: 'Tokyo Night',
		author: 'enkia',
		tone: 'dark',
		split_surface: false,
		palette: {
			bg: '#1a1b26',
			fg: '#c0caf5',
			black: '#15161e',
			red: '#f7768e',
			green: '#9ece6a',
			yellow: '#e0af68',
			blue: '#7aa2f7',
			magenta: '#bb9af7',
			cyan: '#7dcfff',
			white: '#a9b1d6'
		},
		tags: ['cool', 'vibrant']
	},
	{
		name: 'manuscript',
		display_name: 'Manuscript',
		author: 'Notesmith',
		tone: 'light',
		split_surface: true,
		palette: {
			bg: '#f7f1e3',
			fg: '#3b2f2f',
			black: '#2e2424',
			red: '#c85a54',
			green: '#6a8f5b',
			yellow: '#d3a95c',
			blue: '#5b7fa3',
			magenta: '#9b6aa0',
			cyan: '#5f9ea0',
			white: '#faf6ed'
		},
		tags: ['paper', 'warm']
	},
	{
		name: 'github-light',
		display_name: 'GitHub Light',
		author: 'GitHub',
		tone: 'light',
		split_surface: false,
		palette: {
			bg: '#ffffff',
			fg: '#24292f',
			black: '#1f2328',
			red: '#cf222e',
			green: '#1a7f37',
			yellow: '#9a6700',
			blue: '#0969da',
			magenta: '#8250df',
			cyan: '#1b7c83',
			white: '#f6f8fa'
		},
		tags: ['clean', 'neutral']
	}
];

describe('theme picker helpers', () => {
	it('splits themes into dark and light groups while preserving order', () => {
		const grouped = splitThemesByTone(themes);

		expect(grouped.darkThemes.map((theme) => theme.name)).toEqual(['tokyo-night']);
		expect(grouped.lightThemes.map((theme) => theme.name)).toEqual([
			'manuscript',
			'github-light'
		]);
	});

	it('filters themes by display name, tone, author, and tags', () => {
		expect(filterThemes(themes, 'paper').map((theme) => theme.name)).toEqual(['manuscript']);
		expect(filterThemes(themes, 'github').map((theme) => theme.name)).toEqual(['github-light']);
		expect(filterThemes(themes, 'dark').map((theme) => theme.name)).toEqual(['tokyo-night']);
		expect(filterThemes(themes, 'enkia').map((theme) => theme.name)).toEqual(['tokyo-night']);
	});
});
