/**
 * Pure helpers for rendering a permission-request diff preview (issue #189).
 *
 * Kept DOM-free so the diff formatting is unit-testable without mounting a
 * Svelte component. A line is tagged `removed` (from the old text), `added`
 * (from the new text), or `context`. The prompt renders removed lines with a
 * `-` prefix on a danger surface and added lines with a `+` prefix on a success
 * surface, so the change reads correctly without relying on color alone.
 */

import type { DiffPreview } from './types.ts';

export type DiffLineKind = 'added' | 'removed' | 'context';

export interface DiffLine {
	kind: DiffLineKind;
	/** The `+`/`-`/` ` marker shown before the text (accessible without color). */
	marker: string;
	text: string;
}

function splitLines(text: string): string[] {
	// A trailing newline should not produce a spurious empty final line.
	const lines = text.split('\n');
	if (lines.length > 1 && lines[lines.length - 1] === '') lines.pop();
	return lines;
}

/**
 * Format a {@link DiffPreview} into ordered, tagged lines: every old-text line
 * as `removed`, then every new-text line as `added`. A new file (no `oldText`)
 * yields only added lines. This is a simple, accessible block preview — not a
 * minimal line-level diff — which is all the prompt needs for a human glance.
 */
export function formatDiffLines(diff: DiffPreview): DiffLine[] {
	const out: DiffLine[] = [];
	if (diff.oldText != null && diff.oldText.length > 0) {
		for (const text of splitLines(diff.oldText)) {
			out.push({ kind: 'removed', marker: '-', text });
		}
	}
	for (const text of splitLines(diff.newText)) {
		out.push({ kind: 'added', marker: '+', text });
	}
	return out;
}
