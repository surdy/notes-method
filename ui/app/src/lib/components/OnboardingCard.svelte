<script lang="ts">
import { onMount } from 'svelte';
import { fade } from 'svelte/transition';

const STORAGE_KEY = 'notesmith:onboarding-dismissed';

let ready = $state(false);
let dismissed = $state(false);

onMount(() => {
try {
dismissed = localStorage.getItem(STORAGE_KEY) === 'true';
} catch {
dismissed = false;
}

ready = true;
});

function dismiss() {
dismissed = true;
try {
localStorage.setItem(STORAGE_KEY, 'true');
} catch {
// ignore storage failures
}
}
</script>

{#if ready && !dismissed}
<div
class="onboarding-card"
role="complementary"
aria-label="Welcome to Notesmith"
in:fade={{ duration: 220 }}
>
<div class="onboarding-content">
<div class="onboarding-header">
<div class="onboarding-icon" aria-hidden="true">📝</div>
<div>
<h3>Welcome to Notesmith</h3>
<p>
Notesmith runs a small background service that keeps your CLI, agents, and app in
sync. Your markdown files stay on disk — the service just indexes them for fast
search, live preview, and real-time updates.
</p>
</div>
</div>
<p class="onboarding-hint">
You'll see a status indicator in the bottom-left corner of the sidebar. It shows
whether the service is connected and healthy.
</p>
<button class="onboarding-dismiss" type="button" onclick={dismiss}>Got it</button>
</div>
</div>
{/if}

<style>
.onboarding-card {
margin: 8px 10px 10px;
padding: 12px;
border: 1px solid color-mix(in srgb, var(--border-color, #333) 88%, white 12%);
border-radius: 10px;
background: color-mix(in srgb, var(--sidebar-bg, #252526) 82%, white 18%);
box-shadow: 0 10px 24px rgba(0, 0, 0, 0.12);
}

.onboarding-content {
display: grid;
gap: 10px;
}

.onboarding-header {
display: grid;
grid-template-columns: auto 1fr;
gap: 10px;
align-items: start;
}

.onboarding-icon {
display: flex;
align-items: center;
justify-content: center;
width: 28px;
height: 28px;
border-radius: 8px;
background: color-mix(in srgb, var(--selected-bg, #094771) 22%, transparent);
font-size: 16px;
line-height: 1;
}

.onboarding-card h3 {
margin: 0 0 4px;
font-size: 14px;
font-weight: 600;
color: var(--text-primary, #e0e0e0);
}

.onboarding-card p {
margin: 0;
font-size: 12px;
line-height: 1.45;
color: var(--text-secondary, #cccccc);
}

.onboarding-hint {
color: var(--text-muted, #888);
}

.onboarding-dismiss {
justify-self: start;
padding: 6px 12px;
border: 1px solid color-mix(in srgb, var(--selected-bg, #094771) 70%, white 30%);
border-radius: 6px;
background: var(--selected-bg, #094771);
color: white;
font-size: 12px;
font-weight: 600;
cursor: pointer;
}

.onboarding-dismiss:hover {
filter: brightness(1.08);
}
</style>
