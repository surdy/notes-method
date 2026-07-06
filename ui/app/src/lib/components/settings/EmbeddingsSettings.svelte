<script lang="ts">
import type { Capabilities, VaultConfigData } from '$lib/api';
import { toggleField, type SaveImmediateFn } from '$lib/settings-helpers';

interface Props {
cfg: VaultConfigData;
capabilities: Capabilities | null;
saveImmediate: SaveImmediateFn;
}

let { cfg, capabilities, saveImmediate }: Props = $props();

let compiledIn = $derived(capabilities?.embeddings?.compiled_in ?? false);
let canEdit = $derived(capabilities?.can_edit_vault_config ?? false);
let model = $derived(capabilities?.embeddings?.model ?? '');
let dim = $derived(capabilities?.embeddings?.dim ?? 0);
</script>

<section class="config-section">
	{#if !compiledIn}
		<div class="notice">
			This server was built <strong>without</strong> embedding support, so semantic search
			is unavailable. Connect to a daemon that runs an embed-enabled build — the
			<code>*-embed</code> container image, or a desktop build with local embeddings
			bundled — to turn it on.
		</div>
		<label class="field field-toggle">
			<span class="field-label">Enable semantic search for this vault</span>
			<input type="checkbox" checked={cfg.embed.enabled} disabled />
		</label>
	{:else}
		<label class="field field-toggle">
			<span class="field-label">Enable semantic search for this vault</span>
			{#if canEdit}
				<input
					type="checkbox"
					{...toggleField(saveImmediate, 'embed', cfg.embed.enabled, (v) => {
						cfg.embed.enabled = v;
					})}
				/>
			{:else}
				<input type="checkbox" checked={cfg.embed.enabled} disabled />
			{/if}
		</label>
		<p class="field-hint field-hint--toggle">
			Embeddings are enabled <strong>per vault</strong>. When on, a background worker
			embeds this vault's notes so <code>vault_search</code> can rank results by meaning,
			not just keywords. The first pass loads the model and may take a few minutes;
			leaving it off costs nothing.
		</p>
		{#if !canEdit}
			<p class="field-hint">
				This connection is read-only, so the toggle reflects the server's current state
				but can't be changed from here.
			</p>
		{/if}
		<div class="subsection">
			<h3 class="subsection-title">Model</h3>
			<p class="subsection-hint">
				<code>{model}</code>{#if dim} · {dim} dimensions{/if}
			</p>
		</div>
	{/if}
</section>

<style>
.config-section {
	padding: 16px 24px;
	max-width: 560px;
}

.notice {
	margin-bottom: 16px;
	padding: 10px 12px;
	border: 1px solid var(--border-default);
	border-radius: 4px;
	background: var(--bg-secondary);
	color: var(--text-muted);
	font-size: 12px;
	line-height: 1.5;
	max-width: 480px;
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

.field-label {
	font-size: 12px;
	color: var(--text-muted);
}

.field-hint {
	font-size: 11px;
	line-height: 1.5;
	color: var(--text-muted);
	max-width: 400px;
}

.field-hint--toggle {
	margin: -4px 0 4px;
}

.subsection {
	margin: 6px 0 14px;
	padding-top: 12px;
	border-top: 1px solid var(--border-default);
}

.subsection-title {
	margin: 0 0 4px;
	font-size: 12px;
	font-weight: 600;
	color: var(--text-default);
}

.subsection-hint {
	margin: 0;
	font-size: 11px;
	line-height: 1.5;
	color: var(--text-muted);
	max-width: 400px;
}

code {
	font-family: var(--font-mono);
	font-size: 10px;
	padding: 1px 4px;
	border-radius: 3px;
	background: var(--bg-secondary);
}

@media (max-width: 600px) {
	.config-section {
		padding: 12px 16px;
	}
}
</style>
