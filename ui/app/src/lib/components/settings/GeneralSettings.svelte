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
border: 1px solid var(--border-color, #444);
font-size: 12px;
cursor: pointer;
}

.btn-save {
background: #264f78;
color: #fff;
border-color: #264f78;
}

.btn-save:hover {
background: #2d5f8e;
}

.btn-revert {
background: transparent;
color: var(--text-muted, #888);
}

.btn-revert:hover {
background: var(--hover-bg, #2a2d2e);
color: var(--text-primary, #e0e0e0);
}

.field {
display: flex;
flex-direction: column;
gap: 4px;
margin-bottom: 14px;
}

.field-label {
font-size: 12px;
color: var(--text-muted, #888);
}

.field input[type='text'] {
padding: 6px 10px;
border: 1px solid var(--border-color, #444);
border-radius: 4px;
background: var(--bg-secondary, #2a2a2a);
color: var(--text-primary, #e0e0e0);
font-size: 13px;
max-width: 400px;
}

.field input[type='text']:focus {
outline: none;
border-color: #264f78;
}

.field-error {
color: #ff6b6b;
font-size: 11px;
}

.field-warning {
color: #f5c842;
font-size: 11px;
}

@media (max-width: 600px) {
.config-section {
padding: 12px 16px;
}
}
</style>
