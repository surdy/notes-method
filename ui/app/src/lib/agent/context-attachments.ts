/**
 * Pure, framework-free logic for the chat composer's `@`-mention context
 * attachments (issue #197). Keeping this DOM- and Svelte-free makes mention
 * parsing, attachment bookkeeping, and the outgoing context block trivial to
 * unit-test.
 *
 * Notesmith never runs its own chat LLM (ADR 0015 Option A): the user's ACP
 * agent reaches vault data through MCP tools. So the composer's job is only to
 * ATTACH references (note path, folder path, tag, url) and hand them to the
 * agent in the outgoing turn; the agent then resolves @note/@folder/@tag via its
 * own read/list MCP tools. We never fetch note bodies in the frontend.
 */

/** The kinds of reference a user can attach as context. */
export type AttachmentKind = 'note' | 'folder' | 'tag' | 'url';

/**
 * One attached reference. `value` is the canonical identifier the agent needs
 * (note path, folder path, tag text, or url); `label` is what we show the user.
 */
export interface Attachment {
	kind: AttachmentKind;
	value: string;
	label: string;
}

/** Result of inspecting the composer value/caret for an in-progress `@` token. */
export interface MentionTrigger {
	/** True when an `@…` token is being typed at the caret. */
	active: boolean;
	/** A typed kind (`@folder:` → `'folder'`) or null when none was specified. */
	kind: AttachmentKind | null;
	/** The partial text after `@` (and after `kind:`, if present). */
	query: string;
	/** Index of the `@` in the input, so the caller can replace the token. */
	start: number;
}

const KIND_PREFIXES: Record<string, AttachmentKind> = {
	note: 'note',
	folder: 'folder',
	tag: 'tag',
	url: 'url'
};

/**
 * Detect an in-progress `@`-mention at the caret and extract its query.
 *
 * Rule (kept deliberately simple and unambiguous):
 *   - Scan left from the caret. The token runs back to the nearest `@`.
 *   - Any whitespace between the caret and that `@` ends the token (the input is
 *     then a real message, not a mention) — so `@note done` is inactive.
 *   - The `@` must sit at a word boundary: at input start or right after
 *     whitespace. This rejects email-like `foo@bar`.
 *   - If the token reads `kind:rest` and `kind` is one of note/folder/tag/url,
 *     the kind is reported and `query` is `rest`. Otherwise `kind` is null and
 *     `query` is the whole token (so `@foo:bar` is a plain query).
 *   - A bare `@` yields `{ active: true, kind: null, query: '' }`.
 *
 * `start` is the index of the `@`, letting the caller replace `input[start..caret]`
 * when an item is selected.
 */
export function parseMentionTrigger(input: string, caret: number): MentionTrigger {
	const inactive: MentionTrigger = { active: false, kind: null, query: '', start: caret };
	const pos = Math.max(0, Math.min(caret, input.length));
	inactive.start = pos;

	let i = pos - 1;
	while (i >= 0) {
		const ch = input[i];
		if (ch === '@') break;
		if (/\s/.test(ch)) return inactive;
		i -= 1;
	}
	if (i < 0 || input[i] !== '@') return inactive;

	const start = i;
	const before = start > 0 ? input[start - 1] : '';
	if (before && !/\s/.test(before)) return inactive;

	const token = input.slice(start + 1, pos);
	const colon = token.indexOf(':');
	if (colon >= 0) {
		const maybeKind = token.slice(0, colon).toLowerCase();
		if (Object.prototype.hasOwnProperty.call(KIND_PREFIXES, maybeKind)) {
			return { active: true, kind: KIND_PREFIXES[maybeKind], query: token.slice(colon + 1), start };
		}
	}
	return { active: true, kind: null, query: token, start };
}

function lineFor(a: Attachment): string {
	if (a.kind === 'tag') {
		const tag = a.value.startsWith('#') ? a.value : `#${a.value}`;
		return `- tag: ${tag}`;
	}
	return `- ${a.kind}: ${a.value}`;
}

/**
 * Build the compact, deterministic context block to PREPEND to the outgoing
 * prompt. This is what makes the agent aware of the attached references; it then
 * resolves @note/@folder/@tag through its MCP read/list tools. Returns '' when
 * there are no attachments so the seam can skip prepending entirely.
 */
export function assembleContextText(attachments: Attachment[]): string {
	if (attachments.length === 0) return '';
	const lines = attachments.map(lineFor).join('\n');
	return `[Context]\n${lines}\nUse your read/list tools to fetch referenced notes/folders/tags.`;
}

/** Add an attachment, deduping by kind+value. Returns a new list (no mutation). */
export function addAttachment(list: Attachment[], attachment: Attachment): Attachment[] {
	if (list.some((a) => a.kind === attachment.kind && a.value === attachment.value)) {
		return list.slice();
	}
	return [...list, attachment];
}

/** Remove the attachment matching kind+value. Returns a new list (no mutation). */
export function removeAttachment(
	list: Attachment[],
	kind: AttachmentKind,
	value: string
): Attachment[] {
	return list.filter((a) => !(a.kind === kind && a.value === value));
}

function basename(value: string): string {
	const idx = value.lastIndexOf('/');
	return idx >= 0 ? value.slice(idx + 1) : value;
}

/**
 * Filter candidate attachments by a query (case-insensitive). Items whose
 * basename starts with the query rank above plain substring (label) matches;
 * within each tier the original order is preserved (stable). An empty query
 * returns every candidate in its original order.
 */
export function filterAttachments(candidates: Attachment[], query: string): Attachment[] {
	const q = query.trim().toLowerCase();
	if (!q) return [...candidates];

	const prefix: Attachment[] = [];
	const substring: Attachment[] = [];
	for (const c of candidates) {
		const base = basename(c.label).toLowerCase();
		const label = c.label.toLowerCase();
		if (base.startsWith(q)) prefix.push(c);
		else if (label.includes(q)) substring.push(c);
	}
	return [...prefix, ...substring];
}
