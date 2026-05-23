<script lang="ts">
import FileTree from './FileTree.svelte';
import type { FolderNode } from '$lib/tree-builder';
import { vaultStore } from '$lib/stores.svelte';

let { folders }: { folders: string[] } = $props();

function findSubtree(root: FolderNode, folderPath: string): FolderNode | null {
const parts = folderPath.split('/');
let current = root;
for (const part of parts) {
const child = current.children.find((candidate) => candidate.name === part);
if (!child) return null;
current = child;
}
return current;
}

let subtrees = $derived(
folders
.map((folder) => ({ folder, node: findSubtree(vaultStore.tree, folder) }))
.filter((entry): entry is { folder: string; node: FolderNode } => entry.node !== null)
);
</script>

<div class="custom-folders-section">
{#each subtrees as { folder, node } (folder)}
<div class="folder-root" title={folder}>
<FileTree {node} depth={0} />
</div>
{/each}
{#if subtrees.length === 0}
<div class="empty">No folders found</div>
{/if}
</div>

<style>
.custom-folders-section {
display: flex;
flex-direction: column;
}

.folder-root {
display: flex;
flex-direction: column;
}

.empty {
padding: 8px 12px;
font-size: 12px;
color: var(--text-muted);
}
</style>
