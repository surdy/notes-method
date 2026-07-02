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
<p class="field-hint field-hint--toggle">
Enabling git initializes a repository in this vault automatically (with a
minimal <code>.gitignore</code> and an initial commit) if one doesn't exist yet.
</p>
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
<span class="field-label">Commit Message</span>
<input
type="text"
placeholder="e.g. auto: sync changes"
{...textField(markDirty, 'git', cfg.git.commit_message, (v) => {
cfg.git.commit_message = v || null;
})}
/>
</label>
<label class="field">
<span class="field-label">Inactivity Checkpoint</span>
<input
type="text"
placeholder="e.g. 2m"
{...textField(markDirty, 'git', cfg.git.commit_on_inactivity, (v) => {
cfg.git.commit_on_inactivity = v || null;
})}
/>
{#if fieldError('git.commit_on_inactivity')}
<span class="field-error">{fieldError('git.commit_on_inactivity')}</span>
{/if}
<span class="field-hint">
Commit automatically after this much idle time. Leave empty to disable.
</span>
</label>
<div class="subsection">
<h3 class="subsection-title">Remote sync (optional)</h3>
<p class="subsection-hint">
Leave these empty for local-only versioning. When set, they push to and pull
from the <code>origin</code> remote, which must be configured in the vault repo.
</p>
</div>
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

.subsection {
margin: 6px 0 14px;
padding-top: 12px;
border-top: 1px solid var(--border-default);
}

.subsection-title {
margin: 0 0 4px;
font-size: 12px;
font-weight: 600;
color: var(--text-default);
}

.subsection-hint {
margin: 0;
font-size: 11px;
line-height: 1.5;
color: var(--text-muted);
max-width: 400px;
}

.field-hint {
font-size: 11px;
line-height: 1.5;
color: var(--text-muted);
max-width: 400px;
}

.field-hint--toggle {
margin: -4px 0 4px;
}

.field-hint code {
font-family: var(--font-mono);
font-size: 10px;
padding: 1px 4px;
border-radius: 3px;
background: var(--bg-secondary);
}

.subsection-hint code {
font-family: var(--font-mono);
font-size: 10px;
padding: 1px 4px;
border-radius: 3px;
background: var(--bg-secondary);
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

@media (max-width: 600px) {
.config-section {
padding: 12px 16px;
}
}
</style>
