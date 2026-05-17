import { type Extension, type Range } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, WidgetType } from '@codemirror/view';
import { syntaxTree } from '@codemirror/language';
import {
	addMarkdownTableColumn,
	addMarkdownTableRow,
	parseMarkdownTable,
	serializeMarkdownTable,
	updateMarkdownTableCell,
	type MarkdownTable,
	type MarkdownTableCellUpdate
} from './markdown-table.ts';

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

		const toolbar = document.createElement('div');
		toolbar.className = 'cm-lp-table-toolbar';
		toolbar.append(
			this.createButton('+ Row', () => replaceTable(addMarkdownTableRow(latestTable()))),
			this.createButton('+ Column', () => replaceTable(addMarkdownTableColumn(latestTable())))
		);
		wrapper.appendChild(toolbar);

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
				this.applyAlignment(td, table.alignments[columnIndex]);
				tr.appendChild(td);
			});
			tbody.appendChild(tr);
		});
		tableEl.appendChild(tbody);
		wrapper.appendChild(tableEl);

		return wrapper;
	}

	private createButton(label: string, onClick: () => void): HTMLButtonElement {
		const button = document.createElement('button');
		button.type = 'button';
		button.className = 'cm-lp-table-button';
		button.textContent = label;
		button.addEventListener('click', (event) => {
			event.preventDefault();
			event.stopPropagation();
			onClick();
		});
		return button;
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
const inlineCodeMark = Decoration.mark({ class: 'cm-lp-inline-code' });

function cursorLines(view: EditorView): Set<number> {
	const lines = new Set<number>();
	for (const range of view.state.selection.ranges) {
		const startLine = view.state.doc.lineAt(range.from).number;
		const endLine = view.state.doc.lineAt(range.to).number;
		for (let n = startLine; n <= endLine; n++) {
			lines.add(n);
		}
	}
	return lines;
}

function frontmatterEndLine(view: EditorView): number {
	const doc = view.state.doc;
	if (doc.lines < 1) return 0;
	const first = doc.line(1);
	if (first.text !== '---') return 0;
	for (let i = 2; i <= doc.lines; i++) {
		if (doc.line(i).text === '---') return i;
	}
	return 0;
}

function buildLivePreviewDecorations(view: EditorView): DecorationSet {
	const decorations: Range<Decoration>[] = [];
	const activeLines = cursorLines(view);
	const tree = syntaxTree(view.state);
	const fmEnd = frontmatterEndLine(view);

	for (const { from, to } of view.visibleRanges) {
		tree.iterate({
			from,
			to,
			enter(node) {
				const nodeStartLine = view.state.doc.lineAt(node.from).number;
				const nodeEndLine = view.state.doc.lineAt(node.to).number;

				// Check if any line of this node intersects with cursor lines
				let onCursorLine = false;
				for (let n = nodeStartLine; n <= nodeEndLine; n++) {
					if (activeLines.has(n)) {
						onCursorLine = true;
						break;
					}
				}

				const name = node.type.name;

				if (name === 'Table') {
					const rawText = view.state.doc.sliceString(node.from, node.to);
					const startLine = view.state.doc.lineAt(node.from);
					const endLine = view.state.doc.lineAt(Math.max(node.from, node.to - 1));
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
						const line = view.state.doc.lineAt(node.from);
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
					const child = node.node;
					let linkLabel: { from: number; to: number } | null = null;
					let urlStart = -1;
					let cursor = child.firstChild;

					while (cursor) {
						if (cursor.type.name === 'LinkMark') {
							if (cursor.from === node.from) {
								// Opening [ — hide it
								decorations.push(hideMark.range(cursor.from, cursor.to));
							} else {
								// ]( or ] — start of URL portion
								urlStart = cursor.from;
							}
						}
						if (cursor.type.name === 'LinkLabel') {
							linkLabel = { from: cursor.from, to: cursor.to };
						}
						cursor = cursor.nextSibling;
					}

					if (linkLabel) {
						decorations.push(linkTextMark.range(linkLabel.from, linkLabel.to));
					}
					// Hide from ]( to end of link )
					if (urlStart >= 0) {
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
						const markText = view.state.doc.sliceString(node.from, node.to);
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
	return ViewPlugin.fromClass(
		class {
			decorations: DecorationSet;

			constructor(view: EditorView) {
				this.decorations = buildLivePreviewDecorations(view);
			}

			update(update: { docChanged: boolean; selectionSet: boolean; viewportChanged: boolean; view: EditorView }) {
				if (update.docChanged || update.selectionSet || update.viewportChanged) {
					this.decorations = buildLivePreviewDecorations(update.view);
				}
			}
		},
		{
			decorations: (value) => value.decorations
		}
	);
}
