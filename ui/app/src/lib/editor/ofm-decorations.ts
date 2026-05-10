import { RangeSetBuilder, type Extension } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, ViewPlugin, WidgetType } from '@codemirror/view';
import type { NoteSummary, TaskMutationStatus } from '$lib/api';

const WIKILINK_RE = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;
const TAG_RE = /(^|[\s([{])#([A-Za-z0-9/_-]+)/g;
const INLINE_FIELD_RE = /(^|\s)([A-Za-z][\w-]*)::(?=\s|\S)/g;
const CALLOUT_RE = /^>\s+\[!([A-Za-z0-9_-]+)\]/;
const TASK_RE = /^(\s*[-*+]\s+)\[([ xX/\-])\]/;
const FENCE_RE = /^(```|~~~)/;

export interface OFMDecorationOptions {
notes: () => NoteSummary[];
taskHashes: () => Map<string, string>;
onNavigate: (path: string) => void;
onTaskToggle: (taskHash: string, status: TaskMutationStatus) => Promise<void> | void;
}

class TaskCheckboxWidget extends WidgetType {
constructor(
private readonly status: TaskMutationStatus,
private readonly taskHash: string | null,
private readonly onToggle: OFMDecorationOptions['onTaskToggle']
) {
super();
}

eq(other: TaskCheckboxWidget): boolean {
return this.status === other.status && this.taskHash === other.taskHash;
}

toDOM(): HTMLElement {
const checkbox = document.createElement('input');
checkbox.type = 'checkbox';
checkbox.className = 'cm-ofm-task-toggle';
checkbox.checked = this.status === 'done';
checkbox.indeterminate = this.status === 'in_progress';
checkbox.disabled = !this.taskHash;
checkbox.title = this.taskHash ? 'Toggle task status' : 'Save note to refresh task anchor';
checkbox.setAttribute('aria-label', 'Toggle task status');
if (this.status === 'cancelled') {
checkbox.classList.add('cancelled');
}
checkbox.addEventListener('mousedown', (event) => {
event.preventDefault();
});
checkbox.addEventListener('click', (event) => {
event.preventDefault();
event.stopPropagation();
if (!this.taskHash) return;
void this.onToggle(this.taskHash, nextTaskStatus(this.status));
});
return checkbox;
}
}

export function createOFMDecorations(options: OFMDecorationOptions): Extension {
const plugin = ViewPlugin.fromClass(
class {
decorations: DecorationSet;

constructor(view: EditorView) {
this.decorations = buildDecorations(view, options);
}

update(update: { docChanged: boolean; viewportChanged: boolean; view: EditorView }) {
if (update.docChanged || update.viewportChanged) {
this.decorations = buildDecorations(update.view, options);
}
}
},
{
decorations: (value) => value.decorations,
eventHandlers: {
click(event) {
const target = event.target;
if (!(target instanceof HTMLElement)) {
return;
}

const wikilink = target.closest<HTMLElement>('[data-wikilink-target]');
if (!wikilink) {
return;
}

const rawTarget = wikilink.dataset.wikilinkTarget;
if (!rawTarget) {
return;
}

const resolved = resolveWikilink(rawTarget, options.notes());
if (!resolved) {
return;
}

event.preventDefault();
options.onNavigate(resolved);
}
}
}
);

return plugin;
}

function buildDecorations(view: EditorView, options: OFMDecorationOptions): DecorationSet {
const builder = new RangeSetBuilder<Decoration>();
const taskHashes = options.taskHashes();
let inFrontmatter = false;
let inCodeFence = false;

for (let lineNumber = 1; lineNumber <= view.state.doc.lines; lineNumber += 1) {
const line = view.state.doc.line(lineNumber);
const text = line.text;
const trimmed = text.trimStart();

if (lineNumber === 1 && text === '---') {
inFrontmatter = true;
addLineClass(builder, line.from, 'cm-frontmatter-line');
addMark(builder, line.from, line.to, 'cm-frontmatter-delimiter');
continue;
}

if (inFrontmatter) {
addLineClass(builder, line.from, 'cm-frontmatter-line');
if (text === '---') {
addMark(builder, line.from, line.to, 'cm-frontmatter-delimiter');
inFrontmatter = false;
}
continue;
}

if (FENCE_RE.test(trimmed)) {
inCodeFence = !inCodeFence;
}
if (inCodeFence) {
continue;
}

const taskMatch = text.match(TASK_RE);
if (taskMatch) {
const markerStart = line.from + taskMatch[1].length;
const markerEnd = markerStart + 3;
const taskHash = taskHashes.get(text) ?? null;
const status = markerToStatus(taskMatch[2]);
if (status) {
builder.add(
markerStart,
markerEnd,
Decoration.replace({
widget: new TaskCheckboxWidget(status, taskHash, options.onTaskToggle)
})
);
}
}

const calloutMatch = text.match(CALLOUT_RE);
if (calloutMatch?.index !== undefined) {
const prefix = text.indexOf('[!');
if (prefix >= 0) {
const calloutText = `[!${calloutMatch[1]}]`;
const from = line.from + prefix;
addMark(builder, from, from + calloutText.length, 'cm-ofm-callout');
}
}

for (const match of text.matchAll(WIKILINK_RE)) {
const raw = match[0];
const target = match[1]?.trim();
if (!target || match.index === undefined) continue;
addMark(
builder,
line.from + match.index,
line.from + match.index + raw.length,
'cm-ofm-wikilink',
{ 'data-wikilink-target': target }
);
}

for (const match of text.matchAll(TAG_RE)) {
if (match.index === undefined) continue;
const prefixLength = match[1]?.length ?? 0;
const tagText = `#${match[2]}`;
const from = line.from + match.index + prefixLength;
addMark(builder, from, from + tagText.length, 'cm-ofm-tag');
}

for (const match of text.matchAll(INLINE_FIELD_RE)) {
if (match.index === undefined) continue;
const leading = match[1]?.length ?? 0;
const key = match[2];
const keyFrom = line.from + match.index + leading;
addMark(builder, keyFrom, keyFrom + key.length, 'cm-ofm-inline-field-key');
addMark(
builder,
keyFrom + key.length,
keyFrom + key.length + 2,
'cm-ofm-inline-field-delimiter'
);
}
}

return builder.finish();
}

function addLineClass(builder: RangeSetBuilder<Decoration>, at: number, className: string) {
builder.add(at, at, Decoration.line({ class: className }));
}

function addMark(
builder: RangeSetBuilder<Decoration>,
from: number,
to: number,
className: string,
attributes?: Record<string, string>
) {
builder.add(
from,
to,
Decoration.mark({
class: className,
attributes
})
);
}

function markerToStatus(marker: string): TaskMutationStatus | null {
switch (marker) {
case ' ':
return 'todo';
case '/':
return 'in_progress';
case 'x':
case 'X':
return 'done';
case '-':
return 'cancelled';
default:
return null;
}
}

function nextTaskStatus(status: TaskMutationStatus): TaskMutationStatus {
return status === 'done' ? 'todo' : 'done';
}

function resolveWikilink(target: string, notes: NoteSummary[]): string | null {
const noteTarget = target.split('#')[0]?.trim();
if (!noteTarget) {
return null;
}

return (
notes.find((note) => note.path === noteTarget)?.path ??
notes.find((note) => note.path === `${noteTarget}.md`)?.path ??
notes.find((note) => note.path.replace(/\.md$/i, '') === noteTarget)?.path ??
notes.find((note) => note.title === noteTarget)?.path ??
notes.find((note) => note.path.endsWith(`/${noteTarget}.md`) || note.path.endsWith(`${noteTarget}.md`))
?.path ??
notes.find((note) => note.path.includes(noteTarget))?.path ??
null
);
}
