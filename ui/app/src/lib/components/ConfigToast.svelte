<script lang="ts">
let toasts = $state<Array<{ id: number; message: string; type: 'info' | 'error' }>>([]);
let nextId = 0;

export function show(message: string, type: 'info' | 'error' = 'info') {
	const id = nextId++;
	toasts.push({ id, message, type });
	setTimeout(() => {
		toasts = toasts.filter((t) => t.id !== id);
	}, type === 'error' ? 8000 : 3000);
}
</script>

{#if toasts.length > 0}
<div class="toast-container">
	{#each toasts as toast (toast.id)}
	<div class="toast toast-{toast.type}">
		<span class="toast-message">{toast.message}</span>
		<button class="toast-close" onclick={() => (toasts = toasts.filter((t) => t.id !== toast.id))}>×</button>
	</div>
	{/each}
</div>
{/if}

<style>
.toast-container {
	position: fixed;
	bottom: 16px;
	right: 16px;
	z-index: 9999;
	display: flex;
	flex-direction: column;
	gap: 8px;
}
.toast {
	display: flex;
	align-items: center;
	gap: 8px;
	padding: 10px 14px;
	border-radius: 6px;
	font-size: 13px;
	box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2);
	max-width: 400px;
	animation: slide-in 0.2s ease-out;
}
.toast-info {
	background: var(--bg-secondary, #2a2a2a);
	color: var(--text-primary, #e0e0e0);
	border: 1px solid var(--border-color, #444);
}
.toast-error {
	background: #3a1a1a;
	color: #ff6b6b;
	border: 1px solid #5a2a2a;
}
.toast-close {
	background: none;
	border: none;
	color: inherit;
	cursor: pointer;
	font-size: 16px;
	padding: 0 2px;
	opacity: 0.6;
}
.toast-close:hover {
	opacity: 1;
}
@keyframes slide-in {
	from {
		transform: translateX(20px);
		opacity: 0;
	}
	to {
		transform: translateX(0);
		opacity: 1;
	}
}
</style>
