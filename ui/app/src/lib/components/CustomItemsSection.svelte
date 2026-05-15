<script lang="ts">
import type { CustomItem } from '$lib/api';

let {
items,
onActivateItem,
activeItemName
}: {
items: CustomItem[];
onActivateItem: (item: CustomItem) => void;
activeItemName: string | null;
} = $props();
</script>

<div class="custom-items-section">
{#each items as item (item.name)}
<button
class="item"
class:active={activeItemName === item.name}
onclick={() => onActivateItem(item)}
type="button"
>
<span class="item-icon">{item.icon}</span>
<span class="item-name">{item.name}</span>
</button>
{:else}
<div class="empty">No items</div>
{/each}
</div>

<style>
.custom-items-section {
display: flex;
flex-direction: column;
}

.item {
display: flex;
align-items: center;
gap: 8px;
width: 100%;
padding: 6px 12px;
border: none;
background: none;
color: var(--ns-text-secondary);
font-size: 13px;
text-align: left;
cursor: pointer;
}

.item:hover {
background: var(--ns-surface-hover);
}

.item.active {
background: var(--ns-selected-bg);
color: var(--ns-text-inverse);
}

.item-icon {
font-size: 14px;
flex-shrink: 0;
}

.item-name {
overflow: hidden;
text-overflow: ellipsis;
white-space: nowrap;
}

.empty {
padding: 8px 12px;
font-size: 12px;
color: var(--ns-text-muted);
}
</style>
