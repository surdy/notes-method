import { type Extension, EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

export type PasteAction =
	| { type: 'markdown-link'; text: string; url: string }
	| { type: 'image-embed'; alt: string; url: string }
	| { type: 'wikilink-alias'; link: string; alias: string }
	| { type: 'fill-link-parens'; url: string }
	| { type: 'passthrough' };

const PASSTHROUGH: PasteAction = { type: 'passthrough' };
const HTTP_URL_RE = /^https?:\/\/\S+$/i;
const COMMON_URL_RE = /^(?:www\.)?[a-z0-9](?:[a-z0-9-]*[a-z0-9])?(?:\.[a-z0-9](?:[a-z0-9-]*[a-z0-9])?)+(?:[/?#]\S*)?$/i;

export function isUrl(text: string): boolean {
	const trimmed = text.trim();
	if (!trimmed) return false;
	return HTTP_URL_RE.test(trimmed) || COMMON_URL_RE.test(trimmed);
}

export function isWikilink(text: string): string | null {
	const trimmed = text.trim();
	const match = /^\[\[(.+?)\]\]$/s.exec(trimmed);
	return match ? match[1] : null;
}

export function matchesImageWhitelist(url: string, whitelist: string): boolean {
	const trimmedWhitelist = whitelist.trim();
	if (!trimmedWhitelist) return false;

	for (const pattern of trimmedWhitelist.split(/\r?\n/)) {
		const trimmedPattern = pattern.trim();
		if (!trimmedPattern) continue;
		try {
			if (new RegExp(trimmedPattern).test(url)) {
				return true;
			}
		} catch {
			continue;
		}
	}

	return false;
}

export function determinePasteAction(
	selection: string,
	clipboard: string,
	imageWhitelist: string
): PasteAction {
	const trimmedSelection = selection.trim();
	const trimmedClipboard = clipboard.trim();
	const clipboardIsUrl = isUrl(trimmedClipboard);
	const selectionIsUrl = isUrl(trimmedSelection);
	const clipboardWikilink = isWikilink(trimmedClipboard);

	if (selection && clipboardIsUrl) {
		if (!selectionIsUrl && matchesImageWhitelist(trimmedClipboard, imageWhitelist)) {
			return { type: 'image-embed', alt: selection, url: trimmedClipboard };
		}
		return { type: 'markdown-link', text: selection, url: trimmedClipboard };
	}

	if (selectionIsUrl && trimmedClipboard && !clipboardIsUrl && !clipboardWikilink) {
		return { type: 'markdown-link', text: trimmedClipboard, url: trimmedSelection };
	}

	if (selection && clipboardWikilink) {
		return { type: 'wikilink-alias', link: clipboardWikilink, alias: selection };
	}

	return PASSTHROUGH;
}

export function detectInsideLinkParens(
	state: EditorState,
	pos: number
): { from: number; to: number } | null {
	const line = state.doc.lineAt(pos);
	const offset = pos - line.from;
	const open = line.text.lastIndexOf('](', offset);
	if (open === -1) return null;

	const contentFrom = open + 2;
	const close = line.text.indexOf(')', contentFrom);
	if (close === -1) return null;
	if (offset < contentFrom || offset > close) return null;

	return {
		from: line.from + contentFrom,
		to: line.from + close
	};
}

export function createPasteUrlExtension(getImageWhitelist: () => string): Extension {
	return EditorView.domEventHandlers({
		paste(event: ClipboardEvent, view: EditorView) {
			const clipboard = event.clipboardData?.getData('text/plain')?.trim() ?? '';
			if (!clipboard) return false;

			const state = view.state;
			const { from, to } = state.selection.main;
			const selection = state.doc.sliceString(from, to);

			if (from === to && isUrl(clipboard)) {
				const insideParens = detectInsideLinkParens(state, from);
				if (!insideParens) return false;

				view.dispatch({
					changes: { from: insideParens.from, to: insideParens.to, insert: clipboard },
					selection: { anchor: insideParens.from + clipboard.length }
				});
				event.preventDefault();
				return true;
			}

			if (!selection) return false;

			const action = determinePasteAction(selection, clipboard, getImageWhitelist());
			if (action.type === 'passthrough') return false;

			let insert: string;
			switch (action.type) {
				case 'markdown-link':
					insert = `[${action.text}](${action.url})`;
					break;
				case 'image-embed':
					insert = `![${action.alt}](${action.url})`;
					break;
				case 'wikilink-alias':
					insert = `[[${action.link}|${action.alias}]]`;
					break;
				case 'fill-link-parens':
					insert = action.url;
					break;
			}

			view.dispatch({
				changes: { from, to, insert },
				selection: { anchor: from + insert.length }
			});
			event.preventDefault();
			return true;
		}
	});
}
