import { LanguageDescription } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { classHighlighter, highlightTree } from '@lezer/highlight';

type HighlightRange = {
	from: number;
	to: number;
	classes: string;
};

export type FencedCodeBlock = {
	language: string | null;
	code: string;
};

const FENCE_INFO_RE = /^\s*(`{3,}|~{3,})\s*(.*)$/;
const FENCE_CLOSE_RE = /^\s*(`{3,}|~{3,})\s*$/;
const LANGUAGE_RE = /^[\w#+.-]+/;

export function languageFromClassName(className: string): string | null {
	const languageClass = className.split(/\s+/).find((name) => name.startsWith('language-'));
	const language = languageClass?.slice('language-'.length).trim();
	return language ? language : null;
}

export function parseFencedCodeBlock(rawText: string): FencedCodeBlock {
	const lines = rawText.split(/\r?\n/);
	const info = lines[0]?.match(FENCE_INFO_RE)?.[2].trim() ?? '';
	const language = info.match(LANGUAGE_RE)?.[0]?.toLowerCase() ?? null;
	const hasClosingFence = lines.length > 1 && FENCE_CLOSE_RE.test(lines[lines.length - 1]);
	const codeLines = lines.slice(1, hasClosingFence ? -1 : undefined);

	return {
		language,
		code: codeLines.join('\n')
	};
}

export async function highlightCodeToHtml(code: string, language: string | null | undefined): Promise<string> {
	const normalizedLanguage = language?.trim().toLowerCase();
	const description = normalizedLanguage
		? LanguageDescription.matchLanguageName(languages, normalizedLanguage, true)
		: null;
	if (!description) {
		return escapeHtml(code);
	}

	const support = await description.load();
	const tree = support.language.parser.parse(code);
	const ranges: HighlightRange[] = [];
	highlightTree(tree, classHighlighter, (from, to, classes) => {
		ranges.push({ from, to, classes });
	});

	return highlightedRangesToHtml(code, ranges);
}

export async function highlightCodeElement(
	codeElement: HTMLElement,
	code: string,
	language = languageFromClassName(codeElement.className)
): Promise<void> {
	codeElement.classList.add('ns-highlighted-code');
	codeElement.innerHTML = await highlightCodeToHtml(code, language);
}

export async function applySyntaxHighlighting(root: ParentNode): Promise<void> {
	const codeBlocks = Array.from(root.querySelectorAll<HTMLElement>('pre > code'));
	await Promise.all(
		codeBlocks.map((codeElement) =>
			highlightCodeElement(codeElement, codeElement.textContent ?? '', languageFromClassName(codeElement.className))
		)
	);
}

function highlightedRangesToHtml(code: string, ranges: HighlightRange[]): string {
	let position = 0;
	let html = '';

	for (const range of ranges.sort((a, b) => a.from - b.from || a.to - b.to)) {
		if (range.to <= position) {
			continue;
		}

		const from = Math.max(range.from, position);
		if (from > position) {
			html += escapeHtml(code.slice(position, from));
		}

		html += `<span class="${escapeAttribute(range.classes)}">${escapeHtml(code.slice(from, range.to))}</span>`;
		position = range.to;
	}

	if (position < code.length) {
		html += escapeHtml(code.slice(position));
	}

	return html;
}

function escapeHtml(value: string): string {
	return value
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}

function escapeAttribute(value: string): string {
	return value.replace(/[^A-Za-z0-9_() -]/g, '');
}
