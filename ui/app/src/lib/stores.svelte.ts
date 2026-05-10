import { listNotes, type NoteSummary } from './api';

export interface FolderNode {
	name: string;
	path: string;
	children: FolderNode[];
	notes: NoteSummary[];
}

class VaultStore {
	currentVault = $state('');
	notes = $state<NoteSummary[]>([]);
	selectedPath = $state<string | null>(null);
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

	selectNote(path: string) {
		this.selectedPath = path;
	}
}

function buildTree(notes: NoteSummary[]): FolderNode {
	const root: FolderNode = { name: '', path: '', children: [], notes: [] };

	for (const note of notes) {
		const parts = note.path.split('/');
		let current = root;

		for (let index = 0; index < parts.length - 1; index += 1) {
			const folderName = parts[index];
			const folderPath = parts.slice(0, index + 1).join('/');
			let child = current.children.find((candidate) => candidate.name === folderName);
			if (!child) {
				child = { name: folderName, path: folderPath, children: [], notes: [] };
				current.children.push(child);
			}
			current = child;
		}

		current.notes.push(note);
	}

	sortTree(root);
	return root;
}

function sortTree(node: FolderNode) {
	node.children.sort((left, right) => left.name.localeCompare(right.name));
	node.notes.sort((left, right) => left.path.localeCompare(right.path));
	node.children.forEach(sortTree);
}

export const vaultStore = new VaultStore();
