<script lang="ts">
import type { VaultConfigData } from '$lib/api';
import {
textField,
type FieldErrorFn,
type FieldWarningFn,
type MarkDirtyFn,
type RevertFn,
type SaveImmediateFn,
type SaveSectionFn,
type SectionIsDirtyFn
} from '$lib/settings-helpers';

interface Props {
cfg: VaultConfigData;
fieldError: FieldErrorFn;
fieldWarning: FieldWarningFn;
sectionIsDirty: SectionIsDirtyFn;
saveSection: SaveSectionFn;
revert: RevertFn;
markDirty: MarkDirtyFn;
saveImmediate: SaveImmediateFn;
}

let { cfg, fieldError, fieldWarning, sectionIsDirty, saveSection, revert, markDirty }: Props =
$props();
</script>

<section class="config-section">
{#if sectionIsDirty('name') || sectionIsDirty('homepage') || sectionIsDirty('capture')}
<div class="section-actions">
<button type="button" class="btn-save" onclick={() => void saveSection('name')}>Save</button>
<button
type="button"
class="btn-revert"
onclick={() => {
revert('name');
revert('homepage');
revert('capture');
}}
>
Revert
</button>
</div>
{/if}
<label class="field">
<span class="field-label">Vault Name</span>
<input
type="text"
{...textField(markDirty, 'name', cfg.name, (v) => {
cfg.name = v;
})}
/>
{#if fieldError('name')}<span class="field-error">{fieldError('name')}</span>{/if}
</label>
<label class="field">
<span class="field-label">Homepage</span>
<input
type="text"
placeholder="e.g. Dashboard.md"
{...textField(markDirty, 'homepage', cfg.homepage, (v) => {
cfg.homepage = v || null;
})}
/>
</label>
<label class="field">
<span class="field-label">Default capture folder</span>
<input
type="text"
{...textField(markDirty, 'capture', cfg.capture.folder, (v) => {
cfg.capture.folder = v;
})}
/>
{#if fieldError('capture.folder')}
<span class="field-error">{fieldError('capture.folder')}</span>
{/if}
{#if fieldWarning('capture.folder')}
<span class="field-warning">{fieldWarning('capture.folder')}</span>
{/if}
</label>
<label class="field">
<span class="field-label">Capture template</span>
<input
type="text"
{...textField(markDirty, 'capture', cfg.capture.template, (v) => {
cfg.capture.template = v;
})}
/>
</label>
</section>

<style>
.config-section {
padding: 16px 24px;
max-width: 560px;
}

.section-actions {
display: flex;
gap: 6px;
margin-bottom: 12px;
}

.btn-save,
.btn-revert {
padding: 5px 14px;
border-radius: 4px;
border: 1px solid var(--border-strong);
font-size: 12px;
cursor: pointer;
}

.btn-save {
background: var(--accent-bg);
color: var(--text-inverse);
border-color: var(--accent-bg);
}

.btn-save:hover {
background: var(--accent-hover);
}

.btn-revert {
background: transparent;
color: var(--text-muted);
}

.btn-revert:hover {
background: var(--bg-hover);
color: var(--text-default);
}

.field {
display: flex;
flex-direction: column;
gap: 4px;
margin-bottom: 14px;
}

.field-label {
font-size: 12px;
color: var(--text-muted);
}

.field input[type='text'] {
padding: 6px 10px;
border: 1px solid var(--border-strong);
border-radius: 4px;
background: var(--bg-secondary);
color: var(--text-default);
font-size: 13px;
max-width: 400px;
}

.field input[type='text']:focus {
outline: none;
border-color: var(--accent-bg);
}

.field-error {
color: var(--color-danger);
font-size: 11px;
}

.field-warning {
color: var(--color-warning);
font-size: 11px;
}

@media (max-width: 600px) {
.config-section {
padding: 12px 16px;
}
}
</style>
