export interface NoteLoadState {
	selectedPath: string | null;
	currentPath: string | null;
	loadingPath: string | null;
}

export function shouldLoadSelectedNote({
	selectedPath,
	currentPath,
	loadingPath
}: NoteLoadState): boolean {
	return Boolean(selectedPath && selectedPath !== currentPath && selectedPath !== loadingPath);
}
