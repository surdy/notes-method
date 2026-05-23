<script lang="ts">
	import { fuzzyFilter } from '$lib/fuzzy';
	import { inputPalette } from '$lib/input-palette.svelte';

	type ListItem = {
		id: string;
		label: string;
		description?: string;
	};

	let inputRef = $state<HTMLInputElement | undefined>();
	let resultsRef = $state<HTMLDivElement | undefined>();
	let textValue = $state('');
	let query = $state('');
	let selectedIndex = $state(0);

	let request = $derived(inputPalette.request);
	let stepIndex = $derived(inputPalette.currentStep);
	let step = $derived(request ? request.steps[stepIndex] : null);
	let isListStep = $derived(step?.mode === 'list');
	let filteredItems = $derived.by(() => {
		if (!step || step.mode !== 'list') {
			return [] as ListItem[];
		}

		if (!query.trim()) {
			return step.items;
		}

		return fuzzyFilter(query, step.items, (item) => `${item.label} ${item.description ?? ''}`).map(
			(match) => match.item
		);
	});
	let stepLabel = $derived.by(() => {
		if (!step) return '';
		if (!request || request.steps.length === 1) {
			return step.label;
		}

		return `Step ${stepIndex + 1} of ${request.steps.length}: ${step.label}`;
	});

	async function submitCurrentValue() {
		if (!step) return;

		try {
			if (step.mode === 'list') {
				const selected = filteredItems[selectedIndex];
				if (!selected) return;
				await inputPalette.submitStep(selected.id);
				return;
			}

			await inputPalette.submitStep(textValue);
		} catch (error) {
			console.error('Input palette request failed', error);
		}
	}

	function handleInput(event: Event) {
		const value = (event.currentTarget as HTMLInputElement).value;
		if (step?.mode === 'list') {
			query = value;
			return;
		}

		textValue = value;
	}

	function handleKeydown(event: KeyboardEvent) {
		if (!step) return;

		switch (event.key) {
			case 'Escape':
				event.preventDefault();
				inputPalette.cancel();
				return;
			case 'ArrowDown':
				if (step.mode !== 'list') return;
				event.preventDefault();
				selectedIndex =
					filteredItems.length === 0
						? 0
						: Math.min(selectedIndex + 1, filteredItems.length - 1);
				return;
			case 'ArrowUp':
				if (step.mode !== 'list') return;
				event.preventDefault();
				selectedIndex = filteredItems.length === 0 ? 0 : Math.max(selectedIndex - 1, 0);
				return;
			case 'Enter':
				event.preventDefault();
				void submitCurrentValue();
				return;
		}
	}

	$effect(() => {
		const currentStep = step;
		if (!currentStep) return;

		selectedIndex = 0;
		query = '';
		textValue = currentStep.mode === 'text' ? currentStep.defaultValue ?? '' : '';
		inputRef?.focus();
	});

	$effect(() => {
		if (selectedIndex >= filteredItems.length) {
			selectedIndex = Math.max(filteredItems.length - 1, 0);
		}
	});

	$effect(() => {
		if (!inputRef) return;
		step;
		inputRef.focus();
	});

	$effect(() => {
		selectedIndex;
		const selected = resultsRef?.querySelector<HTMLElement>(`[data-index="${selectedIndex}"]`);
		selected?.scrollIntoView({ block: 'nearest' });
	});
</script>

{#if request && step}
	<div aria-label={stepLabel} aria-modal="true" class="palette-backdrop" onclick={(event) => event.target === event.currentTarget && inputPalette.cancel()} onkeydown={handleKeydown} role="dialog" tabindex="-1">
		<div class="palette">
			<div class="palette-header">
				<p class="step-label">{stepLabel}</p>
			</div>

			<input bind:this={inputRef} class="palette-input" oninput={handleInput} placeholder={step.placeholder ?? ''} type="text" value={step.mode === 'list' ? query : textValue} />

			<div bind:this={resultsRef} class="palette-results">
				{#if isListStep}
					{#if filteredItems.length === 0}
						<div class="no-results">No matching options</div>
					{:else}
						{#each filteredItems as item, index (item.id)}
							<button class="palette-item" class:selected={index === selectedIndex} data-index={index} onclick={() => {
								selectedIndex = index;
								void submitCurrentValue();
							}} onmouseenter={() => (selectedIndex = index)} type="button">
								<span class="item-body">
									<span class="item-label">{item.label}</span>
									{#if item.description}
										<span class="item-description">{item.description}</span>
									{/if}
								</span>
							</button>
						{/each}
					{/if}
				{:else}
					<div class="text-hint">
						Press <kbd>Enter</kbd> to continue
					</div>
				{/if}
			</div>

			<div class="palette-footer">
				<span class="hint"><kbd>Enter</kbd> confirm</span>
				{#if isListStep}
					<span class="hint"><kbd>↑↓</kbd> navigate</span>
				{/if}
				<span class="hint"><kbd>Esc</kbd> cancel</span>
			</div>
		</div>
	</div>
{/if}

<style>
	.palette-backdrop {
		position: fixed;
		inset: 0;
		background: var(--overlay);
		display: flex;
		justify-content: center;
		align-items: flex-start;
		padding: min(18vh, 140px) 16px 16px;
		z-index: 55;
	}

	.palette {
		width: min(600px, 100%);
		max-height: min(60vh, 720px);
		display: flex;
		flex-direction: column;
		background: var(--bg-panel);
		border: 1px solid var(--border-overlay);
		border-radius: 16px;
		box-shadow: var(--shadow);
		overflow: hidden;
		animation: palette-slide-in 0.18s ease-out;
	}

	.palette-header {
		padding: 16px 20px 8px;
		border-bottom: 1px solid var(--border-overlay);
	}

	.step-label {
		margin: 0;
		font-size: 13px;
		font-weight: 700;
		letter-spacing: 0.04em;
		color: var(--text-muted);
		text-transform: uppercase;
	}

	.palette-input {
		width: 100%;
		padding: 18px 20px;
		border: none;
		outline: none;
		background: var(--bg-panel);
		color: var(--text-default);
		font-size: 17px;
		border-bottom: 1px solid var(--border-overlay);
	}

	.palette-results {
		overflow-y: auto;
		padding: 8px;
	}

	.palette-item {
		width: 100%;
		display: flex;
		align-items: flex-start;
		padding: 12px 14px;
		border: none;
		border-radius: 10px;
		background: transparent;
		color: var(--text-default);
		cursor: pointer;
		text-align: left;
	}

	.palette-item:hover,
	.palette-item.selected {
		background: var(--bg-active);
	}

	.item-body {
		display: flex;
		flex: 1;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}

	.item-label {
		font-size: 14px;
		font-weight: 500;
	}

	.item-description {
		font-size: 12px;
		color: var(--text-secondary);
	}

	.text-hint,
	.no-results {
		padding: 24px;
		text-align: center;
		color: var(--text-muted);
	}

	.palette-footer {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
		padding: 12px 16px;
		border-top: 1px solid var(--border-overlay);
		background: var(--surface-translucent-subtle);
	}

	.hint {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-secondary);
	}

	kbd {
		padding: 3px 8px;
		border-radius: 999px;
		background: var(--kbd-bg);
		border: 1px solid var(--kbd-border);
		color: var(--text-default);
		font-size: 12px;
	}

	@keyframes palette-slide-in {
		from {
			transform: translateY(-12px);
			opacity: 0;
		}

		to {
			transform: translateY(0);
			opacity: 1;
		}
	}
</style>
