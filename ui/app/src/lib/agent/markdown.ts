/**
 * Minimal, dependency-free Markdown renderer for agent chat bubbles.
 *
 * Agent responses arrive as Markdown text (e.g. `**bold**`, lists, fenced code)
 * but the chat panel previously showed the raw source. This renderer converts
 * the common subset agents emit into HTML for `{@html}`.
 *
 * Safety: all source text is HTML-escaped *before* our own tags are injected,
 * and links are restricted to `http(s)`/`mailto` schemes, so escaping a partial
 * (streaming) or hostile message can never inject markup. Unterminated markers
 * (common mid-stream) degrade to their literal characters rather than throwing.
 */

function escapeHtml(value: string): string {
	return value
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}

function escapeAttribute(value: string): string {
	return value.replace(/[^A-Za-z0-9_-]/g, '');
}

/** Allow only schemes that cannot execute script; null rejects the link. */
function sanitizeUrl(url: string): string | null {
	const trimmed = url.trim();
	if (/^(https?:|mailto:)/i.test(trimmed)) return trimmed;
	return null;
}

/**
 * Display label for a note wikilink target: the file's base name without its
 * folder path or `.md` extension (e.g. `work/Zero-downtime cutover.md` →
 * `Zero-downtime cutover`). Falls back to the raw target when empty.
 */
function noteLinkLabel(target: string): string {
	const base = target.trim().replace(/\/+$/, '');
	const name = base.slice(base.lastIndexOf('/') + 1);
	return name.replace(/\.md$/i, '') || base;
}

const PLACEHOLDER = '\u0000';

function renderInline(text: string): string {
	// Pull inline code spans out first so their contents are never treated as
	// markup; store the already-escaped replacement behind a sentinel.
	const codes: string[] = [];
	let s = text.replace(/`([^`]+)`/g, (_match, code: string) => {
		codes.push(`<code>${escapeHtml(code)}</code>`);
		return `${PLACEHOLDER}${codes.length - 1}${PLACEHOLDER}`;
	});

	// Note wikilinks `[[path]]` / `[[path|label]]` → clickable note links. Pulled
	// out before escaping so path characters (`/`, spaces, `.`) survive intact;
	// the chat component resolves `data-note-target` to the note and opens it on
	// click. Extracted after code spans so a `[[…]]` inside backticks stays code.
	s = s.replace(/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g, (_match, target: string, label?: string) => {
		const attr = escapeHtml(target.trim());
		const display = escapeHtml((label ?? noteLinkLabel(target)).trim());
		codes.push(
			`<a class="agent-notelink" data-note-target="${attr}" role="link" tabindex="0">${display}</a>`
		);
		return `${PLACEHOLDER}${codes.length - 1}${PLACEHOLDER}`;
	});

	s = escapeHtml(s);

	// Links: [label](url) — only safe schemes become anchors.
	s = s.replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (match, label: string, url: string) => {
		const safe = sanitizeUrl(url);
		if (!safe) return label;
		return `<a href="${escapeHtml(safe)}" target="_blank" rel="noopener noreferrer">${label}</a>`;
	});

	// Bold before italic so `**x**` is not consumed by the italic rule.
	s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
	s = s.replace(/__([^_]+)__/g, '<strong>$1</strong>');
	s = s.replace(/(^|[^*])\*([^*\n]+)\*/g, '$1<em>$2</em>');
	s = s.replace(/(^|[^_\w])_([^_\n]+)_/g, '$1<em>$2</em>');

	// Restore code spans.
	s = s.replace(new RegExp(`${PLACEHOLDER}(\\d+)${PLACEHOLDER}`, 'g'), (_m, n: string) => {
		return codes[Number(n)] ?? '';
	});

	return s;
}

function isBlockBoundary(line: string): boolean {
	return (
		line.trim() === '' ||
		/^```/.test(line) ||
		/^#{1,6}\s+/.test(line) ||
		/^\s*[-*+]\s+/.test(line) ||
		/^\s*\d+\.\s+/.test(line)
	);
}

/** Render a Markdown string to sanitized HTML. */
export function renderMarkdown(src: string): string {
	const lines = src.replace(/\r\n?/g, '\n').split('\n');
	const blocks: string[] = [];
	let i = 0;

	while (i < lines.length) {
		const line = lines[i];

		const fence = line.match(/^```(\w*)\s*$/);
		if (fence) {
			const lang = fence[1];
			const code: string[] = [];
			i += 1;
			while (i < lines.length && !/^```\s*$/.test(lines[i])) {
				code.push(lines[i]);
				i += 1;
			}
			i += 1; // consume closing fence if present
			const cls = lang ? ` class="language-${escapeAttribute(lang)}"` : '';
			blocks.push(`<pre><code${cls}>${escapeHtml(code.join('\n'))}</code></pre>`);
			continue;
		}

		const heading = line.match(/^(#{1,6})\s+(.*)$/);
		if (heading) {
			const level = heading[1].length;
			blocks.push(`<h${level}>${renderInline(heading[2])}</h${level}>`);
			i += 1;
			continue;
		}

		if (/^\s*[-*+]\s+/.test(line)) {
			const items: string[] = [];
			while (i < lines.length && /^\s*[-*+]\s+/.test(lines[i])) {
				items.push(`<li>${renderInline(lines[i].replace(/^\s*[-*+]\s+/, ''))}</li>`);
				i += 1;
			}
			blocks.push(`<ul>${items.join('')}</ul>`);
			continue;
		}

		if (/^\s*\d+\.\s+/.test(line)) {
			const items: string[] = [];
			while (i < lines.length && /^\s*\d+\.\s+/.test(lines[i])) {
				items.push(`<li>${renderInline(lines[i].replace(/^\s*\d+\.\s+/, ''))}</li>`);
				i += 1;
			}
			blocks.push(`<ol>${items.join('')}</ol>`);
			continue;
		}

		if (line.trim() === '') {
			i += 1;
			continue;
		}

		const para: string[] = [];
		while (i < lines.length && !isBlockBoundary(lines[i])) {
			para.push(lines[i]);
			i += 1;
		}
		blocks.push(`<p>${para.map(renderInline).join('<br>')}</p>`);
	}

	return blocks.join('');
}
