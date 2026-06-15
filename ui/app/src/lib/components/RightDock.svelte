<script lang="ts">
	import RightRail from './RightRail.svelte';
	import AgentPanel from './agent/AgentPanel.svelte';
	import type { DockSegment } from '$lib/right-dock';

	let {
		segment = 'context',
		collapsed = false,
		onSegmentChange
	}: {
		segment?: DockSegment;
		collapsed?: boolean;
		onSegmentChange?: (segment: DockSegment) => void;
	} = $props();

	// Both panes stay mounted so the chat session and context loads survive
	// segment switches; only visibility changes. Chat is mounted lazily on first
	// activation so the external agent process isn't spawned until the user opens
	// it — then it stays mounted to preserve the live session.
	let railRef = $state<{ refresh: () => void } | null>(null);
	let chatMounted = $state(false);

	$effect(() => {
		if (segment === 'chat') chatMounted = true;
	});

	function select(next: DockSegment) {
		if (next !== segment) onSegmentChange?.(next);
	}

	export function refresh() {
		railRef?.refresh();
	}
</script>

<div class="dock" class:collapsed>
	<div class="dock-segments" role="tablist" aria-label="Right dock">
		<button
			class="dock-segment"
			class:active={segment === 'context'}
			type="button"
			role="tab"
			aria-selected={segment === 'context'}
			onclick={() => select('context')}
		>
			Context
		</button>
		<button
			class="dock-segment"
			class:active={segment === 'chat'}
			type="button"
			role="tab"
			aria-selected={segment === 'chat'}
			onclick={() => select('chat')}
		>
			Chat
		</button>
	</div>

	<div class="dock-body">
		<div class="dock-pane" class:hidden={segment !== 'context'}>
			<RightRail bind:this={railRef} collapsed={collapsed || segment !== 'context'} />
		</div>
		<div class="dock-pane" class:hidden={segment !== 'chat'}>
			{#if chatMounted}
				<AgentPanel collapsed={collapsed || segment !== 'chat'} />
			{/if}
		</div>
	</div>
</div>

<style>
	.dock {
		display: flex;
		flex-direction: column;
		width: 100%;
		height: 100%;
		overflow: hidden;
	}

	.dock.collapsed {
		visibility: hidden;
		pointer-events: none;
	}

	.dock-segments {
		display: flex;
		flex-shrink: 0;
		background: var(--bg-secondary);
		border-left: 1px solid var(--border-default);
		border-bottom: 1px solid var(--border-default);
	}

	.dock-segment {
		flex: 1;
		padding: 9px 4px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		border-bottom: 2px solid transparent;
		text-transform: uppercase;
		letter-spacing: 0.03em;
	}

	.dock-segment:hover {
		color: var(--text-default);
		background: var(--bg-hover);
	}

	.dock-segment.active {
		color: var(--accent);
		border-bottom-color: var(--accent);
	}

	.dock-body {
		position: relative;
		flex: 1;
		min-height: 0;
	}

	.dock-pane {
		height: 100%;
	}

	.dock-pane.hidden {
		display: none;
	}
</style>
