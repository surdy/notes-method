import { listNotes, type NoteSummary } from './api';
import { clearApiError, reportApiError } from './stores/api-errors.svelte.ts';
import { buildTree, type FolderNode } from './tree-builder';

class VaultStore {
currentVault = $state('');
notes = $state<NoteSummary[]>([]);
loading = $state(false);
error = $state<unknown | null>(null);

get tree(): FolderNode {
return buildTree(this.notes);
}

async loadNotes() {
if (!this.currentVault) return;

this.loading = true;
this.error = null;
clearApiError();
try {
this.notes = await listNotes(this.currentVault);
} catch (error) {
this.error = error;
reportApiError(error, 'list-notes');
} finally {
this.loading = false;
}
}

clearError() {
this.error = null;
clearApiError();
}
}

export const vaultStore = new VaultStore();
export type { FolderNode, NoteNode } from './tree-builder';
