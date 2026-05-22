<script lang="ts">
import { Compartment, EditorState } from '@codemirror/state';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import {
EditorView,
drawSelection,
dropCursor,
highlightActiveLine,
keymap
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { bracketMatching, defaultHighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { onMount, tick, untrack } from 'svelte';
import {
ApiError,
getNote,
toggleTaskStatus,
type NoteDetail,
type TaskMutationStatus
} from '$lib/api';
import { classifyError } from '$lib/api/error-classify';
import ErrorBanner from '$lib/components/ErrorBanner.svelte';
import { countWords, editorStatus, getCursorPosition } from '$lib/editor-status.svelte';
import { createAutoSave } from '$lib/editor/auto-save';
import { createExternalChangeDedup } from '$lib/editor/external-change-dedup';
import { findActiveHeadingIndex, parseHeadings } from '$lib/editor/headings';
import { createLineNumberExtensions } from '$lib/editor/line-numbers';
import { createLivePreviewExtension } from '$lib/editor/live-preview';
import { createTableEditorExtension } from '$lib/editor/table-editor';
import {
	duplicateH1HideExtension,
	setDuplicateH1TitleEffect
} from '$lib/editor/duplicate-h1-extension';
import { createOFMDecorations } from '$lib/editor/ofm-decorations';
import { createSqlBlockPlugin, refreshSqlBlockResults } from '$lib/editor/sql-blocks';
import { headingHighlightOverride, notesmithTheme } from '$lib/editor/theme';
import { displayTitleFor } from '$lib/display-title';
import TitleHeader from '$lib/components/TitleHeader.svelte';
import { headingStore } from '$lib/heading-store.svelte';
import { shouldLoadSelectedNote } from '$lib/note-loading';
import { isDashboardNote } from '$lib/right-rail';
import { saveQueue } from '$lib/save-queue';
import { settingsStore } from '$lib/settings.svelte';
import { clearApiError, reportApiError } from '$lib/stores/api-errors.svelte';
import { tabStore } from '$lib/tab-store.svelte';
import { vaultStore } from '$lib/stores.svelte';

const TASK_LINE_RE = /^\s*[-*+]\s+\[[ xX/\-bwhBWH]\]/;
const WORD_COUNT_DEBOUNCE_MS = 150;
type EditorErrorState = {
cause: unknown;
endpointHint: 'note-detail' | 'save-note' | 'toggle-task';
onAction?: () => void;
};

let editorContainer = $state<HTMLDivElement | undefined>();
let view: EditorView | null = null;
let currentPath: string | null = null;
let loadingPath = $state<string | null>(null);
let activeLoadToken: symbol | null = null;
let currentHash: string | null = null;
let currentTaskHashes = new Map<string, string>();
let loading = $state(false);
let error = $state<EditorErrorState | null>(null);
let conflictBanner = $state<{ show: boolean; path: string } | null>(null);
let dirty = $state(false);
let saveError = $state<EditorErrorState | null>(null);
let currentFrontmatter = $state<Record<string, unknown> | null>(null);
const externalChangeDedup = createExternalChangeDedup(() => currentHash);
let headingTimer: number | null = null;
let wordCountTimer: number | null = null;
let loadBanner = $derived(error ? classifyError(error.cause, error.endpointHint) : null);
let saveBanner = $derived(saveError ? classifyError(saveError.cause, saveError.endpointHint) : null);

const livePreviewCompartment = new Compartment();
const lineNumbersCompartment = new Compartment();

function showLineNumbers(): boolean {
	return settingsStore.draftConfig?.editor.show_line_numbers ?? true;
}

function applyExternalChangeOutcome(
	changedPath: string,
	outcome: ReturnType<typeof externalChangeDedup.handle>
) {
	switch (outcome.kind) {
		case 'suppress':
		case 'buffered':
			return;
		case 'reload':
			void loadNote(changedPath);
			return;
		case 'conflict':
			conflictBanner = { show: true, path: changedPath };
	}
}

function drainOutcomes(outcomes: ReturnType<typeof externalChangeDedup.recordSavedHash>) {
	if (!currentPath) {
		return;
	}
	for (const outcome of outcomes) {
		applyExternalChangeOutcome(currentPath, outcome);
	}
}

function setEditorError(
	cause: unknown,
	endpointHint: EditorErrorState['endpointHint'],
	onAction?: () => void
) {
	error = { cause, endpointHint, onAction };
	reportApiError(cause, endpointHint);
}

function setSaveError(
	cause: unknown,
	endpointHint: EditorErrorState['endpointHint'],
	onAction?: () => void
) {
	saveError = { cause, endpointHint, onAction };
	reportApiError(cause, endpointHint);
}

function clearEditorError() {
	error = null;
	clearApiError();
}

function clearSaveError() {
	saveError = null;
	clearApiError();
}

async function retryCurrentSave() {
	if (!view) {
		await saveQueue.retryAll();
		return;
	}

	await autoSave.flush(view.state.doc.toString());
}

function handleLoadErrorAction() {
	if (loadBanner?.action?.type === 'update') {
		window.location.reload();
		return;
	}

	void error?.onAction?.();
}

function handleSaveErrorAction() {
	if (saveBanner?.action?.type === 'update') {
		window.location.reload();
		return;
	}

	void saveError?.onAction?.();
}

const autoSave = createAutoSave({
delay: 1000,
save: async (content: string) => {
if (!currentPath) {
throw new Error('No note selected');
}
return saveQueue.save(vaultStore.currentVault, currentPath, content, {
	expectedHash: currentHash,
	fallbackHash: currentHash
});
},
onSaving: () => {
externalChangeDedup.beginSave();
saveError = null;
clearApiError();
},
onSaved: (hash) => {
currentHash = hash;
dirty = false;
saveError = null;
clearApiError();
if (currentPath) {
tabStore.markDirty(currentPath, false);
}
drainOutcomes(externalChangeDedup.recordSavedHash(hash));
},
onError: (cause) => {
const outcomes = externalChangeDedup.cancelSave();
if (cause instanceof ApiError && cause.status === 409) {
if (currentPath) {
conflictBanner = { show: true, path: currentPath };
}
return;
}
drainOutcomes(outcomes);
setSaveError(cause, 'save-note', () => void retryCurrentSave());
console.error('Auto-save failed', cause);
}
});

function destroyEditor() {
clearHeadingTimer();
clearWordCountTimer();
headingStore.clear();
editorStatus.clear();
if (view) {
view.destroy();
view = null;
}
}

function clearHeadingTimer() {
if (headingTimer) {
	clearTimeout(headingTimer);
	headingTimer = null;
}
}

function clearWordCountTimer() {
if (wordCountTimer) {
	clearTimeout(wordCountTimer);
	wordCountTimer = null;
}
}

function updateEditorCursorStatus(state: EditorState, wordCount = editorStatus.wordCount): void {
const { line, col } = getCursorPosition(state.doc, state.selection.main.head);
editorStatus.update(line, col, wordCount);
}

function updateEditorWordCount(state: EditorState): void {
updateEditorCursorStatus(state, countWords(state.doc.toString()));
}

function scheduleWordCountUpdate(): void {
clearWordCountTimer();
wordCountTimer = window.setTimeout(() => {
	wordCountTimer = null;
	if (!view) {
		return;
	}

	updateEditorWordCount(view.state);
}, WORD_COUNT_DEBOUNCE_MS);
}

function updateHeadings(doc: string): void {
const headings = parseHeadings(doc);
headingStore.update(headings);
}

function updateActiveHeading(cursorPos: number): void {
headingStore.setActive(findActiveHeadingIndex(headingStore.headings, cursorPos));
}

function setDashboardMode(enabled: boolean) {
editorContainer?.classList.toggle('dashboard-mode', enabled);
}

function buildEditorDocument(note: NoteDetail): string {
if (!note.raw_frontmatter) {
return note.body;
}
return `---\n${note.raw_frontmatter}\n---\n${note.body}`;
}

function buildTaskHashes(note: NoteDetail): Map<string, string> {
const hashes = new Map<string, string>();
const tasks = note.tasks ?? [];
let taskIndex = 0;
for (const line of note.body.split(/\r?\n/)) {
if (!TASK_LINE_RE.test(line)) {
continue;
}
const hash = tasks[taskIndex]?.content_hash;
if (hash) {
hashes.set(line, hash);
}
taskIndex += 1;
}
return hashes;
}

function createEditor(note: NoteDetail) {
destroyEditor();

if (!editorContainer) {
return;
}

currentTaskHashes = buildTaskHashes(note);
const documentText = buildEditorDocument(note);
const state = EditorState.create({
doc: documentText,
extensions: [
lineNumbersCompartment.of(createLineNumberExtensions(showLineNumbers())),
highlightActiveLine(),
drawSelection(),
dropCursor(),
indentOnInput(),
bracketMatching(),
closeBrackets(),
highlightSelectionMatches(),
history(),
EditorView.lineWrapping,
EditorView.contentAttributes.of({ spellcheck: 'false' }),
createTableEditorExtension(() => tabStore.activeViewMode === 'source'),
keymap.of([
...closeBracketsKeymap,
...defaultKeymap,
...searchKeymap,
...historyKeymap,
indentWithTab,
{
key: 'Mod-s',
run: () => {
if (view) {
void autoSave.flush(view.state.doc.toString());
}
return true;
}
}
]),
markdown({ base: markdownLanguage, codeLanguages: languages }),
syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
headingHighlightOverride,
notesmithTheme,
createSqlBlockPlugin(() => vaultStore.currentVault),
createOFMDecorations({
notes: () => vaultStore.notes,
taskHashes: () => currentTaskHashes,
onNavigate: (path) => tabStore.selectNote(path),
onTaskToggle: handleTaskToggle
}),
duplicateH1HideExtension(),
livePreviewCompartment.of(
	tabStore.activeViewMode === 'live-preview' ? createLivePreviewExtension() : []
),
EditorView.updateListener.of((update) => {
	if (update.selectionSet || update.docChanged) {
		updateEditorCursorStatus(update.state);
		updateActiveHeading(update.state.selection.main.head);
	}
	if (update.docChanged) {
		dirty = true;
		saveError = null;
		if (currentPath) {
			tabStore.markDirty(currentPath, true);
		}
		autoSave.schedule(update.state.doc.toString());
		scheduleWordCountUpdate();
		clearHeadingTimer();
		headingTimer = window.setTimeout(() => {
			updateHeadings(update.state.doc.toString());
			updateActiveHeading(update.state.selection.main.head);
			headingTimer = null;
		}, 300);
	}
})
]
});

view = new EditorView({ state, parent: editorContainer });
syncDuplicateH1Title();
updateEditorWordCount(state);
updateHeadings(documentText);
updateActiveHeading(state.selection.main.head);
}

function syncDuplicateH1Title() {
	if (!view) return;
	const enabled = settingsStore.draftConfig?.editor.hide_duplicate_h1 ?? true;
	const path = tabStore.selectedPath ?? currentPath ?? '';
	const title =
		enabled && path
			? displayTitleFor({ path, frontmatter: currentFrontmatter })
			: null;
	view.dispatch({ effects: setDuplicateH1TitleEffect.of(title) });
}

async function loadNote(path: string) {
const token = Symbol(path);
activeLoadToken = token;
loadingPath = path;
autoSave.cancel();
loading = true;
error = null;
saveError = null;
clearApiError();
conflictBanner = null;
dirty = false;
externalChangeDedup.reset();
destroyEditor();
currentPath = null;
currentHash = null;
currentFrontmatter = null;
currentTaskHashes = new Map();
setDashboardMode(false);

try {
const vault = vaultStore.currentVault;
const note = await getNote(vault, path);
if (activeLoadToken !== token || tabStore.selectedPath !== path || vaultStore.currentVault !== vault) {
return;
}

currentPath = path;
currentHash = note.hash;
currentFrontmatter = note.frontmatter ?? null;
currentTaskHashes = buildTaskHashes(note);
loading = false;
await tick();
if (activeLoadToken !== token || tabStore.selectedPath !== path || vaultStore.currentVault !== vault) {
return;
}
setDashboardMode(isDashboardNote(note.frontmatter));
createEditor(note);
tabStore.markDirty(path, false);
} catch (cause) {
if (activeLoadToken !== token) {
return;
}
	setEditorError(cause, 'note-detail', () => void loadNote(path));
loading = false;
} finally {
if (activeLoadToken === token) {
loadingPath = null;
activeLoadToken = null;
}
}
}

async function handleTaskToggle(taskHash: string, status: TaskMutationStatus) {
if (!currentPath) {
return;
}

if (dirty && view) {
try {
await autoSave.flush(view.state.doc.toString());
} catch {
return;
}
}

try {
const response = await toggleTaskStatus(vaultStore.currentVault, currentPath, taskHash, status);
// Remember the hash the server just wrote so the watcher's NoteUpdated
// echo for this toggle is suppressed when it arrives.
currentHash = response.hash;
externalChangeDedup.rememberHash(response.hash);
await loadNote(currentPath);
} catch (cause) {
	setSaveError(cause, 'toggle-task', () => void handleTaskToggle(taskHash, status));
console.error('Failed to toggle task', cause);
}
}

export function handleExternalChange(changedPath: string, changedHash?: string) {
if (changedPath !== currentPath) {
return;
}
const outcome = externalChangeDedup.handle({ path: changedPath, hash: changedHash }, dirty);
applyExternalChangeOutcome(changedPath, outcome);
}

export function refreshSqlBlocks() {
if (!view) {
	return;
}

view.dispatch({ effects: refreshSqlBlockResults.of(Date.now()) });
}

export async function flushSave() {
if (!view || !dirty) {
	return;
}
await autoSave.flush(view.state.doc.toString());
}

async function handleReload() {
if (!currentPath) {
return;
}
conflictBanner = null;
dirty = false;
saveError = null;
clearApiError();
tabStore.markDirty(currentPath, false);
await loadNote(currentPath);
}

function handleKeepMine() {
currentHash = null;
conflictBanner = null;
}

function handleScrollTo(event: CustomEvent<{ from: number }>) {
	if (!view) {
		return;
	}

	const { from } = event.detail;
	view.dispatch({
		selection: { anchor: from },
		scrollIntoView: true,
		effects: EditorView.scrollIntoView(from, { y: 'start' })
	});
}

$effect(() => {
const path = tabStore.selectedPath;
untrack(() => {
	if (
	path &&
	shouldLoadSelectedNote({
		selectedPath: path,
		currentPath,
		loadingPath
	})
	) {
	void loadNote(path);
	return;
	}

	if (!path) {
	autoSave.cancel();
	destroyEditor();
	currentPath = null;
	currentHash = null;
	currentTaskHashes = new Map();
	dirty = false;
	loading = false;
	loadingPath = null;
	activeLoadToken = null;
	error = null;
	saveError = null;
	conflictBanner = null;
	setDashboardMode(false);
	}
});
});

$effect(() => {
const mode = tabStore.activeViewMode;
untrack(() => {
	if (!view) return;
	view.dispatch({
		effects: livePreviewCompartment.reconfigure(
			mode === 'live-preview' ? createLivePreviewExtension() : []
		)
	});
});
});

$effect(() => {
const enabled = showLineNumbers();
untrack(() => {
	if (!view) return;
	view.dispatch({
		effects: lineNumbersCompartment.reconfigure(createLineNumberExtensions(enabled))
	});
});
});

$effect(() => {
	// Re-dispatch the duplicate-H1 hide title when the toggle or
	// frontmatter `title:` changes for the active note.
	void settingsStore.draftConfig?.editor.hide_duplicate_h1;
	void currentFrontmatter;
	void tabStore.selectedPath;
	untrack(() => {
		syncDuplicateH1Title();
	});
});

onMount(() => {
	const scrollListener: EventListener = (event) =>
		handleScrollTo(event as CustomEvent<{ from: number }>);
	window.addEventListener('notesmith:scroll-to', scrollListener);

return () => {
	window.removeEventListener('notesmith:scroll-to', scrollListener);
autoSave.cancel();
destroyEditor();
};
});
</script>

<div class="note-editor">
{#if !tabStore.selectedPath}
<div class="empty-state">
<p>Select a note from the sidebar to edit</p>
</div>
{:else if loading}
<div class="loading">Loading...</div>
{:else if error}
<ErrorBanner
error={loadBanner}
onAction={handleLoadErrorAction}
onDismiss={clearEditorError}
/>
{:else}
{#if conflictBanner?.show}
<div class="conflict-banner">
<span>⚠️ This file has changed on disk.</span>
<div class="conflict-actions">
<button type="button" onclick={handleReload}>Reload</button>
<button type="button" onclick={handleKeepMine}>Keep mine</button>
</div>
</div>
{/if}
{#if saveError}
<ErrorBanner
error={saveBanner}
onAction={handleSaveErrorAction}
onDismiss={clearSaveError}
/>
{/if}
<TitleHeader path={tabStore.selectedPath ?? ''} frontmatter={currentFrontmatter} variant="editor" />
<div class="editor-container" bind:this={editorContainer}></div>
{/if}
</div>

<style>
.note-editor {
flex: 1;
min-height: 0;
display: flex;
flex-direction: column;
background: var(--ns-editor-bg);
color: var(--ns-text);
}

.editor-container {
flex: 1;
min-height: 0;
}

:global(.editor-container.dashboard-mode .cm-content) {
padding-top: 10px;
padding-bottom: 10px;
}

:global(.editor-container.dashboard-mode .cm-line) {
padding-left: 12px;
padding-right: 12px;
}

:global(.editor-container.dashboard-mode .cm-sql-result) {
margin-top: 6px;
margin-bottom: 10px;
}

.empty-state,
.loading {
flex: 1;
display: flex;
align-items: center;
justify-content: center;
padding: 24px 32px;
}

.empty-state {
color: var(--ns-text-muted);
}

.conflict-banner {
display: flex;
align-items: center;
justify-content: space-between;
gap: 12px;
padding: 10px 16px;
border-bottom: 1px solid var(--ns-border);
background: var(--ns-warning-bg-soft);
}

.conflict-actions {
display: flex;
gap: 8px;
}

button {
padding: 6px 10px;
border: 1px solid var(--ns-border-strong);
border-radius: 6px;
background: var(--ns-panel-bg-strong);
color: inherit;
cursor: pointer;
}

button:hover {
background: var(--ns-panel-hover-strong);
}
</style>
