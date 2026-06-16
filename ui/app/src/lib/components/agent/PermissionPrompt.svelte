<script lang="ts">
	import type { PermissionEvent } from '$lib/agent/agent-client';
	import type { PermissionDecision } from '$lib/agent/types';
	import { formatDiffLines } from '$lib/agent/diff-preview';
	import { ALLOW_OPTIONS } from '$lib/agent/permission-options';

	let {
		request,
		ondecide
	}: {
		request: PermissionEvent;
		ondecide: (decision: PermissionDecision) => void;
	} = $props();

	const diff = $derived(request.request.diff ?? null);
	const diffLines = $derived(diff ? formatDiffLines(diff) : []);
</script>

<div class="permission" role="alertdialog" aria-label="Agent permission request">
	<p class="prompt">
		The agent wants to run <strong>{request.request.tool}</strong>{#if request.request.kind}
			<span class="kind">({request.request.kind})</span>{/if}.
	</p>

	{#if diff}
		<div class="diff" aria-label="Proposed change preview">
			<p class="diff-path">{diff.path}</p>
			<pre class="diff-body">{#each diffLines as line (line)}<span class="line {line.kind}"
						><span class="marker" aria-hidden="true">{line.marker}</span>{line.text}
</span>{/each}</pre>
		</div>
	{/if}

	<div class="actions">
		{#each ALLOW_OPTIONS as option (option.decision)}
			<button
				class="btn {option.decision === 'allow_always' ? 'allow-always' : 'allow'}"
				type="button"
				onclick={() => ondecide(option.decision)}
			>
				{option.label}
			</button>
		{/each}
		<button class="btn deny" type="button" onclick={() => ondecide('deny')}> Deny </button>
	</div>
</div>

<style>
	.permission {
		margin: 8px 12px;
		padding: 12px;
		border: 1px solid var(--warning-border);
		border-radius: 8px;
		background: var(--warning-bg);
	}

	.prompt {
		margin: 0 0 10px;
		font-size: 13px;
		line-height: 1.5;
		color: var(--warning-text);
	}

	.kind {
		color: var(--warning-text);
		opacity: 0.8;
	}

	.diff {
		margin: 0 0 10px;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		overflow: hidden;
		background: var(--bg-surface);
	}

	.diff-path {
		margin: 0;
		padding: 6px 10px;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-secondary);
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-default);
	}

	.diff-body {
		margin: 0;
		padding: 0;
		max-height: 220px;
		overflow: auto;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.5;
		white-space: pre;
	}

	.line {
		display: block;
		padding: 0 10px 0 0;
	}

	.marker {
		display: inline-block;
		width: 1.4em;
		padding-left: 6px;
		text-align: center;
		user-select: none;
	}

	.line.removed {
		background: var(--danger-surface);
		color: var(--danger-text);
	}

	.line.added {
		background: var(--success-surface);
		color: var(--success-text);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.btn {
		padding: 6px 12px;
		border-radius: 6px;
		border: 1px solid var(--border-strong);
		background: var(--button-bg);
		color: var(--button-text);
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
	}

	.btn:hover {
		background: var(--button-hover);
	}

	.btn.allow,
	.btn.allow-always {
		border-color: var(--accent-bg);
		background: var(--accent-bg);
		color: var(--accent-text);
	}

	.btn.allow:hover,
	.btn.allow-always:hover {
		background: var(--accent-hover);
	}

	.btn.deny {
		border-color: var(--danger-border);
		background: var(--danger-bg);
		color: var(--danger-text);
	}
</style>
