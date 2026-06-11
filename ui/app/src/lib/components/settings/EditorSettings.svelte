<script lang="ts">
import type { VaultConfigData } from '$lib/api';
import {
selectField,
toggleField,
type MarkDirtyFn,
type SaveImmediateFn
} from '$lib/settings-helpers';

interface Props {
cfg: VaultConfigData;
markDirty: MarkDirtyFn;
saveImmediate: SaveImmediateFn;
}

let { cfg, saveImmediate }: Props = $props();
</script>

<section class="config-section">
<label class="field field-toggle">
<span class="field-label">Live Preview</span>
<input
type="checkbox"
{...toggleField(saveImmediate, 'editor', cfg.editor.live_preview, (v) => {
cfg.editor.live_preview = v;
})}
/>
</label>
<label class="field">
<span class="field-label">Default Mode</span>
<select
{...selectField(saveImmediate, 'editor', cfg.editor.default_mode, (v) => {
cfg.editor.default_mode = v;
})}
>
<option value="source">Source</option>
<option value="reading">Reading</option>
<option value="live-preview">Live Preview</option>
</select>
</label>
<label class="field field-toggle field-toggle-stack">
<span class="field-label">Show line numbers</span>
<input
type="checkbox"
{...toggleField(saveImmediate, 'editor', cfg.editor.show_line_numbers, (v) => {
cfg.editor.show_line_numbers = v;
})}
/>
<span class="field-description">
Show line numbers in Source and Live Preview editor modes.
</span>
</label>
<label class="field field-toggle field-toggle-stack">
<span class="field-label">Strict line breaks</span>
<input
type="checkbox"
{...toggleField(saveImmediate, 'editor', cfg.editor.strict_line_breaks, (v) => {
cfg.editor.strict_line_breaks = v;
})}
/>
<span class="field-description">
Require two newlines or trailing spaces for line breaks (standard Markdown). When off,
single newlines create line breaks (Obsidian default).
</span>
</label>
<label class="field field-toggle field-toggle-stack">
<span class="field-label">Hide duplicate H1</span>
<input
type="checkbox"
{...toggleField(saveImmediate, 'editor', cfg.editor.hide_duplicate_h1, (v) => {
cfg.editor.hide_duplicate_h1 = v;
})}
/>
<span class="field-description">
When a note's first heading duplicates the note title, hide it in reading view and
live preview so the title isn't shown twice. The source file is not modified.
</span>
</label>
<label class="field field-stack">
<span class="field-label">Image URL Whitelist</span>
<textarea
rows="4"
autocapitalize="off"
placeholder="youtu.?be|vimeo&#10;imgur\\.com&#10;.*\\.(?:png|jpg|gif)"
value={cfg.editor.paste_url_image_whitelist ?? ''}
onchange={(e) => {
	cfg.editor.paste_url_image_whitelist = e.currentTarget.value;
	saveImmediate('editor');
}}
></textarea>
<span class="field-description">
Regex patterns (one per line) for URLs that should produce image embeds
(![alt](url)) when pasted onto selected text.
</span>
</label>
</section>

<style>
.config-section {
padding: 16px 24px;
max-width: 560px;
}

.field {
display: flex;
flex-direction: column;
gap: 4px;
margin-bottom: 14px;
}

.field-toggle {
flex-direction: row;
align-items: center;
gap: 10px;
}

.field-toggle input[type='checkbox'] {
order: -1;
width: 16px;
height: 16px;
accent-color: var(--accent-bg);
}

.field-toggle .field-label {
order: 1;
}

.field-toggle-stack {
align-items: flex-start;
}

.field-stack {
align-items: flex-start;
}

.field-toggle-stack .field-description {
order: 2;
margin-left: 26px;
}

.field-label {
font-size: 12px;
color: var(--text-muted);
}

.field-description {
font-size: 11px;
color: var(--text-muted);
line-height: 1.4;
max-width: 420px;
}

.field select,
.field textarea {
padding: 6px 10px;
border: 1px solid var(--border-strong);
border-radius: 4px;
background: var(--bg-secondary);
color: var(--text-default);
font-size: 13px;
max-width: 400px;
}

.field textarea {
min-height: 88px;
resize: vertical;
}

.field select:focus,
.field textarea:focus {
outline: none;
border-color: var(--accent-bg);
}

@media (max-width: 600px) {
.config-section {
padding: 12px 16px;
}
}
</style>
