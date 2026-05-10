<script lang="ts">
	import { onMount } from 'svelte';
	import FileTree from '$lib/components/FileTree.svelte';
	import NoteViewer from '$lib/components/NoteViewer.svelte';
	import VaultSwitcher from '$lib/components/VaultSwitcher.svelte';
	import { connectSSE } from '$lib/sse';
	import { vaultStore } from '$lib/stores.svelte';

	let vaults = $state<string[]>([]);
	let sseConnection: EventSource | null = null;

	onMount(() => {
		void (async () => {
			try {
				const url = new URL(window.location.href);
				const vault = url.searchParams.get('vault') ?? 'work';
				vaults = [vault];
				vaultStore.currentVault = vault;
				await vaultStore.loadNotes();

				sseConnection = connectSSE(vault, (event) => {
					if (
						event.type.startsWith('note.') ||
						event.type === 'inbox.added' ||
						event.type === 'daily.created'
					) {
						void vaultStore.loadNotes();
					}
				});
			} catch (error) {
				console.error('Failed to initialize Notesmith app shell', error);
			}
		})();

		return () => {
			sseConnection?.close();
		};
	});
</script>

<div class="app-layout">
	<aside class="sidebar">
		<div class="sidebar-header">
			<h1 class="app-title">📝 Notesmith</h1>
		</div>

		{#if vaults.length > 1}
			<VaultSwitcher {vaults} />
		{/if}

		<div class="file-tree-container">
			{#if vaultStore.loading && vaultStore.notes.length === 0}
				<div class="loading-indicator">Loading...</div>
			{:else if vaultStore.error}
				<div class="error-indicator">{vaultStore.error}</div>
			{:else}
				<FileTree node={vaultStore.tree} />
			{/if}
		</div>
	</aside>

	<main class="content-area">
		<NoteViewer />
	</main>
</div>

<style>
	.app-layout {
		display: flex;
		height: 100vh;
		overflow: hidden;
	}

	.sidebar {
		width: 280px;
		min-width: 200px;
		background: var(--sidebar-bg, #252526);
		border-right: 1px solid var(--border-color, #333);
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.sidebar-header {
		padding: 12px 16px;
		border-bottom: 1px solid var(--border-color, #333);
	}

	.app-title {
		margin: 0;
		font-size: 16px;
		font-weight: 600;
	}

	.file-tree-container {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.content-area {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.loading-indicator,
	.error-indicator {
		padding: 16px;
		text-align: center;
		color: var(--text-muted, #888);
	}

	.error-indicator {
		color: #ff6b6b;
	}

	@media (max-width: 768px) {
		.sidebar {
			width: 240px;
		}
	}

	@media (max-width: 480px) {
		.app-layout {
			flex-direction: column;
		}

		.sidebar {
			width: 100%;
			height: 40vh;
			border-right: none;
			border-bottom: 1px solid var(--border-color, #333);
		}
	}
</style>
