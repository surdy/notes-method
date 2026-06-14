<script lang="ts">
	import { onMount } from 'svelte';
	import { breakGlassStore } from '$lib/agent/break-glass.svelte';

	onMount(() => breakGlassStore.load());
</script>

<section class="config-section">
	<label class="field field-toggle field-toggle-stack">
		<span class="field-label">Allow filesystem &amp; terminal access (break-glass)</span>
		<input
			type="checkbox"
			checked={breakGlassStore.enabled}
			onchange={(e) => breakGlassStore.set(e.currentTarget.checked)}
		/>
		<span class="field-description">
			When off (default), agents can only reach your vault through Notesmith's vetted
			operations. When on, agents may additionally request raw filesystem and terminal
			access, scoped to the active vault. Every write is still permission-gated and is
			blocked entirely in read-only mode. Leave this off unless you trust the agent.
		</span>
	</label>
</section>

<style>
	.config-section {
		padding: 16px 24px;
		max-width: 560px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 14px;
	}

	.field-toggle {
		flex-direction: row;
		align-items: center;
		gap: 10px;
	}

	.field-toggle input[type='checkbox'] {
		order: -1;
		width: 16px;
		height: 16px;
		accent-color: var(--accent-bg);
	}

	.field-toggle .field-label {
		order: 1;
	}

	.field-toggle-stack {
		align-items: flex-start;
	}

	.field-toggle-stack .field-description {
		order: 2;
		margin-left: 26px;
	}

	.field-label {
		font-size: 12px;
		color: var(--text-muted);
	}

	.field-description {
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.4;
		max-width: 420px;
	}
</style>
