<script lang="ts">
import type { VaultConfigData } from '$lib/api';
import {
textField,
type MarkDirtyFn,
type RevertFn,
type SaveSectionFn,
type SectionIsDirtyFn
} from '$lib/settings-helpers';

interface Props {
cfg: VaultConfigData;
sectionIsDirty: SectionIsDirtyFn;
saveSection: SaveSectionFn;
revert: RevertFn;
markDirty: MarkDirtyFn;
}

let { cfg, sectionIsDirty, saveSection, revert, markDirty }: Props = $props();
</script>

<section class="config-section">
{#if sectionIsDirty('hooks')}
<div class="section-actions">
<button type="button" class="btn-save" onclick={() => void saveSection('hooks')}>Save</button>
<button type="button" class="btn-revert" onclick={() => revert('hooks')}>Revert</button>
</div>
{/if}
<label class="field">
<span class="field-label">On Note Create</span>
<input
type="text"
placeholder="shell command"
{...textField(markDirty, 'hooks', cfg.hooks.on_note_create, (v) => {
cfg.hooks.on_note_create = v || null;
})}
/>
</label>
<label class="field">
<span class="field-label">On Daily Create</span>
<input
type="text"
placeholder="shell command"
{...textField(markDirty, 'hooks', cfg.hooks.on_daily_create, (v) => {
cfg.hooks.on_daily_create = v || null;
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

@media (max-width: 600px) {
.config-section {
padding: 12px 16px;
}
}
</style>
