<script lang="ts">
import { EditorState } from '@codemirror/state';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import {
EditorView,
drawSelection,
dropCursor,
highlightActiveLine,
highlightActiveLineGutter,
keymap,
lineNumbers
} from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { closeBrackets, closeBracketsKeymap } from '@codemirror/autocomplete';
import { bracketMatching, defaultHighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language';
import { languages } from '@codemirror/language-data';
import { highlightSelectionMatches, searchKeymap } from '@codemirror/search';
import { onMount, tick } from 'svelte';
import {
ApiError,
getNote,
putNote,
toggleTaskStatus,
type NoteDetail,
type TaskMutationStatus
} from '$lib/api';
import { createAutoSave } from '$lib/editor/auto-save';
import { createOFMDecorations } from '$lib/editor/ofm-decorations';
import { notesmithTheme } from '$lib/editor/theme';
import { vaultStore } from '$lib/stores.svelte';

const TASK_LINE_RE = /^\s*[-*+]\s+\[[ xX/\-bwhBWH]\]/;

let editorContainer = $state<HTMLDivElement | undefined>();
let view: EditorView | null = null;
let currentPath: string | null = null;
let currentHash: string | null = null;
let currentTaskHashes = new Map<string, string>();
let loading = $state(false);
let error = $state<string | null>(null);
let conflictBanner = $state<{ show: boolean; path: string } | null>(null);
let dirty = $state(false);
let saveError = $state<string | null>(null);
let ignoreExternalChange: { path: string; expiresAt: number } | null = null;

const autoSave = createAutoSave({
delay: 1000,
save: async (content: string) => {
if (!currentPath) {
throw new Error('No note selected');
}
return putNote(vaultStore.currentVault, currentPath, content, currentHash);
},
onSaving: () => {
saveError = null;
},
onSaved: (hash) => {
currentHash = hash;
dirty = false;
saveError = null;
if (currentPath) {
vaultStore.markDirty(currentPath, false);
ignoreExternalChange = {
path: currentPath,
expiresAt: Date.now() + 1500
};
}
},
onError: (cause) => {
if (cause instanceof ApiError && cause.status === 409) {
if (currentPath) {
conflictBanner = { show: true, path: currentPath };
}
return;
}
saveError = cause instanceof Error ? cause.message : 'Auto-save failed';
console.error('Auto-save failed', cause);
}
});

function destroyEditor() {
if (view) {
view.destroy();
view = null;
}
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
const state = EditorState.create({
doc: buildEditorDocument(note),
extensions: [
lineNumbers(),
highlightActiveLineGutter(),
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
notesmithTheme,
createOFMDecorations({
notes: () => vaultStore.notes,
taskHashes: () => currentTaskHashes,
onNavigate: (path) => vaultStore.selectNote(path),
onTaskToggle: handleTaskToggle
}),
EditorView.updateListener.of((update) => {
if (!update.docChanged) {
return;
}
dirty = true;
saveError = null;
if (currentPath) {
vaultStore.markDirty(currentPath, true);
}
autoSave.schedule(update.state.doc.toString());
})
]
});

view = new EditorView({ state, parent: editorContainer });
}

async function loadNote(path: string) {
autoSave.cancel();
loading = true;
error = null;
saveError = null;
conflictBanner = null;
dirty = false;
destroyEditor();

try {
const note = await getNote(vaultStore.currentVault, path);
if (vaultStore.selectedPath !== path) {
return;
}

currentPath = path;
currentHash = note.hash;
currentTaskHashes = buildTaskHashes(note);
loading = false;
await tick();
createEditor(note);
vaultStore.markDirty(path, false);
} catch (cause) {
error = cause instanceof Error ? cause.message : 'Failed to load note';
loading = false;
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
await toggleTaskStatus(vaultStore.currentVault, currentPath, taskHash, status);
ignoreExternalChange = {
path: currentPath,
expiresAt: Date.now() + 1500
};
await loadNote(currentPath);
} catch (cause) {
saveError = cause instanceof Error ? cause.message : 'Failed to toggle task';
console.error('Failed to toggle task', cause);
}
}

export function handleExternalChange(changedPath: string) {
if (changedPath !== currentPath) {
return;
}

if (
ignoreExternalChange?.path === changedPath &&
ignoreExternalChange.expiresAt > Date.now()
) {
ignoreExternalChange = null;
return;
}
ignoreExternalChange = null;

if (!dirty) {
void loadNote(changedPath);
return;
}

conflictBanner = { show: true, path: changedPath };
}

async function handleReload() {
if (!currentPath) {
return;
}
conflictBanner = null;
dirty = false;
saveError = null;
vaultStore.markDirty(currentPath, false);
await loadNote(currentPath);
}

function handleKeepMine() {
currentHash = null;
conflictBanner = null;
}

$effect(() => {
const path = vaultStore.selectedPath;
if (path && (path !== currentPath || !view)) {
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
error = null;
saveError = null;
conflictBanner = null;
}
});

onMount(() => {
return () => {
autoSave.cancel();
destroyEditor();
};
});
</script>

<div class="note-editor">
{#if !vaultStore.selectedPath}
<div class="empty-state">
<p>Select a note from the sidebar to edit</p>
</div>
{:else if loading}
<div class="loading">Loading...</div>
{:else if error}
<div class="error">{error}</div>
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
<div class="save-error">{saveError}</div>
{/if}
<div class="editor-container" bind:this={editorContainer}></div>
{/if}
</div>

<style>
.note-editor {
flex: 1;
min-height: 0;
display: flex;
flex-direction: column;
background: #1e1e1e;
color: var(--text-primary, #e0e0e0);
}

.editor-container {
flex: 1;
min-height: 0;
}

.empty-state,
.loading,
.error {
flex: 1;
display: flex;
align-items: center;
justify-content: center;
padding: 24px 32px;
}

.empty-state {
color: var(--text-muted, #888);
}

.error,
.save-error {
color: #ff6b6b;
}

.conflict-banner,
.save-error {
display: flex;
align-items: center;
justify-content: space-between;
gap: 12px;
padding: 10px 16px;
border-bottom: 1px solid var(--border-color, #333);
background: #2a2014;
}

.save-error {
justify-content: flex-start;
background: #3a1f24;
}

.conflict-actions {
display: flex;
gap: 8px;
}

button {
padding: 6px 10px;
border: 1px solid var(--border-color, #444);
border-radius: 6px;
background: #2d2d2d;
color: inherit;
cursor: pointer;
}

button:hover {
background: #373737;
}
</style>
