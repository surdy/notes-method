import { type Extension, type Range, RangeSet, StateField, type Text } from '@codemirror/state';
import { Decoration, type DecorationSet, EditorView, GutterMarker, ViewPlugin, WidgetType, gutterLineClass } from '@codemirror/view';
import type { NoteSummary, TaskMutationStatus } from '$lib/api';
import { nextTaskStatus, TASK_MARKER_PATTERN, taskMarkerToStatus, taskStatusClass } from './task-markers';
import { resolveWikilink } from './wikilink-resolver';

const WIKILINK_RE = /\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g;
const TAG_RE = /(^|[\s([{])#([A-Za-z0-9/_-]+)/g;
const INLINE_FIELD_RE = /(^|\s)([A-Za-z][\w-]*)::(?=\s|\S)/g;
const CALLOUT_RE = /^>\s+\[!([A-Za-z0-9_-]+)\]/;
const TASK_RE = new RegExp(`^(\\s*[-*+]\\s+)\\[([${TASK_MARKER_PATTERN}])\\]`);
const FENCE_RE = /^(```|~~~)/;

export interface UnresolvedWikilink {
	/** The link target with any `#anchor` stripped. */
	target: string;
	/** Existing notes that fuzzily match, for a "did you mean" list. */
	candidates: NoteSummary[];
	/** Viewport coordinates of the click, for positioning a popup. */
	x: number;
	y: number;
}

export interface OFMDecorationOptions {
notes: () => NoteSummary[];
taskHashes: () => Map<string, string>;
onNavigate: (path: string) => void;
onTaskToggle: (taskHash: string, status: TaskMutationStatus) => Promise<void> | void;
onUnresolvedWikilink?: (info: UnresolvedWikilink) => void;
}

class FrontmatterGutterMarker extends GutterMarker {
	elementClass = 'cm-frontmatter-gutter';
}
const frontmatterGutterMarker = new FrontmatterGutterMarker();

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
checkbox.indeterminate = this.status === 'in_progress' || this.status === 'blocked' || this.status === 'waiting' || this.status === 'on_hold';
checkbox.disabled = !this.taskHash;
checkbox.title = this.taskHash ? `${taskStatusLabel(this.status)} — toggle task status` : 'Save note to refresh task anchor';
checkbox.setAttribute('aria-label', `${taskStatusLabel(this.status)} task`);
checkbox.classList.add(taskStatusClass(this.status));
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

const resolution = resolveWikilink(rawTarget, options.notes());
event.preventDefault();
if (resolution.path) {
options.onNavigate(resolution.path);
return;
}
options.onUnresolvedWikilink?.({
target: resolution.name,
candidates: resolution.candidates,
x: event.clientX,
y: event.clientY
});
}
}
}
);

const frontmatterGutterField = StateField.define<RangeSet<GutterMarker>>({
	create(state) {
		return buildFrontmatterGutterMarkers(state.doc);
	},
	update(value, tr) {
		if (tr.docChanged) {
			return buildFrontmatterGutterMarkers(tr.newDoc);
		}
		return value;
	}
});

return [plugin, frontmatterGutterField, gutterLineClass.from(frontmatterGutterField)];
}

function buildDecorations(view: EditorView, options: OFMDecorationOptions): DecorationSet {
const decorations: Range<Decoration>[] = [];
const taskHashes = options.taskHashes();
let inFrontmatter = false;
let inCodeFence = false;

for (let lineNumber = 1; lineNumber <= view.state.doc.lines; lineNumber += 1) {
const line = view.state.doc.line(lineNumber);
const text = line.text;
const trimmed = text.trimStart();

if (lineNumber === 1 && text === '---') {
inFrontmatter = true;
addLineClass(decorations, line.from, 'cm-frontmatter-line');
addMark(decorations, line.from, line.to, 'cm-frontmatter-delimiter');
continue;
}

if (inFrontmatter) {
addLineClass(decorations, line.from, 'cm-frontmatter-line');
if (text === '---') {
addMark(decorations, line.from, line.to, 'cm-frontmatter-delimiter');
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
const status = taskMarkerToStatus(taskMatch[2]);
if (status) {
decorations.push(
Decoration.replace({
widget: new TaskCheckboxWidget(status, taskHash, options.onTaskToggle)
}).range(
markerStart,
markerEnd
)
);
}
}

const calloutMatch = text.match(CALLOUT_RE);
if (calloutMatch?.index !== undefined) {
const prefix = text.indexOf('[!');
if (prefix >= 0) {
const calloutText = `[!${calloutMatch[1]}]`;
const from = line.from + prefix;
addMark(decorations, from, from + calloutText.length, 'cm-ofm-callout');
}
}

const notesForLinks = options.notes();
for (const match of text.matchAll(WIKILINK_RE)) {
const raw = match[0];
const target = match[1]?.trim();
if (!target || match.index === undefined) continue;
const unresolved = resolveWikilink(target, notesForLinks).path === null;
const className = unresolved
? 'cm-ofm-wikilink cm-ofm-wikilink-unresolved'
: 'cm-ofm-wikilink';
addMark(
decorations,
line.from + match.index,
line.from + match.index + raw.length,
className,
{ 'data-wikilink-target': target }
);
}

for (const match of text.matchAll(TAG_RE)) {
if (match.index === undefined) continue;
const prefixLength = match[1]?.length ?? 0;
const tagText = `#${match[2]}`;
const from = line.from + match.index + prefixLength;
addMark(decorations, from, from + tagText.length, 'cm-ofm-tag');
}

for (const match of text.matchAll(INLINE_FIELD_RE)) {
if (match.index === undefined) continue;
const leading = match[1]?.length ?? 0;
const key = match[2];
const keyFrom = line.from + match.index + leading;
addMark(decorations, keyFrom, keyFrom + key.length, 'cm-ofm-inline-field-key');
addMark(
decorations,
keyFrom + key.length,
keyFrom + key.length + 2,
'cm-ofm-inline-field-delimiter'
);
}
}

return Decoration.set(decorations, true);
}

function addLineClass(decorations: Range<Decoration>[], at: number, className: string) {
decorations.push(Decoration.line({ class: className }).range(at));
}

function addMark(
decorations: Range<Decoration>[],
from: number,
to: number,
className: string,
attributes?: Record<string, string>
) {
decorations.push(
Decoration.mark({
class: className,
attributes
}).range(
from,
to
)
);
}

function taskStatusLabel(status: TaskMutationStatus): string {
switch (status) {
case 'todo':
return 'To do';
case 'in_progress':
return 'In progress';
case 'blocked':
return 'Blocked';
case 'waiting':
return 'Waiting';
case 'on_hold':
return 'On hold';
case 'done':
return 'Done';
case 'cancelled':
return 'Cancelled';
}
}

function buildFrontmatterGutterMarkers(doc: Text): RangeSet<GutterMarker> {
	const markers: Range<GutterMarker>[] = [];
	if (doc.lines < 1) return RangeSet.empty;
	const firstLine = doc.line(1);
	if (firstLine.text !== '---') return RangeSet.empty;

	markers.push(frontmatterGutterMarker.range(firstLine.from));
	for (let n = 2; n <= doc.lines; n++) {
		const line = doc.line(n);
		markers.push(frontmatterGutterMarker.range(line.from));
		if (line.text === '---') break;
	}
	return RangeSet.of(markers, true);
}
