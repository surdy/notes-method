import type { NoteSummary, WriteNoteResponse } from './api';
import type { FolderNode } from './tree-builder';

type CreateNote = (
	vault: string,
	title: string,
	content: string,
	folder?: string
) => Promise<WriteNoteResponse>;

export type FolderNoteCreationResult = {
	path: string;
	created: boolean;
};

export type FolderRenameResult = {
	from: string;
	to: string;
	folder_note_from: string | null;
	folder_note_to: string | null;
};

export function folderNotePath(folderPath: string): string | null {
	const parts = folderPath.split('/').filter(Boolean);
	if (parts.length === 0 || parts.some((part) => part.startsWith('.'))) {
		return null;
	}

	const folderName = parts[parts.length - 1];
	return `${parts.join('/')}/${folderName}.md`;
}

export function listFolderPickerItems(
	root: FolderNode
): Array<{ id: string; label: string; description?: string }> {
	const items: Array<{ id: string; label: string; description?: string }> = [];

	const visit = (node: FolderNode) => {
		for (const child of node.children) {
			if (folderNotePath(child.path)) {
				items.push({
					id: child.path,
					label: child.name,
					...(child.path === child.name ? {} : { description: child.path })
				});
				visit(child);
			}
		}
	};

	visit(root);
	return items;
}

export function isFolderNoteSelected(node: FolderNode, selectedPath: string | null | undefined): boolean {
	return Boolean(selectedPath && node.folderNote?.path === selectedPath);
}

export function remapPathAfterFolderRename(path: string, rename: FolderRenameResult): string {
	if (rename.folder_note_from && rename.folder_note_to && path === rename.folder_note_from) {
		return rename.folder_note_to;
	}

	const prefix = `${rename.from}/`;
	if (path.startsWith(prefix)) {
		return `${rename.to}/${path.slice(prefix.length)}`;
	}

	return path;
}

export async function createOrOpenFolderNote({
	vault,
	folderPath,
	notes,
	createNote
}: {
	vault: string;
	folderPath: string;
	notes: NoteSummary[];
	createNote: CreateNote;
}): Promise<FolderNoteCreationResult> {
	const path = folderNotePath(folderPath);
	if (!path) {
		throw new Error(`Cannot create a folder note for ${folderPath}`);
	}

	const existing = notes.find((note) => note.path === path);
	if (existing) {
		return { path: existing.path, created: false };
	}

	const folderName = folderPath.split('/').filter(Boolean).at(-1) ?? '';
	const created = await createNote(vault, folderName, `# ${folderName}\n`, folderPath);
	return { path: created.path, created: true };
}
