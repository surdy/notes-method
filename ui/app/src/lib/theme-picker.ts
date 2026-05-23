import { fuzzyFilter } from './fuzzy';
import type { ThemeEntry } from './theme.svelte';

export function splitThemesByTone(themes: ThemeEntry[]): {
	darkThemes: ThemeEntry[];
	lightThemes: ThemeEntry[];
} {
	return {
		darkThemes: themes.filter((theme) => theme.tone === 'dark'),
		lightThemes: themes.filter((theme) => theme.tone === 'light')
	};
}

export function filterThemes(themes: ThemeEntry[], query: string): ThemeEntry[] {
	const trimmedQuery = query.trim();
	if (!trimmedQuery) return themes;

	return fuzzyFilter(
		trimmedQuery,
		themes,
		(theme) =>
			[
				theme.display_name,
				theme.name,
				theme.author,
				theme.tone,
				...theme.tags
			].join(' ')
	).map((match) => match.item);
}

export function findThemeByName(themes: ThemeEntry[], themeName: string): ThemeEntry | undefined {
	return themes.find((theme) => theme.name === themeName);
}
