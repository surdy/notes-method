/**
 * Helpers for the title-header rename UX (issue #100).
 *
 * Kept as pure functions so they can be unit-tested without DOM/jsdom.
 */

export function computeStem(notePath: string): string | null {
	if (!notePath) return null;
	const filename = notePath.split('/').pop() ?? '';
	const stem = filename.replace(/\.md$/i, '');
	return stem === '' ? null : stem;
}

export function validateName(name: string): string | null {
	if (!name) return 'Name must not be empty';
	if (/[\\/:*?"<>|]/.test(name)) return 'Name contains invalid characters';
	if (name === '.' || name === '..') return 'Invalid name';
	return null;
}

export function hasFrontmatterTitle(frontmatter: Record<string, unknown> | null | undefined): boolean {
	return (
		typeof frontmatter?.title === 'string' && (frontmatter.title as string).trim() !== ''
	);
}
