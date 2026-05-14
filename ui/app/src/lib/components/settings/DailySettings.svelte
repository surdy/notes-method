<script lang="ts">
import type { VaultConfigData } from '$lib/api';
import {
textField,
toggleField,
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

let {
cfg,
fieldError,
fieldWarning,
sectionIsDirty,
saveSection,
revert,
markDirty,
saveImmediate
}: Props = $props();
</script>

<section class="config-section">
{#if sectionIsDirty('daily')}
<div class="section-actions">
<button type="button" class="btn-save" onclick={() => void saveSection('daily')}>Save</button>
<button type="button" class="btn-revert" onclick={() => revert('daily')}>Revert</button>
</div>
{/if}
<label class="field">
<span class="field-label">Folder</span>
<input
type="text"
{...textField(markDirty, 'daily', cfg.daily.folder, (v) => {
cfg.daily.folder = v;
})}
/>
{#if fieldError('daily.folder')}
<span class="field-error">{fieldError('daily.folder')}</span>
{/if}
{#if fieldWarning('daily.folder')}
<span class="field-warning">{fieldWarning('daily.folder')}</span>
{/if}
</label>
<label class="field">
<span class="field-label">Template</span>
<input
type="text"
{...textField(markDirty, 'daily', cfg.daily.template, (v) => {
cfg.daily.template = v;
})}
/>
</label>
<label class="field">
<span class="field-label">Generate At (HH:MM)</span>
<input
type="text"
placeholder="e.g. 06:00"
{...textField(markDirty, 'daily', cfg.daily.generate_at, (v) => {
cfg.daily.generate_at = v || null;
})}
/>
{#if fieldError('daily.generate_at')}
<span class="field-error">{fieldError('daily.generate_at')}</span>
{/if}
</label>
<label class="field">
<span class="field-label">Timezone</span>
<input
type="text"
placeholder="e.g. America/New_York"
{...textField(markDirty, 'daily', cfg.daily.timezone, (v) => {
cfg.daily.timezone = v || null;
})}
/>
{#if fieldError('daily.timezone')}
<span class="field-error">{fieldError('daily.timezone')}</span>
{/if}
</label>
<label class="field field-toggle">
<span class="field-label">Catch Up Missed Days</span>
<input
type="checkbox"
{...toggleField(saveImmediate, 'daily', cfg.daily.catch_up, (v) => {
cfg.daily.catch_up = v;
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

.field-toggle {
flex-direction: row;
align-items: center;
gap: 10px;
}

.field-toggle input[type='checkbox'] {
order: -1;
width: 16px;
height: 16px;
accent-color: #264f78;
}

.field-toggle .field-label {
order: 1;
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
