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

	// Feather/Lucide-style icons for each dock tab, keyed by tab id. Rendered
	// above a small text label (variant C) so the row stays compact but the
	// active pane still names itself.
	const TAB_ICONS: Record<DockTabId, string> = {
		metadata:
			'<circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/>',
		links:
			'<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>',
		toc: '<line x1="8" y1="6" x2="21" y2="6"/><line x1="8" y1="12" x2="21" y2="12"/><line x1="8" y1="18" x2="21" y2="18"/><line x1="3" y1="6" x2="3.01" y2="6"/><line x1="3" y1="12" x2="3.01" y2="12"/><line x1="3" y1="18" x2="3.01" y2="18"/>',
		chat: '<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/>'
	};
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
				aria-label={tab.label}
				title={tab.label}
				onclick={() => onTab(tab.id)}
			>
				<svg
					class="dock-tab-icon"
					viewBox="0 0 24 24"
					width="16"
					height="16"
					fill="none"
					stroke="currentColor"
					stroke-width="2"
					stroke-linecap="round"
					stroke-linejoin="round"
					aria-hidden="true"
				>
					<!-- eslint-disable-next-line svelte/no-at-html-tags -->
					{@html TAB_ICONS[tab.id]}
				</svg>
				<span class="dock-tab-label">{tab.label}</span>
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
		border-bottom: 1px solid var(--border-subtle);
	}

	.dock-tab {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 3px;
		padding: 7px 10px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		font-size: 10px;
		font-weight: 600;
		cursor: pointer;
		border-bottom: 2px solid transparent;
		white-space: nowrap;
	}

	.dock-tab-icon {
		display: block;
	}

	.dock-tab-label {
		letter-spacing: 0.01em;
		line-height: 1;
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
