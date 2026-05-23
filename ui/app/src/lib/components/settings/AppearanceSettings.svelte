<script lang="ts">
	import { onDestroy } from 'svelte';

	import type { VaultConfigData } from '$lib/api/config';
	import { themeStore, type ThemeEntry, type ThemeMode, type VisualMode } from '$lib/theme.svelte';
	import { findThemeByName, splitThemesByTone } from '$lib/theme-picker';
	import themeCatalog from '../../../styles/theme-catalog.json';

	let {
		cfg,
		saveImmediate
	}: {
		cfg: VaultConfigData;
		saveImmediate: (section: string) => Promise<void>;
	} = $props();

	const themeOptions: ThemeEntry[] = themeCatalog as ThemeEntry[];
	const { darkThemes, lightThemes } = splitThemesByTone(themeOptions);
	const modeOptions: Array<{ value: ThemeMode; label: string }> = [
		{ value: 'dark', label: 'Dark' },
		{ value: 'light', label: 'Light' },
		{ value: 'system', label: 'System' }
	];
	let previewRestore = $state<(() => void) | null>(null);

	function currentThemeName(): string {
		const theme = cfg?.appearance?.theme;
		return theme === 'dark' || theme === 'light' || theme === 'system' || theme === 'manuscript' || theme === 'hc-dark'
			? themeStore.theme
			: theme ?? themeStore.theme;
	}

	function currentModeValue(): ThemeMode {
		const mode = cfg?.appearance?.mode;
		return mode === 'dark' || mode === 'light' || mode === 'system' ? mode : themeStore.mode;
	}

	function currentVisualModeValue(): VisualMode {
		const visualMode = cfg?.appearance?.visualMode;
		return visualMode === 'default' || visualMode === 'high-contrast'
			? visualMode
			: themeStore.visualMode;
	}

	function updateAppearance(partial: Partial<{ theme: string; mode: ThemeMode; visualMode: VisualMode }>) {
		cfg.appearance = {
			theme: currentThemeName(),
			mode: currentModeValue(),
			visualMode: currentVisualModeValue(),
			...partial
		};
	}

	function selectedThemeEntry(): ThemeEntry | undefined {
		return findThemeByName(themeOptions, currentThemeName());
	}

	function clearPreview() {
		previewRestore?.();
		previewRestore = null;
	}

	function previewTheme(themeName: string) {
		if (previewRestore && currentThemeName() === themeName) {
			clearPreview();
		}
		clearPreview();
		previewRestore = themeStore.preview(themeName);
	}

	async function selectTheme(themeName: string) {
		clearPreview();
		updateAppearance({ theme: themeName });
		themeStore.setTheme(themeName);
		await saveImmediate('appearance');
	}

	async function selectMode(mode: ThemeMode) {
		updateAppearance({ mode });
		themeStore.setMode(mode);
		await saveImmediate('appearance');
	}

	async function setVisualMode(visualMode: VisualMode) {
		updateAppearance({ visualMode });
		themeStore.setVisualMode(visualMode);
		await saveImmediate('appearance');
	}

	$effect(() => {
		if (cfg?.appearance) {
			themeStore.applyFromConfig(cfg.appearance);
		}
	});

	onDestroy(() => {
		clearPreview();
	});
</script>

<section class="section-content appearance-section">
	<h2>Appearance</h2>
	<p class="section-description">
		Choose a catalog theme, tone preference, and optional high-contrast overlay for this vault.
	</p>

	<div class="theme-section">
		<div class="theme-toolbar">
			<div class="mode-stack">
				<h3>Mode</h3>
				<div class="mode-selector" role="group" aria-label="Theme mode">
					{#each modeOptions as option}
						<button
							class="mode-button"
							class:active={currentModeValue() === option.value}
							type="button"
							onclick={() => void selectMode(option.value)}
						>
							{option.label}
						</button>
					{/each}
				</div>
				<p class="field-hint">System follows the operating system color scheme.</p>
			</div>

			<div class="theme-summary">
				<div>
					<span class="summary-label">Current theme</span>
					<strong>{selectedThemeEntry()?.display_name ?? currentThemeName()}</strong>
					{#if selectedThemeEntry()}
						<span class="field-hint">by {selectedThemeEntry()?.author}</span>
					{/if}
				</div>

				<label class="hc-toggle">
					<input
						type="checkbox"
						checked={currentVisualModeValue() === 'high-contrast'}
						onchange={(event) =>
							void setVisualMode(
								(event.currentTarget as HTMLInputElement).checked
									? 'high-contrast'
									: 'default'
							)}
					/>
					<span>High Contrast</span>
				</label>
			</div>
		</div>

		<div class="theme-group">
			<h4>Dark Themes</h4>
			<div class="theme-grid">
				{#each darkThemes as theme}
					<button
						class="theme-card"
						class:active={currentThemeName() === theme.name}
						type="button"
						aria-pressed={currentThemeName() === theme.name}
						onmouseenter={() => previewTheme(theme.name)}
						onmouseleave={clearPreview}
						onfocus={() => previewTheme(theme.name)}
						onblur={clearPreview}
						onclick={() => void selectTheme(theme.name)}
					>
						<div class="theme-card-top">
							<div class="theme-swatch" aria-hidden="true">
								<span class="swatch swatch-bg" style={`background: ${theme.palette.bg}`}></span>
								<span class="swatch swatch-fg" style={`background: ${theme.palette.fg}`}></span>
								<span class="swatch swatch-accent" style={`background: ${theme.palette.blue}`}></span>
								<span class="swatch swatch-red" style={`background: ${theme.palette.red}`}></span>
								<span class="swatch swatch-green" style={`background: ${theme.palette.green}`}></span>
							</div>
							<span class="theme-tone-badge">{theme.tone}</span>
						</div>
						<span class="theme-name">{theme.display_name}</span>
						<span class="theme-meta">{theme.author}</span>
					</button>
				{/each}
			</div>
		</div>

		<div class="theme-group">
			<h4>Light Themes</h4>
			<div class="theme-grid">
				{#each lightThemes as theme}
					<button
						class="theme-card"
						class:active={currentThemeName() === theme.name}
						type="button"
						aria-pressed={currentThemeName() === theme.name}
						onmouseenter={() => previewTheme(theme.name)}
						onmouseleave={clearPreview}
						onfocus={() => previewTheme(theme.name)}
						onblur={clearPreview}
						onclick={() => void selectTheme(theme.name)}
					>
						<div class="theme-card-top">
							<div class="theme-swatch" aria-hidden="true">
								<span class="swatch swatch-bg" style={`background: ${theme.palette.bg}`}></span>
								<span class="swatch swatch-fg" style={`background: ${theme.palette.fg}`}></span>
								<span class="swatch swatch-accent" style={`background: ${theme.palette.blue}`}></span>
								<span class="swatch swatch-red" style={`background: ${theme.palette.red}`}></span>
								<span class="swatch swatch-green" style={`background: ${theme.palette.green}`}></span>
							</div>
							<span class="theme-tone-badge">{theme.tone}</span>
						</div>
						<span class="theme-name">{theme.display_name}</span>
						<span class="theme-meta">{theme.author}</span>
					</button>
				{/each}
			</div>
		</div>
	</div>
</section>

<style>
	.appearance-section {
		max-width: 1120px;
	}

	.theme-section {
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	.theme-toolbar {
		display: flex;
		flex-wrap: wrap;
		justify-content: space-between;
		gap: 20px;
		padding: 18px;
		border: 1px solid var(--border-default);
		border-radius: 16px;
		background: var(--bg-surface);
	}

	.mode-stack,
	.theme-summary {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.mode-stack h3,
	.theme-group h4 {
		margin: 0;
		font-size: 14px;
		font-weight: 600;
		color: var(--text-default);
	}

	.theme-group {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.mode-selector {
		display: inline-flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.mode-button,
	.theme-card,
	.hc-toggle {
		color: var(--text-default);
	}

	.mode-button {
		padding: 8px 14px;
		border: 1px solid var(--border-default);
		border-radius: 999px;
		background: var(--bg-default);
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		transition:
			background-color 120ms ease,
			border-color 120ms ease,
			color 120ms ease,
			transform 120ms ease;
	}

	.mode-button:hover,
	.mode-button:focus-visible {
		background: var(--bg-hover);
		border-color: var(--border-strong);
		outline: none;
	}

	.mode-button.active {
		background: var(--accent-bg);
		border-color: var(--accent);
		color: var(--accent-text);
	}

	.theme-summary {
		align-items: flex-start;
	}

	.summary-label {
		display: block;
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-muted);
	}

	.field-hint,
	.theme-meta {
		font-size: 12px;
		color: var(--text-muted);
	}

	.theme-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
		gap: 14px;
	}

	.theme-card {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-height: 124px;
		padding: 14px;
		border: 1px solid var(--border-default);
		border-radius: 14px;
		background: var(--bg-surface);
		cursor: pointer;
		text-align: left;
		transition:
			transform 120ms ease,
			border-color 120ms ease,
			background-color 120ms ease,
			box-shadow 120ms ease;
	}

	.theme-card:hover,
	.theme-card:focus-visible {
		background: var(--bg-hover);
		border-color: var(--border-strong);
		box-shadow: 0 12px 28px color-mix(in srgb, var(--bg-default) 82%, transparent);
		transform: translateY(-1px);
		outline: none;
	}

	.theme-card.active {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 40%, transparent);
	}

	.theme-card-top {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
	}

	.theme-swatch {
		display: grid;
		grid-template-columns: repeat(5, minmax(0, 1fr));
		gap: 4px;
		flex: 1;
		padding: 6px;
		border: 1px solid var(--border-subtle);
		border-radius: 10px;
		background: var(--bg-default);
	}

	.swatch {
		display: block;
		height: 30px;
		border-radius: 6px;
	}

	.theme-name {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-default);
	}

	.theme-tone-badge {
		flex: 0 0 auto;
		padding: 4px 8px;
		border: 1px solid var(--border-subtle);
		border-radius: 999px;
		background: var(--bg-default);
		font-size: 11px;
		font-weight: 600;
		text-transform: capitalize;
		color: var(--text-muted);
	}

	.hc-toggle {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		border: 1px solid var(--border-default);
		border-radius: 12px;
		background: var(--bg-default);
		font-size: 13px;
		font-weight: 500;
	}

	.hc-toggle input {
		accent-color: var(--accent);
		color: var(--text-default);
	}

	@media (max-width: 720px) {
		.theme-toolbar {
			padding: 16px;
		}

		.theme-grid {
			grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
		}
	}
</style>
