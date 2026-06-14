<script lang="ts">
	import type { PermissionEvent } from '$lib/agent/agent-client';
	import type { PermissionDecision } from '$lib/agent/types';

	let {
		request,
		ondecide
	}: {
		request: PermissionEvent;
		ondecide: (decision: PermissionDecision) => void;
	} = $props();
</script>

<div class="permission" role="alertdialog" aria-label="Agent permission request">
	<p class="prompt">
		The agent wants to run <strong>{request.request.tool}</strong>{#if request.request.kind}
			<span class="kind">({request.request.kind})</span>{/if}.
	</p>
	<div class="actions">
		<button class="btn allow" type="button" onclick={() => ondecide('allow_once')}>
			Allow once
		</button>
		<button class="btn allow-always" type="button" onclick={() => ondecide('allow_always')}>
			Allow for session
		</button>
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
