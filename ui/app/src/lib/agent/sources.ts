/**
 * Pure helpers for the per-message "Show Sources" affordance (issue #242).
 *
 * A message's *sources* are the vault notes an agent used to ground its answer:
 * the notes returned by the `vault_search` MCP tool and the notes read via
 * `get_note` for that turn. This is deliberately **vault-only** — web grounding
 * (from web fetch/search tools) is surfaced as inline citations in the message
 * body and never feeds this control (see the issue's decision/non-goals).
 *
 * Keeping extraction pure and framework-free makes the notes-only / web-only /
 * mixed / none matrix exhaustively unit-testable without a DOM.
 */

/** A single vault note that grounded an assistant message. */
export interface NoteSource {
	/** Vault-relative note path, e.g. `people/Acme.md`. */
	path: string;
	/** Display title when known; falls back to the path in the UI. */
	title: string | null;
	/** Retrieval score (higher is better) when the tool provides one. */
	score: number | null;
	/** A short matching excerpt when the tool provides one. */
	snippet: string | null;
	/** 1-based lexical rank, when `vault_search` ranked it lexically. */
	lexicalRank: number | null;
	/** 1-based semantic rank, when `vault_search` ranked it semantically. */
	semanticRank: number | null;
}

/**
 * Whether a tool call, identified by the (possibly namespaced) tool name,
 * grounds a message in vault notes.
 *
 * ACP clients namespace MCP tools per server (e.g.
 * `notesmith-<vault>-vault_search`), so we match by suffix/substring rather
 * than exact equality. Returns the grounding shape to expect in the result:
 * `'search'` for search-hit arrays, `'note'` for a single read note, or `null`
 * when the tool is not a vault-grounding tool.
 */
export function groundingKind(name: string): 'search' | 'note' | null {
	const n = name.toLowerCase();
	if (n.includes('vault_search') || n.includes('search_notes')) return 'search';
	if (n.includes('get_note')) return 'note';
	return null;
}

/** A note-like path is vault-relative and carries no URL scheme. */
function isNotePath(value: unknown): value is string {
	return typeof value === 'string' && value.length > 0 && !value.includes('://');
}

function asString(value: unknown): string | null {
	return typeof value === 'string' && value.length > 0 ? value : null;
}

function asNumber(value: unknown): number | null {
	return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

/**
 * Parse a tool result's text content as JSON, tolerating non-JSON (returns
 * `null`). Tool results are agent-mediated strings, so this must never throw.
 */
function parseJson(content: string): unknown {
	try {
		return JSON.parse(content);
	} catch {
		return null;
	}
}

/** Map one `vault_search` hit object to a {@link NoteSource}, or `null`. */
function hitToSource(raw: unknown): NoteSource | null {
	if (raw === null || typeof raw !== 'object') return null;
	const obj = raw as Record<string, unknown>;
	// A URL-bearing result is a web hit — deliberately excluded.
	if ('url' in obj) return null;
	if (!isNotePath(obj.path)) return null;
	return {
		path: obj.path,
		title: asString(obj.title),
		score: asNumber(obj.score),
		snippet: asString(obj.snippet),
		lexicalRank: asNumber(obj.lexical_rank),
		semanticRank: asNumber(obj.semantic_rank)
	};
}

/**
 * Heuristic shape match for a `vault_search` hit array, used when the tool name
 * is unrecognized (agents may title MCP calls arbitrarily). A hit array is an
 * array of objects each carrying a note-like `path`, a numeric `score`, and a
 * `snippet` — the distinctive `HybridHit` shape, which `list_notes` /
 * `vault_stats` / web results do not have.
 */
function looksLikeSearchHits(value: unknown): value is unknown[] {
	if (!Array.isArray(value) || value.length === 0) return false;
	return value.every((item) => {
		if (item === null || typeof item !== 'object') return false;
		const obj = item as Record<string, unknown>;
		return (
			isNotePath(obj.path) &&
			typeof obj.score === 'number' &&
			typeof obj.snippet === 'string' &&
			!('url' in obj)
		);
	});
}

/** Map a `get_note` result object to a single {@link NoteSource}, or `null`. */
function noteToSource(value: unknown): NoteSource | null {
	if (value === null || typeof value !== 'object' || Array.isArray(value)) return null;
	const obj = value as Record<string, unknown>;
	if (!isNotePath(obj.path)) return null;
	// `get_note` always returns note content; the guard keeps this from matching
	// unrelated single-object tool results.
	if (typeof obj.content !== 'string') return null;
	const fm = obj.frontmatter;
	const title =
		fm !== null && typeof fm === 'object'
			? asString((fm as Record<string, unknown>).title)
			: null;
	return { path: obj.path, title, score: null, snippet: null, lexicalRank: null, semanticRank: null };
}

/**
 * Extract the vault note sources a single tool call contributes to its turn.
 *
 * Returns `[]` for non-grounding tools (including web tools) and for malformed
 * or error results. `name` is the tool's (possibly namespaced) name; `content`
 * is the tool result's text; `isError` suppresses extraction from failures.
 */
export function extractNoteSources(
	name: string,
	content: string,
	isError = false
): NoteSource[] {
	if (isError || !content) return [];
	const kind = groundingKind(name);
	const parsed = parseJson(content);
	if (parsed === null) return [];

	if (kind === 'note') {
		const source = noteToSource(parsed);
		return source ? [source] : [];
	}

	if (kind === 'search') {
		if (!Array.isArray(parsed)) return [];
		return parsed.map(hitToSource).filter((s): s is NoteSource => s !== null);
	}

	// Unknown tool name: fall back to shape detection, but only for the
	// distinctive search-hit array so we never mistake list_notes / web results
	// for grounding.
	if (looksLikeSearchHits(parsed)) {
		return parsed.map(hitToSource).filter((s): s is NoteSource => s !== null);
	}
	return [];
}

/**
 * Merge a new batch of sources into an accumulator, de-duplicating by path and
 * keeping the richest entry (highest score, then any non-null field). Order of
 * first appearance is otherwise preserved; call {@link sortSources} for display.
 */
export function mergeSources(existing: NoteSource[], incoming: NoteSource[]): NoteSource[] {
	const byPath = new Map<string, NoteSource>();
	for (const source of [...existing, ...incoming]) {
		const prior = byPath.get(source.path);
		if (!prior) {
			byPath.set(source.path, source);
			continue;
		}
		byPath.set(source.path, {
			path: source.path,
			title: prior.title ?? source.title,
			score: pickScore(prior.score, source.score),
			snippet: prior.snippet ?? source.snippet,
			lexicalRank: prior.lexicalRank ?? source.lexicalRank,
			semanticRank: prior.semanticRank ?? source.semanticRank
		});
	}
	return [...byPath.values()];
}

function pickScore(a: number | null, b: number | null): number | null {
	if (a === null) return b;
	if (b === null) return a;
	return Math.max(a, b);
}

/**
 * Sort sources for display: highest score first, scored before unscored, then
 * alphabetically by path so the order is stable.
 */
export function sortSources(sources: NoteSource[]): NoteSource[] {
	return [...sources].sort((a, b) => {
		if (a.score !== null && b.score !== null && a.score !== b.score) return b.score - a.score;
		if (a.score !== null && b.score === null) return -1;
		if (a.score === null && b.score !== null) return 1;
		return a.path.localeCompare(b.path);
	});
}
