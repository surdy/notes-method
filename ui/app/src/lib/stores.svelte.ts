import { listNotes, type NoteSummary } from './api';
import { buildTree, type FolderNode } from './tree-builder';

class VaultStore {
currentVault = $state('');
notes = $state<NoteSummary[]>([]);
loading = $state(false);
error = $state<string | null>(null);

get tree(): FolderNode {
return buildTree(this.notes);
}

async loadNotes() {
if (!this.currentVault) return;

this.loading = true;
this.error = null;
try {
this.notes = await listNotes(this.currentVault);
} catch (error) {
this.error = error instanceof Error ? error.message : 'Failed to load notes';
} finally {
this.loading = false;
}
}
}

export const vaultStore = new VaultStore();
export type { FolderNode, NoteNode } from './tree-builder';
