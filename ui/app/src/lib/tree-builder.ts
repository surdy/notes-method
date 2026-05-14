import type { NoteSummary } from './api';

export type NoteNode = NoteSummary;

export interface FolderNode {
	name: string;
	path: string;
	children: FolderNode[];
	notes: NoteSummary[];
}

export function buildTree(notes: NoteSummary[]): FolderNode {
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

	return sortTree(root);
}

export function sortTree(node: FolderNode): FolderNode {
	return {
		...node,
		children: [...node.children]
			.map((child) => sortTree(child))
			.sort((left, right) => left.name.localeCompare(right.name)),
		notes: [...node.notes].sort((left, right) => left.path.localeCompare(right.path))
	};
}
