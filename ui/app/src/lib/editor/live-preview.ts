import { type EditorState, type Extension, type Range } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, WidgetType } from '@codemirror/view';
import { syntaxTree } from '@codemirror/language';
import {
	deleteColumnAt,
	deleteRowAt,
	duplicateColumn,
	duplicateRow,
	insertColumnAt,
	insertRowAt,
	moveColumn,
	moveRow,
	parseMarkdownTable,
	serializeMarkdownTable,
	setColumnAlignment,
	updateMarkdownTableCell,
	type MarkdownTable,
	type MarkdownTableAlignment,
	type MarkdownTableCellUpdate
} from './markdown-table.ts';
import { highlightCodeElement, parseFencedCodeBlock } from './code-highlighting.ts';
import { isExternalLinkUrl } from './link-classification.ts';
import { livePreviewTableContextMenuTheme } from './theme.ts';

// Inject context menu styles once into the document head
let contextMenuStylesInjected = false;
function ensureContextMenuStyles() {
	if (contextMenuStylesInjected) return;
	if (typeof document === 'undefined') return;
	const style = document.createElement('style');
	style.textContent = livePreviewTableContextMenuTheme;
	document.head.appendChild(style);
	contextMenuStylesInjected = true;
}

type CalloutFold = 'open' | 'closed' | null;

type ParsedCallout = {
	identifier: string;
	type: string;
	title: string;
	fold: CalloutFold;
	bodyLines: string[];
};

type CalloutBlock = {
	from: number;
	to: number;
	startLine: number;
	endLine: number;
	rawText: string;
	render: boolean;
};

const CALLOUT_RE = /^\s*>\s*\[!([\w-]+)\]([+-])?\s*(.*)$/;

class CodeBlockWidget extends WidgetType {
	constructor(
		private rawText: string,
		private from: number
	) {
		super();
	}

	eq(other: CodeBlockWidget): boolean {
		return this.rawText === other.rawText && this.from === other.from;
	}

	toDOM(view: EditorView): HTMLElement {
		const { language, code } = parseFencedCodeBlock(this.rawText);
		const pre = document.createElement('pre');
		pre.className = 'cm-lp-code-block';

		const codeElement = document.createElement('code');
		codeElement.className = language ? `cm-lp-code language-${language}` : 'cm-lp-code';
		codeElement.textContent = code;
		pre.appendChild(codeElement);

		void highlightCodeElement(codeElement, code, language).catch((cause) => {
			console.error('Failed to highlight live preview code block', cause);
		});

		pre.addEventListener('mousedown', (event) => {
			event.preventDefault();
			event.stopPropagation();
			view.dispatch({
				selection: { anchor: Math.min(this.from, view.state.doc.length) },
				scrollIntoView: true
			});
			view.focus();
		});

		return pre;
	}

	ignoreEvent(event: Event): boolean {
		return event.type !== 'mousedown';
	}
}

class HorizontalRuleWidget extends WidgetType {
	toDOM(): HTMLElement {
		const hr = document.createElement('hr');
		hr.className = 'cm-lp-hr';
		return hr;
	}
}

class BulletWidget extends WidgetType {
	toDOM(): HTMLElement {
		const span = document.createElement('span');
		span.className = 'cm-lp-bullet';
		span.textContent = '•';
		return span;
	}
}

class CalloutWidget extends WidgetType {
	constructor(
		private rawText: string,
		private from: number
	) {
		super();
	}

	eq(other: CalloutWidget): boolean {
		return this.rawText === other.rawText && this.from === other.from;
	}

	toDOM(view: EditorView): HTMLElement {
		const callout = createCalloutElement(this.rawText);
		callout.addEventListener('mousedown', (event) => {
			event.preventDefault();
			event.stopPropagation();
			view.dispatch({
				selection: { anchor: Math.min(this.from, view.state.doc.length) },
				scrollIntoView: true
			});
			view.focus();
		});
		return callout;
	}

	ignoreEvent(event: Event): boolean {
		return event.type !== 'mousedown';
	}
}

class TableWidget extends WidgetType {
	constructor(
		private rawText: string,
		private from: number,
		private to: number
	) {
		super();
	}

	eq(other: TableWidget): boolean {
		return this.rawText === other.rawText && this.from === other.from && this.to === other.to;
	}

	toDOM(view: EditorView): HTMLElement {
		ensureContextMenuStyles();
		const wrapper = document.createElement('div');
		wrapper.className = 'cm-lp-table-wrapper';

		const table = parseMarkdownTable(this.rawText);
		if (!table.headers.length) {
			wrapper.textContent = this.rawText;
			return wrapper;
		}

		const replaceTable = (nextTable: MarkdownTable) => {
			const nextMarkdown = serializeMarkdownTable(nextTable);
			view.dispatch({
				changes: { from: this.from, to: Math.min(this.to, view.state.doc.length), insert: nextMarkdown },
				selection: { anchor: this.from }
			});
		};

		const latestTable = () =>
			parseMarkdownTable(view.state.doc.sliceString(this.from, Math.min(this.to, view.state.doc.length)));

		const commitCell = (update: MarkdownTableCellUpdate) => {
			replaceTable(updateMarkdownTableCell(latestTable(), update));
		};

		const tableEl = document.createElement('table');
		tableEl.className = 'cm-lp-table';

		const thead = document.createElement('thead');
		const headRow = document.createElement('tr');
		table.headers.forEach((cell, columnIndex) => {
			const th = this.createEditableCell(
				'th',
				cell,
				{
					section: 'header',
					rowIndex: 0,
					columnIndex,
					value: cell
				},
				commitCell
			);
			th.dataset.row = '-1';
			th.dataset.col = String(columnIndex);
			this.applyAlignment(th, table.alignments[columnIndex]);
			headRow.appendChild(th);
		});
		thead.appendChild(headRow);
		tableEl.appendChild(thead);

		const tbody = document.createElement('tbody');
		table.rows.forEach((row, rowIndex) => {
			const tr = document.createElement('tr');
			table.headers.forEach((_, columnIndex) => {
				const value = row[columnIndex] ?? '';
				const td = this.createEditableCell(
					'td',
					value,
					{
						section: 'body',
						rowIndex,
						columnIndex,
						value
					},
					commitCell
				);
				td.dataset.row = String(rowIndex);
				td.dataset.col = String(columnIndex);
				this.applyAlignment(td, table.alignments[columnIndex]);
				tr.appendChild(td);
			});
			tbody.appendChild(tr);
		});
		tableEl.appendChild(tbody);
		wrapper.appendChild(tableEl);

		wrapper.addEventListener('contextmenu', (event) => {
			event.preventDefault();
			event.stopPropagation();
			const target = (event.target as HTMLElement).closest('td, th') as HTMLElement | null;
			if (!target) return;
			const rowIdx = parseInt(target.dataset.row ?? '-1', 10);
			const colIdx = parseInt(target.dataset.col ?? '0', 10);
			showTableContextMenu(event as MouseEvent, rowIdx, colIdx, latestTable, replaceTable, view);
		});

		return wrapper;
	}

	private createEditableCell(
		tagName: 'td' | 'th',
		text: string,
		update: MarkdownTableCellUpdate,
		onCommit: (update: MarkdownTableCellUpdate) => void
	): HTMLTableCellElement {
		const cell = document.createElement(tagName);
		cell.className = 'cm-lp-table-cell';
		cell.contentEditable = 'true';
		cell.spellcheck = false;
		cell.textContent = text;
		cell.addEventListener('keydown', (event) => {
			if (event.key === 'Enter') {
				event.preventDefault();
				(event.currentTarget as HTMLElement).blur();
			}
			if (event.key === 'Escape') {
				event.preventDefault();
				const target = event.currentTarget as HTMLElement;
				target.textContent = text;
				target.blur();
			}
		});
		cell.addEventListener('blur', (event) => {
			const value = (event.currentTarget as HTMLElement).textContent?.replace(/\s*\n\s*/g, ' ') ?? '';
			if (value !== text) {
				onCommit({ ...update, value });
			}
		});
		return cell;
	}

	private applyAlignment(cell: HTMLTableCellElement, alignment: MarkdownTable['alignments'][number] | undefined) {
		if (alignment && alignment !== 'left') {
			cell.style.textAlign = alignment;
		}
	}
}

function dismissTableContextMenu() {
	const existing = document.querySelector('.cm-lp-table-context-menu');
	if (existing) existing.remove();
}

function showTableContextMenu(
	event: MouseEvent,
	rowIdx: number,
	colIdx: number,
	latestTable: () => MarkdownTable,
	replaceTable: (t: MarkdownTable) => void,
	editorView: EditorView
) {
	dismissTableContextMenu();

	const menu = document.createElement('div');
	menu.className = 'cm-lp-table-context-menu';

	const isHeaderRow = rowIdx === -1;
	const table = latestTable();
	const totalRows = table.rows.length;
	const totalCols = table.headers.length;

	type MenuItem = { label: string; action: () => void; disabled?: boolean } | 'separator';
	const items: MenuItem[] = [];

	// Row operations
	if (isHeaderRow) {
		items.push({ label: 'Insert row below', action: () => replaceTable(insertRowAt(latestTable(), 0)) });
	} else {
		items.push({ label: 'Insert row above', action: () => replaceTable(insertRowAt(latestTable(), rowIdx)) });
		items.push({ label: 'Insert row below', action: () => replaceTable(insertRowAt(latestTable(), rowIdx + 1)) });
		items.push('separator');
		items.push({
			label: 'Move row up',
			action: () => replaceTable(moveRow(latestTable(), rowIdx, rowIdx - 1)),
			disabled: rowIdx === 0
		});
		items.push({
			label: 'Move row down',
			action: () => replaceTable(moveRow(latestTable(), rowIdx, rowIdx + 1)),
			disabled: rowIdx >= totalRows - 1
		});
		items.push('separator');
		items.push({ label: 'Duplicate row', action: () => replaceTable(duplicateRow(latestTable(), rowIdx)) });
		items.push({ label: 'Delete row', action: () => replaceTable(deleteRowAt(latestTable(), rowIdx)) });
	}

	items.push('separator');

	// Column operations
	items.push({ label: 'Insert column left', action: () => replaceTable(insertColumnAt(latestTable(), colIdx)) });
	items.push({ label: 'Insert column right', action: () => replaceTable(insertColumnAt(latestTable(), colIdx + 1)) });
	items.push('separator');
	items.push({
		label: 'Move column left',
		action: () => replaceTable(moveColumn(latestTable(), colIdx, colIdx - 1)),
		disabled: colIdx === 0
	});
	items.push({
		label: 'Move column right',
		action: () => replaceTable(moveColumn(latestTable(), colIdx, colIdx + 1)),
		disabled: colIdx >= totalCols - 1
	});
	items.push('separator');

	// Alignment submenu
	const alignments: { label: string; value: MarkdownTableAlignment }[] = [
		{ label: 'Align left', value: 'left' },
		{ label: 'Align center', value: 'center' },
		{ label: 'Align right', value: 'right' }
	];
	for (const a of alignments) {
		const current = table.alignments[colIdx] ?? 'left';
		items.push({
			label: current === a.value ? `${a.label} ✓` : a.label,
			action: () => replaceTable(setColumnAlignment(latestTable(), colIdx, a.value))
		});
	}
	items.push('separator');
	items.push({ label: 'Duplicate column', action: () => replaceTable(duplicateColumn(latestTable(), colIdx)) });
	items.push({
		label: 'Delete column',
		action: () => replaceTable(deleteColumnAt(latestTable(), colIdx)),
		disabled: totalCols <= 1
	});

	for (const item of items) {
		if (item === 'separator') {
			const sep = document.createElement('div');
			sep.className = 'cm-lp-table-context-menu-separator';
			menu.appendChild(sep);
		} else {
			const btn = document.createElement('button');
			btn.type = 'button';
			btn.className = 'cm-lp-table-context-menu-item';
			btn.textContent = item.label;
			if (item.disabled) {
				btn.disabled = true;
				btn.classList.add('disabled');
			}
			btn.addEventListener('click', (e) => {
				e.preventDefault();
				e.stopPropagation();
				dismissTableContextMenu();
				item.action();
				editorView.focus();
			});
			menu.appendChild(btn);
		}
	}

	document.body.appendChild(menu);

	// Position the menu at mouse coordinates, but keep it on screen
	const rect = menu.getBoundingClientRect();
	let x = event.clientX;
	let y = event.clientY;
	if (x + rect.width > window.innerWidth) x = window.innerWidth - rect.width - 4;
	if (y + rect.height > window.innerHeight) y = window.innerHeight - rect.height - 4;
	menu.style.left = `${x}px`;
	menu.style.top = `${y}px`;

	// Dismiss on click outside or Escape
	const dismiss = (e: Event) => {
		if (!menu.contains(e.target as Node)) {
			dismissTableContextMenu();
			cleanup();
		}
	};
	const dismissOnKey = (e: KeyboardEvent) => {
		if (e.key === 'Escape') {
			dismissTableContextMenu();
			cleanup();
		}
	};
	const cleanup = () => {
		document.removeEventListener('mousedown', dismiss);
		document.removeEventListener('keydown', dismissOnKey);
	};
	setTimeout(() => {
		document.addEventListener('mousedown', dismiss);
		document.addEventListener('keydown', dismissOnKey);
	}, 0);
}

function canonicalCalloutType(identifier: string): string {
	switch (identifier) {
		case 'abstract':
		case 'summary':
		case 'tldr':
			return 'abstract';
		case 'tip':
		case 'hint':
		case 'important':
			return 'tip';
		case 'success':
		case 'check':
		case 'done':
			return 'success';
		case 'question':
		case 'help':
		case 'faq':
			return 'question';
		case 'warning':
		case 'caution':
		case 'attention':
			return 'warning';
		case 'failure':
		case 'fail':
		case 'missing':
			return 'failure';
		case 'danger':
		case 'error':
			return 'danger';
		case 'quote':
		case 'cite':
			return 'quote';
		case 'note':
		case 'info':
		case 'todo':
		case 'bug':
		case 'example':
			return identifier;
		default:
			return 'note';
	}
}

function titleCase(value: string): string {
	return value.length === 0 ? value : `${value[0].toUpperCase()}${value.slice(1)}`;
}

function stripOneBlockquoteMarker(line: string): string {
	return line.replace(/^\s*>\s?/, '');
}

function parseCallout(rawText: string): ParsedCallout | null {
	const lines = rawText.split(/\r?\n/);
	const match = lines[0]?.match(CALLOUT_RE);
	if (!match) {
		return null;
	}

	const identifier = match[1].toLowerCase();
	const marker = match[2] ?? '';
	const title = match[3].trim() || titleCase(identifier);
	const fold = marker === '-' ? 'closed' : marker === '+' ? 'open' : null;

	return {
		identifier,
		type: canonicalCalloutType(identifier),
		title,
		fold,
		bodyLines: lines.slice(1).map(stripOneBlockquoteMarker)
	};
}

function createCalloutElement(rawText: string): HTMLElement {
	const parsed = parseCallout(rawText);
	const callout = document.createElement('div');
	if (!parsed) {
		callout.textContent = rawText;
		return callout;
	}

	callout.className = `cm-lp-callout callout-${parsed.type}`;
	callout.dataset.callout = parsed.identifier;
	if (parsed.fold) {
		callout.dataset.fold = parsed.fold;
	}

	const title = document.createElement('div');
	title.className = 'cm-lp-callout-title';
	title.textContent = parsed.title;
	if (parsed.fold) {
		title.addEventListener('click', (event) => {
			event.preventDefault();
			event.stopPropagation();
			callout.dataset.fold = callout.dataset.fold === 'closed' ? 'open' : 'closed';
		});
	}
	callout.appendChild(title);

	const body = createCalloutBody(parsed.bodyLines);
	if (body.childNodes.length > 0) {
		callout.appendChild(body);
	}

	return callout;
}

function createCalloutBody(lines: string[]): HTMLElement {
	const body = document.createElement('div');
	body.className = 'cm-lp-callout-body';
	let paragraph: string[] = [];

	const flushParagraph = () => {
		if (paragraph.length === 0) {
			return;
		}
		const p = document.createElement('p');
		p.textContent = paragraph.join('\n');
		body.appendChild(p);
		paragraph = [];
	};

	for (let index = 0; index < lines.length; index += 1) {
		const line = lines[index];
		if (CALLOUT_RE.test(line)) {
			flushParagraph();
			const nested = [line];
			while (index + 1 < lines.length && /^\s*>/.test(lines[index + 1])) {
				index += 1;
				nested.push(lines[index]);
			}
			body.appendChild(createCalloutElement(nested.join('\n')));
			continue;
		}
		if (line.trim() === '') {
			flushParagraph();
			continue;
		}
		paragraph.push(line);
	}

	flushParagraph();
	return body;
}

const bulletReplace = Decoration.replace({ widget: new BulletWidget() });
const hideMark = Decoration.replace({});

const renderedHeading1 = Decoration.line({ class: 'cm-lp-h1' });
const renderedHeading2 = Decoration.line({ class: 'cm-lp-h2' });
const renderedHeading3 = Decoration.line({ class: 'cm-lp-h3' });
const renderedHeading4 = Decoration.line({ class: 'cm-lp-h4' });
const renderedHeading5 = Decoration.line({ class: 'cm-lp-h5' });
const renderedHeading6 = Decoration.line({ class: 'cm-lp-h6' });

const headingDecorations = [
	renderedHeading1,
	renderedHeading2,
	renderedHeading3,
	renderedHeading4,
	renderedHeading5,
	renderedHeading6
];

const boldMark = Decoration.mark({ class: 'cm-lp-bold' });
const italicMark = Decoration.mark({ class: 'cm-lp-italic' });
const strikethroughMark = Decoration.mark({ class: 'cm-lp-strikethrough' });
const linkTextMark = Decoration.mark({ class: 'cm-lp-link-text' });
const linkExternalMark = Decoration.mark({ class: 'cm-lp-link-external' });
const inlineCodeMark = Decoration.mark({ class: 'cm-lp-inline-code' });

function cursorLines(state: EditorState): Set<number> {
	const lines = new Set<number>();
	for (const range of state.selection.ranges) {
		const startLine = state.doc.lineAt(range.from).number;
		const endLine = state.doc.lineAt(range.to).number;
		for (let n = startLine; n <= endLine; n++) {
			lines.add(n);
		}
	}
	return lines;
}

function frontmatterEndLine(state: EditorState): number {
	const doc = state.doc;
	if (doc.lines < 1) return 0;
	const first = doc.line(1);
	if (first.text !== '---') return 0;
	for (let i = 2; i <= doc.lines; i++) {
		if (doc.line(i).text === '---') return i;
	}
	return 0;
}

function rangeContainedBy(from: number, to: number, ranges: Array<{ from: number; to: number }>): boolean {
	return ranges.some((range) => from >= range.from && to <= range.to);
}

function findCalloutBlocks(state: EditorState, activeLines: Set<number>, fmEnd: number): CalloutBlock[] {
	const blocks: CalloutBlock[] = [];
	const doc = state.doc;
	let lineNumber = 1;

	while (lineNumber <= doc.lines) {
		const line = doc.line(lineNumber);
		if (lineNumber <= fmEnd || !CALLOUT_RE.test(line.text)) {
			lineNumber += 1;
			continue;
		}

		let endLineNumber = lineNumber;
		while (endLineNumber + 1 <= doc.lines && /^\s*>/.test(doc.line(endLineNumber + 1).text)) {
			endLineNumber += 1;
		}

		const endLine = doc.line(endLineNumber);
		let render = true;
		for (let n = lineNumber; n <= endLineNumber; n += 1) {
			if (activeLines.has(n)) {
				render = false;
				break;
			}
		}

		blocks.push({
			from: line.from,
			to: endLine.to,
			startLine: lineNumber,
			endLine: endLineNumber,
			rawText: doc.sliceString(line.from, endLine.to),
			render
		});
		lineNumber = endLineNumber + 1;
	}

	return blocks;
}

export function buildLivePreviewDecorationsForState(state: EditorState): DecorationSet {
	const decorations: Range<Decoration>[] = [];
	const activeLines = cursorLines(state);
	const tree = syntaxTree(state);
	const fmEnd = frontmatterEndLine(state);
	const doc = state.doc;
	const calloutBlocks = findCalloutBlocks(state, activeLines, fmEnd);
	const calloutRanges = calloutBlocks.map(({ from, to }) => ({ from, to }));

	for (const block of calloutBlocks) {
		if (!block.render) {
			continue;
		}
		decorations.push(
			Decoration.replace({
				widget: new CalloutWidget(block.rawText, block.from),
				block: true
			}).range(block.from, block.to)
		);
	}

	for (const { from, to } of [{ from: 0, to: doc.length }]) {
		tree.iterate({
			from,
			to,
			enter(node) {
				const nodeStartLine = doc.lineAt(node.from).number;
				const nodeEndLine = doc.lineAt(Math.max(node.from, node.to - 1)).number;

				// Check if any line of this node intersects with cursor lines
				let onCursorLine = false;
				for (let n = nodeStartLine; n <= nodeEndLine; n++) {
					if (activeLines.has(n)) {
						onCursorLine = true;
						break;
					}
				}

				const name = node.type.name;

				if (rangeContainedBy(node.from, node.to, calloutRanges)) return false;

				if (name === 'FencedCode') {
					if (!onCursorLine) {
						const rawText = doc.sliceString(node.from, node.to);
						const startLine = doc.lineAt(node.from);
						const endLine = doc.lineAt(Math.max(node.from, node.to - 1));
						decorations.push(
							Decoration.replace({
								widget: new CodeBlockWidget(rawText, node.from),
								block: true
							}).range(startLine.from, endLine.to)
						);
					}
					return false;
				}

				if (name === 'Table') {
					const rawText = doc.sliceString(node.from, node.to);
					const startLine = doc.lineAt(node.from);
					const endLine = doc.lineAt(Math.max(node.from, node.to - 1));
					decorations.push(
						Decoration.replace({
							widget: new TableWidget(rawText, node.from, node.to),
							block: true
						}).range(startLine.from, endLine.to)
					);
					return false;
				}

				if (onCursorLine) return;

				// ATX Headings: hide # markers, apply heading style
				if (name.startsWith('ATXHeading') && name.length === 11) {
					const level = parseInt(name[10], 10);
					if (level >= 1 && level <= 6) {
						const line = doc.lineAt(node.from);
						decorations.push(headingDecorations[level - 1].range(line.from));

						// Find and hide the HeaderMark (the # chars + trailing space)
						const child = node.node;
						let cursor = child.firstChild;
						while (cursor) {
							if (cursor.type.name === 'HeaderMark') {
								// Hide the # markers plus the space after
								const markEnd = Math.min(cursor.to + 1, line.to);
								decorations.push(hideMark.range(cursor.from, markEnd));
							}
							cursor = cursor.nextSibling;
						}
					}
					return false; // don't descend into heading children for further processing
				}

				// Bold (StrongEmphasis): hide ** markers, apply bold
				if (name === 'StrongEmphasis') {
					decorations.push(boldMark.range(node.from, node.to));
					const child = node.node;
					let cursor = child.firstChild;
					while (cursor) {
						if (cursor.type.name === 'EmphasisMark') {
							decorations.push(hideMark.range(cursor.from, cursor.to));
						}
						cursor = cursor.nextSibling;
					}
					return false;
				}

				// Italic (Emphasis): hide * or _ markers, apply italic
				if (name === 'Emphasis') {
					decorations.push(italicMark.range(node.from, node.to));
					const child = node.node;
					let cursor = child.firstChild;
					while (cursor) {
						if (cursor.type.name === 'EmphasisMark') {
							decorations.push(hideMark.range(cursor.from, cursor.to));
						}
						cursor = cursor.nextSibling;
					}
					return false;
				}

				// Strikethrough: hide ~~ markers, apply strikethrough
				if (name === 'Strikethrough') {
					decorations.push(strikethroughMark.range(node.from, node.to));
					const child = node.node;
					let cursor = child.firstChild;
					while (cursor) {
						if (cursor.type.name === 'StrikethroughMark') {
							decorations.push(hideMark.range(cursor.from, cursor.to));
						}
						cursor = cursor.nextSibling;
					}
					return false;
				}

				// Links: hide [, ](url), show text styled as link
				if (name === 'Link') {
					const slice = state.doc.sliceString(node.from, node.to);
					const match = slice.match(/^\[([^\]]*)\]\(([^)]*)\)\s*$/);
					if (match) {
						const labelText = match[1];
						const url = match[2].trim();
						const labelFrom = node.from + 1;
						const labelTo = labelFrom + labelText.length;
						const urlStart = labelTo + 1; // the ']('
						// Hide opening [
						decorations.push(hideMark.range(node.from, node.from + 1));
						// Style the label
						const mark = isExternalLinkUrl(url) ? linkExternalMark : linkTextMark;
						decorations.push(mark.range(labelFrom, labelTo));
						// Hide ](...) tail
						decorations.push(hideMark.range(urlStart, node.to));
					}
					return false;
				}

				// Inline code: hide backticks, style content
				if (name === 'InlineCode') {
					decorations.push(inlineCodeMark.range(node.from, node.to));
					const child = node.node;
					let cursor = child.firstChild;
					while (cursor) {
						if (cursor.type.name === 'CodeMark') {
							decorations.push(hideMark.range(cursor.from, cursor.to));
						}
						cursor = cursor.nextSibling;
					}
					return false;
				}

				// Bullet list markers: replace `- ` / `* ` / `+ ` with a bullet
				if (name === 'ListMark') {
					// Skip markers inside frontmatter
					if (nodeStartLine <= fmEnd) return;
					const parent = node.node.parent;
					if (parent?.parent?.type.name === 'BulletList') {
						const markText = doc.sliceString(node.from, node.to);
						if (markText === '-' || markText === '*' || markText === '+') {
							// Replace the marker character (keep trailing space)
							decorations.push(bulletReplace.range(node.from, node.to));
						}
					}
					return;
				}

				// Horizontal rule: replace with widget
				if (name === 'HorizontalRule') {
					decorations.push(
						Decoration.replace({ widget: new HorizontalRuleWidget() }).range(
							node.from,
							node.to
						)
					);
					return false;
				}
			}
		});
	}

	return Decoration.set(decorations, true);
}

export function createLivePreviewExtension(): Extension {
	return EditorView.decorations.compute(['doc', 'selection'], buildLivePreviewDecorationsForState);
}
