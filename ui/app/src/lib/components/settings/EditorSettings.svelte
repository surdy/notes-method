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
accent-color: #264f78;
}

.field-toggle .field-label {
order: 1;
}

.field-label {
font-size: 12px;
color: var(--text-muted, #888);
}

.field select {
padding: 6px 10px;
border: 1px solid var(--border-color, #444);
border-radius: 4px;
background: var(--bg-secondary, #2a2a2a);
color: var(--text-primary, #e0e0e0);
font-size: 13px;
max-width: 400px;
}

.field select:focus {
outline: none;
border-color: #264f78;
}

@media (max-width: 600px) {
.config-section {
padding: 12px 16px;
}
}
</style>
