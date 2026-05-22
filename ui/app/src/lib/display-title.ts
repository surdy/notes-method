/**
 * Compute the title to display in app chrome for a note.
 *
 * Identity (the filename) is the canonical reference used by wikilinks,
 * search, and the index. This helper produces the *display* title:
 *
 *   - If frontmatter contains a non-empty string `title`, use that (trimmed).
 *   - Otherwise, use the filename with the `.md` extension stripped.
 *   - If the path is empty or has no basename, return "Untitled".
 *
 * An H1 in the body is NOT promoted to the display title — body content
 * is content, not identity. See ADR / issue #95.
 */
export interface DisplayTitleInput {
	path: string;
	frontmatter?: Record<string, unknown> | null;
}

export function displayTitleFor({ path, frontmatter }: DisplayTitleInput): string {
	const override = frontmatter?.title;
	if (typeof override === 'string') {
		const trimmed = override.trim();
		if (trimmed.length > 0) {
			return trimmed;
		}
	}

	const basename = path.split('/').filter(Boolean).pop();
	if (!basename) {
		return 'Untitled';
	}

	return basename.replace(/\.md$/i, '');
}
