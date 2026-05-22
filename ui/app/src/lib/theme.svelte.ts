export type ThemeChoice = 'dark' | 'light' | 'system' | 'manuscript' | 'hc-dark';

const DARK_MODE_QUERY = '(prefers-color-scheme: dark)';

const THEME_CLASSES: Record<Exclude<ThemeChoice, 'system'>, string> = {
	dark: 'theme-dark',
	light: 'theme-light',
	manuscript: 'theme-manuscript',
	'hc-dark': 'theme-hc-dark'
};

export function isThemeChoice(value: string | null | undefined): value is ThemeChoice {
	return value === 'dark' || value === 'light' || value === 'system' || value === 'manuscript' || value === 'hc-dark';
}

function getSystemTheme(): 'dark' | 'light' {
	if (typeof window === 'undefined') return 'dark';
	return window.matchMedia(DARK_MODE_QUERY).matches ? 'dark' : 'light';
}

function resolveClass(choice: ThemeChoice): string {
	if (choice === 'system') {
		return THEME_CLASSES[getSystemTheme()];
	}

	return THEME_CLASSES[choice];
}

function applyTheme(choice: ThemeChoice): void {
	if (typeof document === 'undefined') return;

	const html = document.documentElement;
	for (const className of Object.values(THEME_CLASSES)) {
		html.classList.remove(className);
	}

	html.classList.add(resolveClass(choice));
}

class ThemeStore {
	current = $state<ThemeChoice>('system');

	constructor() {
		if (typeof window === 'undefined') return;

		const mediaQuery = window.matchMedia(DARK_MODE_QUERY);

		applyTheme(this.current);

		mediaQuery.addEventListener('change', () => {
			if (this.current === 'system') {
				applyTheme('system');
			}
		});
	}

	/** Called when vault config is loaded to apply the vault's theme. */
	applyFromConfig(theme: string): void {
		const choice = isThemeChoice(theme) ? theme : 'system';
		this.current = choice;
		applyTheme(choice);
	}

	set(choice: ThemeChoice): void {
		this.current = choice;
		applyTheme(choice);
	}
}

export const themeStore = new ThemeStore();
