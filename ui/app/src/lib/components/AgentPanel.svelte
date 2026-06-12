<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import {
		applyAgentEvent,
		emptyChatState,
		endSession,
		startedChatState,
		type ChatState
	} from '$lib/agent-chat';
	import {
		agentLocalFileAccessDefault,
		listenForSession,
		mcpEndpoint,
		sendAgentMessage,
		startAgentSession,
		stopAgentSession,
		type AgentKind
	} from '$lib/agent-bridge';
	import { API_BASE } from '$lib/api/core';
	import { vaultStore } from '$lib/stores.svelte';

	const AGENTS: { value: AgentKind; label: string }[] = [
		{ value: 'claude-code', label: 'Claude Code' },
		{ value: 'codex', label: 'Codex' },
		{ value: 'copilot', label: 'Copilot' }
	];

	let chat = $state<ChatState>(emptyChatState());
	let sessionId = $state<string | null>(null);
	let agent = $state<AgentKind>('claude-code');
	let readOnly = $state(true);
	let localFileAccess = $state(false);
	let input = $state('');
	let busy = $state(false);
	let panelError = $state<string | null>(null);
	let teardown: (() => void) | null = null;

	const running = $derived(chat.running);
	const mcpUrl = $derived(mcpEndpoint(API_BASE, vaultStore.currentVault ?? '', readOnly));

	onMount(async () => {
		// Honor the persisted global default as the toggle's starting state
		// (ADR 0012); the per-session toggle then overrides it on start.
		localFileAccess = await agentLocalFileAccessDefault();
	});

	async function send() {
		const message = input.trim();
		if (!message || busy) {
			return;
		}
		const vault = vaultStore.currentVault;
		if (!vault) {
			panelError = 'Select a vault first.';
			return;
		}

		busy = true;
		panelError = null;
		try {
			if (!sessionId) {
				chat = startedChatState();
				const id = await startAgentSession({ vault, agent, mcpUrl, localFileAccess });
				sessionId = id;
				teardown = await listenForSession(id, {
					onEvent: (event) => {
						chat = applyAgentEvent(chat, event);
					},
					onEnded: () => {
						chat = endSession(chat);
						cleanupSession();
					}
				});
			}
			input = '';
			await sendAgentMessage(sessionId, message);
		} catch (cause) {
			panelError = cause instanceof Error ? cause.message : 'Failed to reach the agent.';
			chat = endSession(chat);
			cleanupSession();
		} finally {
			busy = false;
		}
	}

	async function stop() {
		if (!sessionId) {
			return;
		}
		try {
			await stopAgentSession(sessionId);
		} catch (cause) {
			console.warn('failed to stop agent session', cause);
		}
		chat = endSession(chat);
		cleanupSession();
	}

	function reset() {
		void stop();
		chat = emptyChatState();
		input = '';
		panelError = null;
	}

	function cleanupSession() {
		teardown?.();
		teardown = null;
		sessionId = null;
	}

	function onKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
			event.preventDefault();
			void send();
		}
	}

	function formatArgs(args: unknown): string {
		if (args === null || args === undefined) {
			return '';
		}
		try {
			return JSON.stringify(args, null, 2);
		} catch {
			return String(args);
		}
	}

	onDestroy(() => {
		// Terminate any live session so we never orphan the child process.
		if (sessionId) {
			void stopAgentSession(sessionId);
		}
		teardown?.();
	});
</script>

<div class="agent-panel">
	<div class="agent-toolbar">
		<label class="agent-picker">
			<span class="sr-only">Agent</span>
			<select bind:value={agent} disabled={running || busy}>
				{#each AGENTS as option (option.value)}
					<option value={option.value}>{option.label}</option>
				{/each}
			</select>
		</label>
		<label class="agent-scope" title="Read-only lets the agent read the vault; read-write also lets it edit notes.">
			<input type="checkbox" bind:checked={readOnly} disabled={running || busy} />
			<span>Read-only</span>
		</label>
		<label class="agent-scope" title="Grant the agent scoped filesystem and terminal access to the vault directory (off by default).">
			<input type="checkbox" bind:checked={localFileAccess} disabled={running || busy} />
			<span>Local file access</span>
		</label>
		<div class="agent-toolbar-status">
			{#if running}
				<span class="agent-running">● running</span>
			{:else if chat.messages.length > 0}
				<span class="agent-idle">idle</span>
			{/if}
		</div>
		{#if running}
			<button type="button" class="agent-btn" onclick={stop}>Stop</button>
		{:else if chat.messages.length > 0}
			<button type="button" class="agent-btn" onclick={reset}>New chat</button>
		{/if}
	</div>

	{#if mcpUrl}
		<div class="agent-scope-badge">
			operating on <strong>{vaultStore.currentVault}</strong> ·
			<span class:scope-rw={!readOnly}>{readOnly ? 'read-only' : 'read-write'}</span>
		</div>
	{/if}

	<div class="agent-stream">
		{#if chat.messages.length === 0}
			<div class="agent-empty">
				Ask an agent about <strong>{vaultStore.currentVault || 'this vault'}</strong>. The agent
				runs locally with your own CLI credentials.
			</div>
		{:else}
			{#each chat.messages as message, index (index)}
				{#if message.kind === 'text'}
					<div class="msg msg-{message.role}">
						<div class="msg-role">{message.role === 'user' ? 'You' : 'Agent'}</div>
						<div class="msg-text">{message.text}</div>
					</div>
				{:else if message.kind === 'tool'}
					<details class="tool-card" class:tool-error={message.result?.isError} open>
						<summary>
							<span class="tool-name">{message.name}</span>
							{#if message.result}
								<span class="tool-state">{message.result.isError ? 'error' : 'done'}</span>
							{:else}
								<span class="tool-state running">running…</span>
							{/if}
						</summary>
						{#if formatArgs(message.args)}
							<pre class="tool-args">{formatArgs(message.args)}</pre>
						{/if}
						{#if message.result}
							<pre class="tool-result">{message.result.content}</pre>
						{/if}
					</details>
				{:else}
					<div class="msg-status" class:msg-status-error={message.isError}>{message.text}</div>
				{/if}
			{/each}
		{/if}
	</div>

	{#if panelError}
		<div class="agent-panel-error">{panelError}</div>
	{/if}

	<div class="agent-composer">
		<textarea
			bind:value={input}
			onkeydown={onKeydown}
			placeholder="Message the agent…  (⌘/Ctrl+Enter to send)"
			rows="3"
		></textarea>
		<button type="button" class="agent-send" onclick={send} disabled={busy || !input.trim()}>
			{busy ? 'Sending…' : 'Send'}
		</button>
	</div>
</div>

<style>
	.agent-panel {
		display: flex;
		flex-direction: column;
		height: 100%;
		min-height: 0;
	}

	.agent-toolbar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border-default);
	}

	.agent-picker select {
		background: var(--bg-secondary);
		color: var(--text-default);
		border: 1px solid var(--border-default);
		border-radius: 6px;
		padding: 4px 8px;
		font-size: 12px;
	}

	.agent-toolbar-status {
		flex: 1;
		font-size: 12px;
		color: var(--text-muted);
	}

	.agent-scope {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--text-muted);
		cursor: pointer;
	}

	.agent-scope input {
		cursor: pointer;
	}

	.agent-scope-badge {
		padding: 4px 12px;
		font-size: 11px;
		color: var(--text-muted);
		background: var(--bg-secondary);
		border-bottom: 1px solid var(--border-default);
	}

	.agent-scope-badge strong {
		color: var(--text-default);
	}

	.agent-scope-badge .scope-rw {
		color: var(--accent);
	}

	.agent-running {
		color: var(--accent);
	}

	.agent-btn,
	.agent-send {
		background: var(--bg-secondary);
		color: var(--text-default);
		border: 1px solid var(--border-default);
		border-radius: 6px;
		padding: 4px 10px;
		font-size: 12px;
		cursor: pointer;
	}

	.agent-btn:hover,
	.agent-send:hover:not(:disabled) {
		background: var(--bg-hover);
	}

	.agent-stream {
		flex: 1;
		overflow-y: auto;
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.agent-empty {
		color: var(--text-muted);
		font-size: 13px;
		line-height: 1.5;
	}

	.msg {
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.msg-role {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--text-muted);
	}

	.msg-text {
		font-size: 13px;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
		padding: 8px 10px;
		border-radius: 8px;
		background: var(--bg-secondary);
	}

	.msg-user .msg-text {
		background: var(--accent-bg);
		color: var(--accent-text);
	}

	.msg-status {
		font-size: 12px;
		color: var(--text-muted);
		font-style: italic;
	}

	.msg-status-error {
		color: var(--danger-text-muted);
		background: var(--danger-bg-muted);
		padding: 6px 10px;
		border-radius: 6px;
		font-style: normal;
	}

	.tool-card {
		border: 1px solid var(--border-default);
		border-radius: 8px;
		background: var(--bg-secondary);
		font-size: 12px;
		overflow: hidden;
	}

	.tool-card.tool-error {
		border-color: var(--danger-text-muted);
	}

	.tool-card summary {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 6px 10px;
		cursor: pointer;
		list-style: none;
	}

	.tool-name {
		font-weight: 600;
		color: var(--text-default);
	}

	.tool-state {
		color: var(--text-muted);
		font-size: 11px;
	}

	.tool-state.running {
		color: var(--accent);
	}

	.tool-args,
	.tool-result {
		margin: 0;
		padding: 8px 10px;
		border-top: 1px solid var(--border-default);
		white-space: pre-wrap;
		word-break: break-word;
		color: var(--text-muted);
		font-family: var(--font-mono, monospace);
		font-size: 11px;
		max-height: 220px;
		overflow-y: auto;
	}

	.agent-panel-error {
		padding: 8px 12px;
		color: var(--danger-text-muted);
		background: var(--danger-bg-muted);
		font-size: 12px;
	}

	.agent-composer {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 12px;
		border-top: 1px solid var(--border-default);
	}

	.agent-composer textarea {
		resize: vertical;
		background: var(--bg-secondary);
		color: var(--text-default);
		border: 1px solid var(--border-default);
		border-radius: 8px;
		padding: 8px 10px;
		font-size: 13px;
		font-family: inherit;
	}

	.agent-send {
		align-self: flex-end;
	}

	.agent-send:disabled {
		opacity: 0.5;
		cursor: default;
	}

	.sr-only {
		position: absolute;
		width: 1px;
		height: 1px;
		padding: 0;
		margin: -1px;
		overflow: hidden;
		clip: rect(0, 0, 0, 0);
		white-space: nowrap;
		border: 0;
	}
</style>
