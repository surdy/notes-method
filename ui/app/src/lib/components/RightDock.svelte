<script lang="ts">
	import RightRail from './RightRail.svelte';
	import AgentPanel from './agent/AgentPanel.svelte';
	import {
		dockTabs,
		loadRailTab,
		saveRailTab,
		type DockSegment,
		type DockTabId,
		type RailTab
	} from '$lib/right-dock';
	import { vaultStore } from '$lib/stores.svelte';

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
	// The dock owns the Context sub-tab (Metadata/Links/TOC) so a single unified
	// tab row can drive both the segment and the active context pane.
	let railTab = $state<RailTab>('metadata');

	// Reload the persisted context sub-tab whenever the active vault changes.
	$effect(() => {
		railTab = loadRailTab(vaultStore.currentVault);
	});

	$effect(() => {
		if (segment === 'chat') chatMounted = true;
	});

	const tabs = $derived(dockTabs(segment, railTab));

	function select(next: DockSegment) {
		if (next !== segment) onSegmentChange?.(next);
	}

	function onTab(id: DockTabId) {
		if (id === 'chat') {
			select('chat');
			return;
		}
		railTab = id;
		saveRailTab(vaultStore.currentVault, id);
		select('context');
	}

	export function refresh() {
		railRef?.refresh();
	}
</script>

<div class="dock" class:collapsed>
	<div class="dock-tabrow" role="tablist" aria-label="Right dock">
		{#each tabs as tab (tab.id)}
			<button
				class="dock-tab"
				class:active={tab.active}
				class:chat={tab.kind === 'chat'}
				type="button"
				role="tab"
				aria-selected={tab.active}
				onclick={() => onTab(tab.id)}
			>
				{tab.label}
			</button>
		{/each}
	</div>

	<div class="dock-body">
		<div class="dock-pane" class:hidden={segment !== 'context'}>
			<RightRail
				bind:this={railRef}
				collapsed={collapsed || segment !== 'context'}
				activeTab={railTab}
			/>
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

	.dock-tabrow {
		display: flex;
		gap: 2px;
		padding: 0 6px;
		flex-shrink: 0;
		background: var(--bg-secondary);
		border-left: 1px solid var(--border-default);
		border-bottom: 1px solid var(--border-default);
	}

	.dock-tab {
		padding: 8px 9px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		border-bottom: 2px solid transparent;
		white-space: nowrap;
	}

	.dock-tab:hover {
		color: var(--text-default);
	}

	.dock-tab.active {
		color: var(--text-default);
		border-bottom-color: var(--accent);
	}

	.dock-tab.chat {
		color: var(--accent);
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
