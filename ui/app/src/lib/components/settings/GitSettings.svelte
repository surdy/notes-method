<script lang="ts">
import type { VaultConfigData } from '$lib/api';
import {
textField,
toggleField,
type FieldErrorFn,
type MarkDirtyFn,
type RevertFn,
type SaveImmediateFn,
type SaveSectionFn,
type SectionIsDirtyFn
} from '$lib/settings-helpers';

interface Props {
cfg: VaultConfigData;
fieldError: FieldErrorFn;
sectionIsDirty: SectionIsDirtyFn;
saveSection: SaveSectionFn;
revert: RevertFn;
markDirty: MarkDirtyFn;
saveImmediate: SaveImmediateFn;
}

let { cfg, fieldError, sectionIsDirty, saveSection, revert, markDirty, saveImmediate }: Props =
$props();
</script>

<section class="config-section">
{#if sectionIsDirty('git')}
<div class="section-actions">
<button type="button" class="btn-save" onclick={() => void saveSection('git')}>Save</button>
<button type="button" class="btn-revert" onclick={() => revert('git')}>Revert</button>
</div>
{/if}
<label class="field field-toggle">
<span class="field-label">Enabled</span>
<input
type="checkbox"
{...toggleField(saveImmediate, 'git', cfg.git.enabled, (v) => {
cfg.git.enabled = v;
})}
/>
</label>
<label class="field">
<span class="field-label">Auto-commit Interval</span>
<input
type="text"
placeholder="e.g. 5m"
{...textField(markDirty, 'git', cfg.git.auto_commit_every, (v) => {
cfg.git.auto_commit_every = v || null;
})}
/>
{#if fieldError('git.auto_commit_every')}
<span class="field-error">{fieldError('git.auto_commit_every')}</span>
{/if}
</label>
<label class="field">
<span class="field-label">Auto-pull Interval</span>
<input
type="text"
placeholder="e.g. 5m"
{...textField(markDirty, 'git', cfg.git.auto_pull_every, (v) => {
cfg.git.auto_pull_every = v || null;
})}
/>
{#if fieldError('git.auto_pull_every')}
<span class="field-error">{fieldError('git.auto_pull_every')}</span>
{/if}
</label>
<label class="field">
<span class="field-label">Auto-push Interval</span>
<input
type="text"
placeholder="e.g. 5m"
{...textField(markDirty, 'git', cfg.git.auto_push_every, (v) => {
cfg.git.auto_push_every = v || null;
})}
/>
{#if fieldError('git.auto_push_every')}
<span class="field-error">{fieldError('git.auto_push_every')}</span>
{/if}
</label>
<label class="field">
<span class="field-label">Commit Message</span>
<input
type="text"
placeholder="e.g. auto: sync changes"
{...textField(markDirty, 'git', cfg.git.commit_message, (v) => {
cfg.git.commit_message = v || null;
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

@media (max-width: 600px) {
.config-section {
padding: 12px 16px;
}
}
</style>
