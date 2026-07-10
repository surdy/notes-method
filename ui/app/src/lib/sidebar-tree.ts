import type { FolderNode } from './tree-builder';
import type { NoteSummary } from './api';

/**
 * Pure logic for the sidebar file-tree search/filter and keyboard navigation
 * (top-5 review item #4). Kept separate from the recursive FileTree view so it
 * can be unit-tested.
 */

/** Display label for a note: its title, else the filename without extension. */
export function noteLabel(note: NoteSummary): string {
	if (note.title) return note.title;
	const parts = note.path.split('/');
	return parts[parts.length - 1].replace(/\.md$/, '');
}

/**
 * Prune a folder tree to notes/folders matching `query` (case-insensitive
 * substring). A folder is kept when its own name matches (with its whole
 * subtree), or when it contains matching descendants. Returns `null` when
 * nothing under `node` matches. An empty query returns the node unchanged.
 */
export function filterTree(node: FolderNode, query: string): FolderNode | null {
	const q = query.trim().toLowerCase();
	if (!q) return node;

	if (node.name && node.name.toLowerCase().includes(q)) {
		return node;
	}

	const children = node.children
		.map((child) => filterTree(child, query))
		.filter((child): child is FolderNode => child !== null);

	const notes = node.notes.filter((note) => noteLabel(note).toLowerCase().includes(q));
	const folderNoteMatches = node.folderNote
		? noteLabel(node.folderNote).toLowerCase().includes(q)
		: false;

	if (children.length === 0 && notes.length === 0 && !folderNoteMatches) {
		return null;
	}

	return {
		...node,
		children,
		notes,
		...(folderNoteMatches && node.folderNote ? { folderNote: node.folderNote } : {})
	};
}

/** Total number of notes (including folder notes) contained in the tree. */
export function treeNoteCount(node: FolderNode | null): number {
	if (!node) return 0;
	let count = node.notes.length + (node.folderNote ? 1 : 0);
	for (const child of node.children) {
		count += treeNoteCount(child);
	}
	return count;
}

/** Move an index by `delta` within `[0, count)`, wrapping at both ends. */
export function wrapIndex(count: number, current: number, delta: number): number {
	if (count <= 0) return 0;
	return (((current + delta) % count) + count) % count;
}

/**
 * Typeahead: find the index of the next label (searching forward from
 * `from + 1`, wrapping) whose text starts with `buffer` (case-insensitive).
 * Returns `null` when nothing matches.
 */
export function nextTypeaheadIndex(labels: string[], from: number, buffer: string): number | null {
	const needle = buffer.trim().toLowerCase();
	if (!needle || labels.length === 0) return null;
	for (let offset = 1; offset <= labels.length; offset += 1) {
		const index = (from + offset) % labels.length;
		if (labels[index].toLowerCase().startsWith(needle)) {
			return index;
		}
	}
	return null;
}
