/**
 * Helpers for hiding the leading H1 of a note when it duplicates the
 * note's displayed title (filename or frontmatter `title:` override).
 *
 * Per issue #97 / ADR for display-title: a body H1 is just content, not
 * a title source. When the first body element is an H1 whose plain text
 * equals the chrome title, we hide it so the user doesn't see two
 * stacked titles.
 */

const FIRST_H1_RE = /^\s*<h1(?:\s[^>]*)?>([\s\S]*?)<\/h1>/i;

function stripInlineFormatting(html: string): string {
	// Remove all tags and decode the most common entities, leaving plain text.
	const text = html.replace(/<[^>]+>/g, '');
	return text
		.replace(/&amp;/g, '&')
		.replace(/&lt;/g, '<')
		.replace(/&gt;/g, '>')
		.replace(/&quot;/g, '"')
		.replace(/&#39;/g, "'")
		.trim();
}

function normalizeForCompare(value: string): string {
	return value.trim().toLowerCase();
}

/**
 * Returns true if the very first block-level element in `html` is an
 * `<h1>` whose plain text content matches `title` (case-insensitive,
 * whitespace-trimmed, inline formatting stripped).
 */
export function firstH1MatchesTitle(html: string, title: string): boolean {
	if (!html || !title) return false;
	const trimmedTitle = normalizeForCompare(title);
	if (!trimmedTitle) return false;

	const match = html.match(FIRST_H1_RE);
	if (!match) return false;

	const h1Text = normalizeForCompare(stripInlineFormatting(match[1]));
	return h1Text === trimmedTitle;
}

/**
 * If the leading H1 of `html` matches `title`, return `html` with that
 * H1 removed. Otherwise return `html` unchanged.
 *
 * Only the first H1 is candidate — subsequent H1s in the body are left
 * alone.
 */
export function stripFirstH1IfMatchesTitle(html: string, title: string): string {
	if (!firstH1MatchesTitle(html, title)) return html;
	return html.replace(FIRST_H1_RE, '').replace(/^\s+/, '');
}
