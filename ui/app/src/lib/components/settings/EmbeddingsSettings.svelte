<script lang="ts">
import { onDestroy, onMount } from 'svelte';
import type { Capabilities, EmbeddingStats, VaultConfigData } from '$lib/api';
import { getEmbeddingStats } from '$lib/api';
import { toggleField, type SaveImmediateFn } from '$lib/settings-helpers';
import { deriveEmbeddingStatusView, isIndexingInProgress } from '$lib/embeddings-status';

interface Props {
cfg: VaultConfigData;
capabilities: Capabilities | null;
saveImmediate: SaveImmediateFn;
vault: string;
}

let { cfg, capabilities, saveImmediate, vault }: Props = $props();

let compiledIn = $derived(capabilities?.embeddings?.compiled_in ?? false);
let canEdit = $derived(capabilities?.can_edit_vault_config ?? false);
let model = $derived(capabilities?.embeddings?.model ?? '');
let dim = $derived(capabilities?.embeddings?.dim ?? 0);

const STATS_POLL_MS = 5_000;

let stats = $state<EmbeddingStats | null>(null);
let statsError = $state<string | null>(null);
let indexing = $state(false);
let pollTimer: ReturnType<typeof setInterval> | null = null;
let previousStats: EmbeddingStats | null = null;

let statusView = $derived(deriveEmbeddingStatusView(cfg.embed.enabled, stats, indexing));

async function refreshStats() {
	if (!vault || !cfg.embed.enabled || !compiledIn) return;
	try {
		const next = await getEmbeddingStats(vault);
		indexing = isIndexingInProgress(previousStats, next);
		previousStats = next;
		stats = next;
		statsError = null;
	} catch {
		statsError = 'Could not load embedding index status.';
	}
}

onMount(() => {
	void refreshStats();
	pollTimer = setInterval(() => void refreshStats(), STATS_POLL_MS);
});

onDestroy(() => {
	if (pollTimer) {
		clearInterval(pollTimer);
		pollTimer = null;
	}
});
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
		{#if cfg.embed.enabled}
			<div class="subsection">
				<h3 class="subsection-title">Index status</h3>
				{#if statsError}
					<p class="subsection-hint status-error">{statsError}</p>
				{:else if statusView.state === 'never-indexed'}
					<p class="subsection-hint">
						<span class="status-dot status-dot--pending"></span>
						Not yet indexed. The background worker hasn't embedded this vault yet.
					</p>
				{:else if statusView.state === 'indexing'}
					<p class="subsection-hint">
						<span class="status-dot status-dot--active"></span>
						{#if statusView.determinate && statusView.notesTotal !== null && statusView.notesDone !== null}
							Indexing… <strong
								>{statusView.notesDone.toLocaleString()} / {statusView.notesTotal.toLocaleString()}
								notes</strong
							>
						{:else}
							Indexing… {statusView.vectorCount.toLocaleString()} vectors so far.
						{/if}
					</p>
					{#if statusView.determinate && statusView.percent !== null}
						<div class="progress-bar" role="progressbar" aria-valuenow={statusView.percent} aria-valuemin="0" aria-valuemax="100">
							<span style="width: {statusView.percent}%"></span>
						</div>
						<p class="progress-meta">
							<span>{statusView.percent}%</span>
							<span>{statusView.vectorCount.toLocaleString()} vectors so far</span>
						</p>
					{:else}
						<div class="progress-bar progress-bar--indeterminate"><span></span></div>
						<p class="progress-caption">
							Progress total unavailable from this server — showing activity only.
						</p>
					{/if}
				{:else if statusView.state === 'ready'}
					<p class="subsection-hint">
						<span class="status-dot status-dot--ready"></span>
						{statusView.vectorCount} vectors indexed{#if statusView.lastIndexedLabel}
							· last indexed {statusView.lastIndexedLabel}{/if}
					</p>
					{#if statusView.embedderId}
						<p class="subsection-hint">Embedder: <code>{statusView.embedderId}</code></p>
					{/if}
					{#if statusView.p50Ms !== null && statusView.p95Ms !== null}
						<p class="subsection-hint">
							Search latency: p50 {statusView.p50Ms.toFixed(0)}ms · p95 {statusView.p95Ms.toFixed(
								0
							)}ms
						</p>
					{/if}
				{/if}
			</div>
		{/if}
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

.status-dot {
	display: inline-block;
	width: 8px;
	height: 8px;
	border-radius: 50%;
	margin-right: 6px;
}

.status-dot--pending {
	background: var(--text-muted);
}

.status-dot--active {
	background: var(--accent-bg);
	animation: pulse 1.2s ease-in-out infinite;
}

.status-dot--ready {
	background: var(--color-success);
}

.status-error {
	color: var(--text-muted);
}

@keyframes pulse {
	0%,
	100% {
		opacity: 1;
	}
	50% {
		opacity: 0.35;
	}
}

.progress-bar {
	height: 6px;
	margin: 8px 0 4px;
	border: 1px solid var(--border-default);
	border-radius: 999px;
	background: var(--bg-secondary);
	overflow: hidden;
	max-width: 400px;
}

.progress-bar > span {
	display: block;
	height: 100%;
	border-radius: 999px;
	background: var(--accent-bg);
	transition: width 0.3s ease;
}

.progress-bar--indeterminate > span {
	width: 40%;
	animation: indeterminate 1.2s ease-in-out infinite;
}

@keyframes indeterminate {
	0% {
		margin-left: -40%;
	}
	100% {
		margin-left: 100%;
	}
}

.progress-meta {
	display: flex;
	justify-content: space-between;
	max-width: 400px;
	margin: 0;
	font-size: 10px;
	color: var(--text-muted);
}

.progress-caption {
	margin: 4px 0 0;
	font-size: 10px;
	color: var(--text-muted);
	max-width: 400px;
}

@media (max-width: 600px) {
	.config-section {
		padding: 12px 16px;
	}
}
</style>
