<script lang="ts">
	import { onDestroy, onMount, tick } from 'svelte';
	import { ChatStore } from '$lib/agent/chat-store.svelte';
	import { createAgentClient } from '$lib/agent/agent-client';
	import { breakGlassStore } from '$lib/agent/break-glass.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import ChatMessage from './ChatMessage.svelte';
	import ToolCallCard from './ToolCallCard.svelte';
	import PermissionPrompt from './PermissionPrompt.svelte';

	let { collapsed = false }: { collapsed?: boolean } = $props();

	let store = $state<ChatStore | null>(null);
	let showThreads = $state(false);
	let listEl = $state<HTMLDivElement | null>(null);
	let currentVault = $state('');

	onMount(() => {
		breakGlassStore.load();
		void initFor(vaultStore.currentVault);
	});

	// Rebuild the store when the active vault changes.
	$effect(() => {
		const vault = vaultStore.currentVault;
		if (vault && vault !== currentVault) {
			void initFor(vault);
		}
	});

	async function initFor(vault: string) {
		if (!vault) return;
		currentVault = vault;
		store?.dispose();
		const next = new ChatStore(vault, createAgentClient(), {
			breakGlass: () => breakGlassStore.enabled
		});
		next.start();
		store = next;
		await next.loadAgents();
		await next.loadThreads();
	}

	onDestroy(() => store?.dispose());

	// Autoscroll on new items.
	$effect(() => {
		const _ = store?.conversation.items.length;
		void _;
		void tick().then(() => {
			if (listEl) listEl.scrollTop = listEl.scrollHeight;
		});
	});

	async function onSubmit(e: SubmitEvent) {
		e.preventDefault();
		await store?.send();
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			void store?.send();
		}
	}
</script>

<div class="agent-shell" class:collapsed>
	<div class="agent-panel">
		{#if !store}
			<div class="empty">Loading agent…</div>
		{:else if !store.available}
			<div class="empty">
				The AI agent is only available in the Notesmith desktop app.
			</div>
		{:else}
			<header class="bar">
				<div class="controls">
					<select
						class="picker"
						aria-label="Agent"
						value={store.selectedAgent ?? ''}
						onchange={(e) => store?.selectAgent(e.currentTarget.value)}
					>
						{#each store.agents as agent (agent.id)}
							<option value={agent.id} disabled={!agent.available}>
								{agent.name}{agent.available ? '' : ' (unavailable)'}
							</option>
						{/each}
					</select>

					{#if store.modelPicker}
						<select
							class="picker"
							aria-label="Model"
							value={store.selectedModel ?? store.modelPicker.current}
							onchange={(e) => void store?.selectModel(e.currentTarget.value)}
						>
							{#each store.modelPicker.options as model (model.id)}
								<option value={model.id}>{model.name}</option>
							{/each}
						</select>
					{/if}

					<button
						class="mode-toggle"
						class:ro={store.readOnly}
						type="button"
						onclick={() => void store?.toggleReadOnly()}
						title={store.readOnly ? 'Read-only — click to allow writes' : 'Read-write — click to lock'}
					>
						{store.readOnly ? 'Read-only' : 'Read-write'}
					</button>

					<button
						class="threads-toggle"
						type="button"
						aria-pressed={showThreads}
						onclick={() => (showThreads = !showThreads)}
						title="Conversations"
					>
						☰
					</button>
				</div>
				<div class="badge" title="Agent operating scope">
					Operating on <strong>{currentVault}</strong> ·
					<span class:ro={store.readOnly}>{store.readOnly ? 'read-only' : 'read-write'}</span>
				</div>
			</header>

			{#if showThreads}
				<div class="threads">
					<button class="thread-new" type="button" onclick={() => { store?.newThread(); showThreads = false; }}>
						+ New conversation
					</button>
					{#each store.threads as thread (thread.id)}
						<div class="thread-row" class:active={thread.id === store.currentThreadId}>
							<button
								class="thread-open"
								type="button"
								onclick={() => { void store?.openThread(thread.id); showThreads = false; }}
							>
								{thread.title}
							</button>
							<button
								class="thread-del"
								type="button"
								aria-label="Delete conversation"
								onclick={() => void store?.deleteThread(thread.id)}
							>
								✕
							</button>
						</div>
					{/each}
					{#if store.threads.length === 0}
						<div class="thread-empty">No saved conversations yet.</div>
					{/if}
				</div>
			{/if}

			<div class="messages" bind:this={listEl}>
				{#if store.conversation.items.length === 0}
					<div class="empty">Ask the agent about your vault.</div>
				{/if}
				{#each store.conversation.items as item (item.id)}
					{#if item.kind === 'message'}
						<ChatMessage {item} />
					{:else if item.kind === 'tool'}
						<ToolCallCard {item} />
					{:else if item.kind === 'status'}
						<div class="status">{item.message}</div>
					{:else if item.kind === 'error'}
						<div class="error">{item.message}</div>
					{/if}
				{/each}
			</div>

			{#if store.pendingPermission}
				<PermissionPrompt
					request={store.pendingPermission}
					ondecide={(d) => void store?.answerPermission(d)}
				/>
			{/if}

			{#if store.errorMessage}
				<div class="error inline">{store.errorMessage}</div>
			{/if}

			<form class="composer" onsubmit={onSubmit}>
				<textarea
					class="input"
					rows="2"
					placeholder="Message the agent…"
					bind:value={store.input}
					onkeydown={onKeydown}
					disabled={store.busy}
				></textarea>
				<button class="send" type="submit" disabled={store.busy || store.input.trim().length === 0}>
					{store.busy ? '…' : 'Send'}
				</button>
			</form>
		{/if}
	</div>
</div>

<style>
	.agent-shell {
		width: 100%;
		height: 100%;
		overflow: hidden;
	}

	.agent-panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg-secondary);
		border-left: 1px solid var(--border-default);
	}

	.collapsed .agent-panel {
		visibility: hidden;
		pointer-events: none;
	}

	.bar {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-default);
	}

	.controls {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		gap: 6px;
	}

	.picker {
		padding: 4px 8px;
		border: 1px solid var(--border-strong);
		border-radius: 6px;
		background: var(--bg-input);
		color: var(--text-default);
		font-size: 12px;
	}

	.picker:focus {
		outline: none;
		border-color: var(--accent-bg);
	}

	.mode-toggle,
	.threads-toggle {
		padding: 4px 10px;
		border: 1px solid var(--border-strong);
		border-radius: 6px;
		background: var(--button-bg);
		color: var(--button-text);
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
	}

	.mode-toggle:hover,
	.threads-toggle:hover {
		background: var(--button-hover);
	}

	.mode-toggle.ro {
		border-color: var(--warning-border);
		background: var(--warning-bg);
		color: var(--warning-text);
	}

	.threads-toggle {
		margin-left: auto;
	}

	.badge {
		font-size: 11px;
		color: var(--text-muted);
	}

	.badge strong {
		color: var(--text-default);
	}

	.badge .ro {
		color: var(--warning-text);
		font-weight: 600;
	}

	.threads {
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border-default);
		background: var(--bg-default);
		max-height: 200px;
		overflow-y: auto;
	}

	.thread-new {
		padding: 8px 12px;
		border: none;
		background: transparent;
		color: var(--accent);
		text-align: left;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
	}

	.thread-new:hover {
		background: var(--bg-hover);
	}

	.thread-row {
		display: flex;
		align-items: center;
	}

	.thread-row.active {
		background: var(--bg-selected);
	}

	.thread-open {
		flex: 1;
		padding: 8px 12px;
		border: none;
		background: transparent;
		color: var(--text-default);
		text-align: left;
		font-size: 12px;
		cursor: pointer;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.thread-open:hover {
		background: var(--bg-hover);
	}

	.thread-del {
		padding: 8px 10px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		font-size: 11px;
		cursor: pointer;
	}

	.thread-del:hover {
		color: var(--danger-text);
	}

	.thread-empty {
		padding: 10px 12px;
		font-size: 12px;
		color: var(--text-muted);
	}

	.messages {
		flex: 1;
		overflow-y: auto;
		padding: 8px 0;
	}

	.empty {
		padding: 24px 16px;
		color: var(--text-muted);
		font-size: 13px;
		text-align: center;
	}

	.status {
		padding: 4px 16px;
		font-size: 11px;
		color: var(--text-muted);
		font-style: italic;
	}

	.error {
		margin: 6px 12px;
		padding: 8px 10px;
		border-radius: 6px;
		background: var(--danger-bg-muted);
		color: var(--danger-text-muted);
		font-size: 12px;
	}

	.error.inline {
		margin-top: 0;
	}

	.composer {
		display: flex;
		gap: 8px;
		padding: 10px 12px;
		border-top: 1px solid var(--border-default);
	}

	.input {
		flex: 1;
		resize: none;
		padding: 8px 10px;
		border: 1px solid var(--border-input);
		border-radius: 8px;
		background: var(--bg-input);
		color: var(--text-default);
		font-size: 13px;
		line-height: 1.4;
		font-family: inherit;
	}

	.input:focus {
		outline: none;
		border-color: var(--accent-bg);
	}

	.send {
		align-self: flex-end;
		padding: 8px 14px;
		border: 1px solid var(--accent-bg);
		border-radius: 8px;
		background: var(--accent-bg);
		color: var(--accent-text);
		font-size: 13px;
		font-weight: 600;
		cursor: pointer;
	}

	.send:hover:not(:disabled) {
		background: var(--accent-hover);
	}

	.send:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
</style>
