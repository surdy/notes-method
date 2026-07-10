import type { NoteSummary } from '$lib/api';

/**
 * Wikilink resolution for `[[target]]` links (UX review: "Link navigation
 * should help create and disambiguate"). Pure + tested so both the editor
 * (ofm-decorations) and the reading view (NoteViewer) share one behavior.
 *
 * Outcomes:
 *  - `path` set            → a confident, unambiguous match; navigate directly.
 *  - `path` null, candidates → ambiguous / no confident match; let the user
 *                            pick an existing note or create a new one.
 *  - `path` null, no candidates → dead link; offer to create the note.
 */
export interface WikilinkResolution {
	/** Confident match path, or null when the link is ambiguous/missing. */
	path: string | null;
	/** Fuzzy alternatives to offer when there is no confident match. */
	candidates: NoteSummary[];
	/** The note name with any `#anchor` stripped (used for labels/creation). */
	name: string;
}

/** Drop a trailing `#heading` anchor and surrounding whitespace. */
export function stripAnchor(target: string): string {
	return target.split('#')[0]?.trim() ?? '';
}

function withoutExt(path: string): string {
	return path.replace(/\.md$/i, '');
}

function basename(path: string): string {
	return withoutExt(path).split('/').pop() ?? '';
}

/**
 * Given an exact-match group, return a confident resolution when it points at
 * a single note, an ambiguous (candidates) resolution when several match, or
 * null when the group is empty (so the caller falls through to the next tier).
 */
function resolveByGroup(group: NoteSummary[], name: string): WikilinkResolution | null {
	if (group.length === 0) return null;
	const byPath = new Map(group.map((n) => [n.path, n]));
	const paths = Array.from(byPath.keys());
	if (paths.length === 1) return { path: paths[0], candidates: [], name };
	return { path: null, candidates: paths.map((p) => byPath.get(p)!), name };
}

/**
 * Split a wikilink name into an optional folder and a title, so a link like
 * `[[Projects/Roadmap]]` creates `Roadmap` inside `Projects`.
 */
export function splitWikilinkTarget(name: string): { folder?: string; title: string } {
	const clean = withoutExt(name).trim();
	const idx = clean.lastIndexOf('/');
	if (idx === -1) return { title: clean };
	const folder = clean.slice(0, idx).trim();
	const title = clean.slice(idx + 1).trim();
	return folder ? { folder, title } : { title };
}

export function resolveWikilink(target: string, notes: NoteSummary[]): WikilinkResolution {
	const name = stripAnchor(target);
	if (!name) return { path: null, candidates: [], name: '' };

	const lower = name.toLowerCase();

	// Path-based tiers are inherently unique — safe to take the first hit.
	const pathMatch =
		notes.find((n) => n.path === name)?.path ??
		notes.find((n) => n.path === `${name}.md`)?.path ??
		notes.find((n) => withoutExt(n.path) === name)?.path ??
		null;
	if (pathMatch) return { path: pathMatch, candidates: [], name };

	// Exact title/basename (case-sensitive). Unique → confident; many → ambiguous.
	const exact = notes.filter((n) => n.title === name || basename(n.path) === name);
	const exactResolution = resolveByGroup(exact, name);
	if (exactResolution) return exactResolution;

	// Case-insensitive title/basename. Unique → confident; many → ambiguous.
	const ciExact = notes.filter(
		(n) => (n.title ?? '').toLowerCase() === lower || basename(n.path).toLowerCase() === lower
	);
	const ciResolution = resolveByGroup(ciExact, name);
	if (ciResolution) return ciResolution;

	// Fuzzy: substring match on basename/title/path (capped).
	const seen = new Set<string>();
	const candidates: NoteSummary[] = [];
	for (const n of notes) {
		if (seen.has(n.path)) continue;
		const hay = `${basename(n.path)} ${n.title ?? ''} ${withoutExt(n.path)}`.toLowerCase();
		if (hay.includes(lower)) {
			candidates.push(n);
			seen.add(n.path);
			if (candidates.length >= 8) break;
		}
	}
	return { path: null, candidates, name };
}
