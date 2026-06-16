<script lang="ts">
	import { onDestroy, onMount, tick } from 'svelte';
	import { goto } from '$app/navigation';
	import { base } from '$app/paths';
	import { ChatStore } from '$lib/agent/chat-store.svelte';
	import { createAgentClient } from '$lib/agent/agent-client';
	import { breakGlassStore } from '$lib/agent/break-glass.svelte';
	import { settingsRoute } from '$lib/vault-menu';
	import { vaultStore } from '$lib/stores.svelte';
	import ChatMessage from './ChatMessage.svelte';
	import ToolCallCard from './ToolCallCard.svelte';
	import PermissionPrompt from './PermissionPrompt.svelte';
	import SlashCommandPalette from './SlashCommandPalette.svelte';
	import MentionAutocomplete from './MentionAutocomplete.svelte';
	import ContextPills from './ContextPills.svelte';
	import { listPrompts } from '$lib/api/prompts';
	import {
		filterSlashCommands,
		parseSlashQuery,
		slashCommandsFromPrompts,
		type SlashCommand
	} from '$lib/agent/slash-commands';
	import {
		addAttachment,
		assembleContextText,
		filterAttachments,
		parseMentionTrigger,
		removeAttachment,
		type Attachment
	} from '$lib/agent/context-attachments';
	import type { EditorContext } from '$lib/agent/types';
	import { tabStore } from '$lib/tab-store.svelte';
	import { activeEditorStore } from '$lib/editor/active-editor.svelte';
	import { activeSession } from '$lib/agent/active-session.svelte';
	import { listFolderPickerItems } from '$lib/folder-notes';
	import { toastStore } from '$lib/toast-store.svelte';

	let { collapsed = false }: { collapsed?: boolean } = $props();

	let store = $state<ChatStore | null>(null);
	let showThreads = $state(false);
	let listEl = $state<HTMLDivElement | null>(null);
	let currentVault = $state('');

	// Slash-command palette state. Commands are fetched lazily from the merged
	// prompt store (defaults + vault `_prompts/`) the first time the palette
	// opens, then cached per vault so the dropdown is instant on subsequent `/`.
	let slashCommands = $state<SlashCommand[]>([]);
	let slashLoadedVault = $state<string | null>(null);
	let slashSelected = $state(0);

	const slashQuery = $derived(store ? parseSlashQuery(store.input) : { active: false, query: '' });
	const slashFiltered = $derived(
		slashQuery.active ? filterSlashCommands(slashCommands, slashQuery.query) : []
	);
	const slashOpen = $derived(slashQuery.active && slashFiltered.length > 0);

	// Load the command set lazily when the palette becomes active. Degrades to an
	// empty list on failure so a missing daemon never throws into the composer.
	$effect(() => {
		if (!slashQuery.active) return;
		const vault = currentVault;
		if (!vault || slashLoadedVault === vault) return;
		slashLoadedVault = vault;
		void listPrompts(vault)
			.then((prompts) => {
				slashCommands = slashCommandsFromPrompts(prompts);
			})
			.catch(() => {
				slashCommands = [];
			});
	});

	// Keep the highlighted item within bounds as the filtered list changes.
	$effect(() => {
		if (slashSelected >= slashFiltered.length) slashSelected = 0;
	});

	// A fresh vault invalidates the cached command set.
	$effect(() => {
		const vault = vaultStore.currentVault;
		if (vault && slashLoadedVault && vault !== slashLoadedVault) {
			slashLoadedVault = null;
			slashCommands = [];
		}
	});

	// Insert (not auto-send): replace the `/token` with the prompt body so the
	// user can add detail before pressing Send. The AC allows "sends/inserts".
	function selectSlashCommand(command: SlashCommand) {
		if (!store) return;
		store.input = command.body;
		slashSelected = 0;
	}

	// --- @-mention context attachments (issue 197) ---------------------------
	// Attached references (note/folder/tag/url) the agent resolves via its MCP
	// read/list tools; the frontend only attaches references, never note bodies.
	let attachments = $state<Attachment[]>([]);
	// Active note auto-include toggle. Selection (when present) always rides along
	// via the same EditorContext path.
	let includeActiveNote = $state(true);
	let inputEl = $state<HTMLTextAreaElement | null>(null);
	let caret = $state(0);
	let mentionSelected = $state(0);

	function syncCaret() {
		caret = inputEl?.selectionStart ?? (store?.input.length ?? 0);
	}

	const mention = $derived(
		store && !slashQuery.active
			? parseMentionTrigger(store.input, caret)
			: { active: false, kind: null, query: '', start: caret }
	);

	// Build kind-aware candidates from frontend stores only. Each source degrades
	// to an empty list so a missing/empty vault never throws into the composer.
	const mentionItems = $derived.by((): Attachment[] => {
		if (!mention.active) return [];
		const kind = mention.kind ?? 'note';
		if (kind === 'note') {
			const candidates = vaultStore.notes.map((n) => ({
				kind: 'note' as const,
				value: n.path,
				label: n.path
			}));
			return filterAttachments(candidates, mention.query).slice(0, 8);
		}
		if (kind === 'folder') {
			const candidates = listFolderPickerItems(vaultStore.tree).map((f) => ({
				kind: 'folder' as const,
				value: f.id,
				label: f.id
			}));
			return filterAttachments(candidates, mention.query).slice(0, 8);
		}
		if (kind === 'tag') {
			const seen = new Set<string>();
			for (const n of vaultStore.notes) {
				for (const t of n.tags ?? []) seen.add(t);
			}
			const candidates = [...seen].sort().map((t) => ({
				kind: 'tag' as const,
				value: t,
				label: `#${t}`
			}));
			return filterAttachments(candidates, mention.query).slice(0, 8);
		}
		// @url has no autocomplete list: synthesize the typed URL as a single item.
		const url = mention.query.trim();
		return url ? [{ kind: 'url' as const, value: url, label: url }] : [];
	});

	const mentionOpen = $derived(mention.active && mentionItems.length > 0);

	// Keep the highlighted mention item within bounds as the list changes.
	$effect(() => {
		if (mentionSelected >= mentionItems.length) mentionSelected = 0;
	});

	// Replace the in-progress `@token` with nothing and add a pill. Strip-and-pill
	// keeps the composer text clean; the reference travels in the context block.
	function selectMention(item: Attachment) {
		if (!store) return;
		const trigger = parseMentionTrigger(store.input, caret);
		const start = trigger.active ? trigger.start : caret;
		store.input = store.input.slice(0, start) + store.input.slice(caret);
		attachments = addAttachment(attachments, item);
		mentionSelected = 0;
		caret = start;
		void tick().then(() => {
			inputEl?.focus();
			inputEl?.setSelectionRange(start, start);
		});
	}

	function removePill(attachment: Attachment) {
		attachments = removeAttachment(attachments, attachment.kind, attachment.value);
	}

	// Active note + current selection flow to the agent via EditorContext (issue 196).
	function buildEditorContext(): EditorContext {
		const activeNote = includeActiveNote ? (tabStore.selectedPath ?? null) : null;
		let selection: string | null = null;
		const view = activeEditorStore.view;
		if (view) {
			const sel = view.state.selection.main;
			if (!sel.empty) selection = view.state.sliceDoc(sel.from, sel.to);
		}
		return { activeNote, selection };
	}

	async function submit() {
		if (!store) return;
		const editor = buildEditorContext();
		const preamble = assembleContextText(attachments);
		await store.send(editor, preamble);
		attachments = [];
	}

	async function forkThread(threadId: string) {
		if (!store) return;
		try {
			await store.forkThread(threadId);
			showThreads = false;
			toastStore.add('Conversation forked', 'success');
		} catch (err) {
			toastStore.add(err instanceof Error ? err.message : 'Failed to fork conversation', 'error');
		}
	}

	async function exportThread(threadId: string) {
		if (!store) return;
		try {
			const path = await store.exportThread(threadId);
			toastStore.add(`Exported to ${path}`, 'success');
		} catch (err) {
			toastStore.add(
				err instanceof Error ? err.message : 'Failed to export conversation',
				'error'
			);
		}
	}

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
		activeSession.set(null);
		const next = new ChatStore(vault, createAgentClient(), {
			breakGlass: () => breakGlassStore.enabled,
			applyToEditor: (mode, text) => activeEditorStore.applyOutput(mode, text)
		});
		next.start();
		store = next;
		activeSession.set(next);
		await next.loadAgents();
		await next.loadThreads();
		// Establish the session up front so the model picker is available before
		// the first message (ACP exposes models only after session/new).
		void next.prepareSession();
	}

	onDestroy(() => {
		store?.dispose();
		activeSession.set(null);
	});

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
		await submit();
	}

	function onKeydown(e: KeyboardEvent) {
		if (slashOpen) {
			if (e.key === 'ArrowDown') {
				e.preventDefault();
				slashSelected = (slashSelected + 1) % slashFiltered.length;
				return;
			}
			if (e.key === 'ArrowUp') {
				e.preventDefault();
				slashSelected = (slashSelected - 1 + slashFiltered.length) % slashFiltered.length;
				return;
			}
			if (e.key === 'Enter' && !e.shiftKey) {
				e.preventDefault();
				const command = slashFiltered[slashSelected];
				if (command) selectSlashCommand(command);
				return;
			}
			if (e.key === 'Escape') {
				e.preventDefault();
				// Drop the leading `/` so the palette closes without losing other text.
				if (store) store.input = store.input.replace(/^\/\S*/, '');
				return;
			}
		}
		if (mentionOpen) {
			if (e.key === 'ArrowDown') {
				e.preventDefault();
				mentionSelected = (mentionSelected + 1) % mentionItems.length;
				return;
			}
			if (e.key === 'ArrowUp') {
				e.preventDefault();
				mentionSelected = (mentionSelected - 1 + mentionItems.length) % mentionItems.length;
				return;
			}
			if (e.key === 'Enter' && !e.shiftKey) {
				e.preventDefault();
				const item = mentionItems[mentionSelected];
				if (item) selectMention(item);
				return;
			}
			if (e.key === 'Escape') {
				e.preventDefault();
				// Drop the in-progress `@token` so the dropdown closes.
				if (store) store.input = store.input.slice(0, mention.start) + store.input.slice(caret);
				caret = mention.start;
				return;
			}
		}
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			void submit();
		}
	}

	// Available agents first so the picker defaults to something launchable;
	// unavailable ones follow, shown disabled and labelled "(not found)".
	const sortedAgents = $derived(
		store
			? [...store.agents].sort((a, b) => Number(b.available) - Number(a.available))
			: []
	);

	function openSettings() {
		void goto(settingsRoute(base, vaultStore.currentVault));
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
					{#if store.agents.length === 0}
						<div class="no-agents">
							<span class="no-agents-text">
								No agent CLI found — install Copilot, Claude, Codex, or Gemini, or
								configure one in Settings.
							</span>
							<button type="button" class="link-btn" onclick={openSettings}>
								Open AI Agent settings
							</button>
						</div>
					{:else}
						<select
							class="picker"
							aria-label="Agent"
							value={store.selectedAgent ?? ''}
							onchange={(e) => store?.selectAgent(e.currentTarget.value)}
						>
							{#each sortedAgents as agent (agent.id)}
								<option
									value={agent.id}
									disabled={!agent.available && agent.id !== store.selectedAgent}
								>
									{agent.name}{agent.available ? '' : ' (not found)'}
								</option>
							{/each}
						</select>
					{/if}

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
								class="thread-act"
								type="button"
								aria-label="Fork conversation"
								title="Fork conversation"
								onclick={() => { void forkThread(thread.id); }}
							>
								⑂
							</button>
							<button
								class="thread-act"
								type="button"
								aria-label="Export conversation to note"
								title="Export conversation to note"
								onclick={() => { void exportThread(thread.id); }}
							>
								↗
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
				{#if slashOpen}
					<div class="slash-anchor">
						<SlashCommandPalette
							commands={slashFiltered}
							selected={slashSelected}
							onselect={selectSlashCommand}
							onhover={(i) => (slashSelected = i)}
						/>
					</div>
				{:else if mentionOpen}
					<div class="slash-anchor">
						<MentionAutocomplete
							items={mentionItems}
							kind={mention.kind}
							selected={mentionSelected}
							onselect={selectMention}
							onhover={(i) => (mentionSelected = i)}
						/>
					</div>
				{/if}
				<ContextPills
					{attachments}
					{includeActiveNote}
					activeNotePath={tabStore.selectedPath}
					onremove={removePill}
					ontoggleactivenote={(next) => (includeActiveNote = next)}
				/>
				<div class="composer-row">
					<textarea
						class="input"
						rows="2"
						placeholder="Message the agent… (@ to attach context)"
						bind:this={inputEl}
						bind:value={store.input}
						onkeydown={onKeydown}
						oninput={syncCaret}
						onclick={syncCaret}
						onkeyup={syncCaret}
						disabled={store.busy}
					></textarea>
					{#if store.busy}
						<button class="send stop" type="button" onclick={() => store?.stop()}>
							Stop
						</button>
					{:else}
						<button
							class="send"
							type="submit"
							disabled={store.input.trim().length === 0}
						>
							Send
						</button>
					{/if}
				</div>
				{#if store.canRegenerate}
					<div class="turn-actions">
						<button class="link-btn" type="button" onclick={() => store?.regenerate()}>
							↻ Regenerate
						</button>
					</div>
				{/if}
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

	.no-agents {
		display: flex;
		flex-direction: column;
		gap: 4px;
		max-width: 320px;
	}

	.no-agents-text {
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.4;
	}

	.link-btn {
		align-self: flex-start;
		padding: 0;
		border: none;
		background: none;
		color: var(--accent-bg);
		font-size: 12px;
		cursor: pointer;
		text-decoration: underline;
	}

	.link-btn:hover {
		color: var(--accent-bg);
		text-decoration: none;
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

	.thread-act {
		padding: 8px 8px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		font-size: 12px;
		cursor: pointer;
	}

	.thread-act:hover {
		color: var(--text-default);
		background: var(--bg-hover);
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
		flex-direction: column;
		gap: 8px;
		padding: 10px 12px;
		border-top: 1px solid var(--border-default);
		position: relative;
	}

	.composer-row {
		display: flex;
		gap: 8px;
	}

	.slash-anchor {
		position: absolute;
		left: 12px;
		right: 12px;
		bottom: calc(100% - 4px);
		z-index: 10;
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

	.send.stop {
		border-color: var(--danger-border);
		background: var(--danger-surface);
		color: var(--danger-text);
	}

	.send.stop:hover {
		background: var(--danger-bg);
	}

	.turn-actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 6px;
	}
</style>
