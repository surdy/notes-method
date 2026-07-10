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
	import { suggestedPrompts } from '$lib/agent/agent-onboarding';
	import { activeEditorStore } from '$lib/editor/active-editor.svelte';
	import { activeSession } from '$lib/agent/active-session.svelte';
	import { listFolderPickerItems } from '$lib/folder-notes';
	import { toastStore } from '$lib/toast-store.svelte';

	let { collapsed = false }: { collapsed?: boolean } = $props();

	let store = $state<ChatStore | null>(null);
	let showThreads = $state(false);
	let listEl = $state<HTMLDivElement | null>(null);
	let currentVault = $state('');

	// One ChatStore (and its live ACP session) per vault, kept for the life of
	// this panel instance (issue #262). Switching vaults used to dispose the
	// store and eagerly `prepareSession()` a brand-new ACP session/subprocess
	// every time — including flipping back to a vault already visited this
	// run. Reusing the cached store avoids that redundant spawn; the process
	// is only actually stopped (via `ChatStore.stop()`) on real teardown.
	const storesByVault = new Map<string, ChatStore>();

	// Mode dropdown (Ask = read-only, Agent = read-write) shown in the composer.
	// Custom-agent personas are folded into the same menu as write-capable agents.
	let modeOpen = $state(false);
	const modeLabel = $derived(
		store?.activePersona ? store.activePersona.name : store?.readOnly ? 'Ask' : 'Agent'
	);
	const modeIsWrite = $derived(store ? !store.readOnly : false);

	async function chooseAsk(): Promise<void> {
		modeOpen = false;
		store?.selectPersona(null);
		await store?.setReadOnly(true);
	}

	async function chooseAgent(): Promise<void> {
		modeOpen = false;
		store?.selectPersona(null);
		await store?.setReadOnly(false);
	}

	async function choosePersona(id: string): Promise<void> {
		modeOpen = false;
		// The store applies the persona's own read/write capability; individual
		// writes still prompt per call for read-write personas.
		store?.selectPersona(id);
	}

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

		const cached = storesByVault.get(vault);
		if (cached) {
			// Already live (or being lazily started) for this vault — reuse it
			// rather than spawning a second ACP session/subprocess.
			store = cached;
			activeSession.set(cached);
			return;
		}

		const next = new ChatStore(vault, createAgentClient(), {
			breakGlass: () => breakGlassStore.enabled,
			applyToEditor: (mode, text) => activeEditorStore.applyOutput(mode, text)
		});
		next.start();
		storesByVault.set(vault, next);
		store = next;
		activeSession.set(next);
		await next.loadAgents();
		await next.loadThreads();
		await next.loadCustomizations();
		// Establish the session up front so the model picker is available before
		// the first message (ACP exposes models only after session/new). This
		// still happens exactly once per vault for the life of the panel.
		void next.prepareSession();
	}

	onDestroy(() => {
		for (const cached of storesByVault.values()) {
			void cached.endSession();
			cached.dispose();
		}
		storesByVault.clear();
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

	// Onboarding: example prompts for an empty conversation, tailored to the
	// active note when one is open.
	const activeNoteTitle = $derived.by(() => {
		const path = tabStore.selectedPath;
		if (!path) return null;
		const note = vaultStore.notes.find((n) => n.path === path);
		if (note?.title) return note.title;
		const file = path.split('/').pop() ?? path;
		return file.replace(/\.md$/, '');
	});
	const promptSuggestions = $derived(suggestedPrompts(activeNoteTitle));

	async function useSuggestion(prompt: string) {
		if (!store) return;
		store.input = prompt;
		await tick();
		inputEl?.focus();
		const end = inputEl?.value.length ?? 0;
		inputEl?.setSelectionRange(end, end);
	}
</script>


<div class="agent-shell" class:collapsed>
	<div class="agent-panel">
		{#if !store}
			<div class="empty">Loading agent…</div>
		{:else if !store.available}
			<div class="onboard">
				<div class="onboard-card">
					<div class="onboard-icon" aria-hidden="true">
						<svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
							<path d="M12 8V4H8" />
							<rect width="16" height="12" x="4" y="8" rx="2" />
							<path d="M2 14h2M20 14h2M15 13v2M9 13v2" />
						</svg>
					</div>
					<h2 class="onboard-title">Chat runs in the desktop app</h2>
					<p class="onboard-lead">
						The AI agent talks to a local agent CLI (Copilot, Claude, Codex, or
						Gemini), so it's only available in the Notesmith desktop app — not the
						browser.
					</p>
					<div class="onboard-section">
						<span class="onboard-label">Once you're in the desktop app you can</span>
						<ul class="onboard-list">
							<li><strong>Ask</strong> read-only questions about your vault, or switch to <strong>Agent</strong> to create and edit notes.</li>
							<li>Attach context with <kbd>@</kbd> and run saved prompts with <kbd>/</kbd>.</li>
							<li>The active note is included automatically as context.</li>
						</ul>
					</div>
					<div class="onboard-section">
						<span class="onboard-label">Try prompts like</span>
						<div class="onboard-examples">
							{#each suggestedPrompts(activeNoteTitle) as suggestion (suggestion.label)}
								<span class="onboard-example">{suggestion.label}</span>
							{/each}
						</div>
					</div>
					<button type="button" class="onboard-cta" onclick={openSettings}>
						Configure AI agent settings
					</button>
				</div>
			</div>
		{:else}
			<header class="bar">
				<div class="controls pickers">
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
						<div class="pick-wrap">
							<select
								class="picker"
								aria-label="Provider"
								title="AI provider — the agent CLI powering this chat"
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
							<span class="pick-caret" aria-hidden="true">
								<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg>
							</span>
						</div>
					{/if}

					{#if store.modelPicker}
						<div class="pick-wrap">
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
							<span class="pick-caret" aria-hidden="true">
								<svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6" /></svg>
							</span>
						</div>
					{/if}
				</div>

				<div class="controls actions">
					<button
						class="head-act new"
						type="button"
						aria-label="New conversation"
						title="New conversation"
						onclick={() => { store?.newThread(); showThreads = false; }}
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							<path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
							<path d="M9 10h6" />
							<path d="M12 7v6" />
						</svg>
					</button>

					<button
						class="head-act"
						type="button"
						aria-label="Fork this conversation"
						title="Fork this conversation"
						disabled={!store.currentThreadId}
						onclick={() => { if (store?.currentThreadId) void forkThread(store.currentThreadId); }}
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							<circle cx="12" cy="18" r="3" />
							<circle cx="6" cy="6" r="3" />
							<circle cx="18" cy="6" r="3" />
							<path d="M18 9v2c0 .6-.4 1-1 1H7c-.6 0-1-.4-1-1V9" />
							<path d="M12 12v3" />
						</svg>
					</button>

					<button
						class="head-act"
						type="button"
						aria-label="Export this conversation to a note"
						title="Export this conversation to a note"
						disabled={!store.currentThreadId}
						onclick={() => { if (store?.currentThreadId) void exportThread(store.currentThreadId); }}
					>
						<svg
							xmlns="http://www.w3.org/2000/svg"
							width="16"
							height="16"
							viewBox="0 0 24 24"
							fill="none"
							stroke="currentColor"
							stroke-width="2"
							stroke-linecap="round"
							stroke-linejoin="round"
							aria-hidden="true"
						>
							<path d="M21 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2h6" />
							<path d="m21 3-9 9" />
							<path d="M15 3h6v6" />
						</svg>
					</button>

					<span class="head-div" aria-hidden="true"></span>

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
					Operating on <strong>{currentVault}</strong>
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
					<div class="chat-onboard">
						<p class="chat-onboard-lead">
							{#if activeNoteTitle}
								Ask about <strong>{activeNoteTitle}</strong> or your whole vault.
							{:else}
								Ask a question about your vault to get started.
							{/if}
						</p>
						<div class="chat-suggestions">
							{#each promptSuggestions as suggestion (suggestion.label)}
								<button
									type="button"
									class="chat-suggestion"
									onclick={() => void useSuggestion(suggestion.prompt)}
								>
									{suggestion.label}
								</button>
							{/each}
						</div>
						<p class="chat-onboard-hint">
							<kbd>@</kbd> add context · <kbd>/</kbd> saved prompts ·
							<strong>{modeIsWrite ? 'Agent' : 'Ask'}</strong> mode below
						</p>
					</div>
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
				<div class="composer-tools">
					<div class="mode-wrap">
						<button
							type="button"
							class="mode-pill"
							class:write={modeIsWrite}
							aria-haspopup="menu"
							aria-expanded={modeOpen}
							title={store.readOnly
								? 'Ask — read-only, the agent can search and answer but never writes'
								: 'Agent — read-write, the agent can modify the vault (prompts per write)'}
							onclick={() => (modeOpen = !modeOpen)}
						>
							<span class="mode-dot" class:write={modeIsWrite}></span>
							{modeLabel}
							<span class="mode-caret" aria-hidden="true">⌄</span>
						</button>

						{#if modeOpen}
							<button
								class="mode-backdrop"
								type="button"
								aria-label="Close mode menu"
								onclick={() => (modeOpen = false)}
							></button>
							<div class="mode-menu" role="menu">
								<button
									type="button"
									class="mode-item"
									class:sel={store.readOnly && !store.activePersona}
									role="menuitemradio"
									aria-checked={store.readOnly && !store.activePersona}
									onclick={() => void chooseAsk()}
								>
									<span class="mi-ic ask" aria-hidden="true">
										<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
											<path d="M7.9 20A9 9 0 1 0 4 16.1L2 22Z" />
										</svg>
									</span>
									<span class="mi-text">
										<span class="mi-title">Ask <span class="mi-tag">· read-only</span></span>
										<span class="mi-desc">Search &amp; answer questions. Never modifies the vault.</span>
									</span>
								</button>

								<button
									type="button"
									class="mode-item"
									class:sel={!store.readOnly && !store.activePersona}
									role="menuitemradio"
									aria-checked={!store.readOnly && !store.activePersona}
									onclick={() => void chooseAgent()}
								>
									<span class="mi-ic write" aria-hidden="true">
										<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
											<path d="M12 8V4H8" />
											<rect width="16" height="12" x="4" y="8" rx="2" />
											<path d="M2 14h2" />
											<path d="M20 14h2" />
											<path d="M15 13v2" />
											<path d="M9 13v2" />
										</svg>
									</span>
									<span class="mi-text">
										<span class="mi-title">Agent <span class="mi-tag">· read-write</span></span>
										<span class="mi-desc">Can create &amp; edit notes. Prompts for each write.</span>
									</span>
								</button>

								{#if store.personas.length > 0}
									<div class="mode-div"></div>
									<div class="mode-hdr">Custom agents</div>
									{#each store.personas as p (p.id)}
										<button
											type="button"
											class="mode-item"
											class:sel={store.activePersonaId === p.id}
											role="menuitemradio"
											aria-checked={store.activePersonaId === p.id}
											title={p.description}
											onclick={() => void choosePersona(p.id)}
										>
											<span class="mi-ic" class:ask={p.readOnly} class:write={!p.readOnly} aria-hidden="true">
												<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
													<path d="m12 3-1.9 5.8a2 2 0 0 1-1.3 1.3L3 12l5.8 1.9a2 2 0 0 1 1.3 1.3L12 21l1.9-5.8a2 2 0 0 1 1.3-1.3L21 12l-5.8-1.9a2 2 0 0 1-1.3-1.3Z" />
												</svg>
											</span>
											<span class="mi-text">
												<span class="mi-title">{p.name} <span class="mi-tag">· {p.readOnly ? 'read-only' : 'read-write'}</span></span>
												{#if p.description}<span class="mi-desc">{p.description}</span>{/if}
											</span>
										</button>
									{/each}
								{/if}
							</div>
						{/if}
					</div>
				</div>
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
		gap: 6px;
		padding: 10px 12px;
		border-bottom: 1px solid var(--border-default);
	}

	.controls {
		display: flex;
		flex-wrap: nowrap;
		align-items: center;
		gap: 5px;
	}

	.pick-wrap {
		position: relative;
		display: inline-flex;
		align-items: center;
		min-width: 0;
		flex: 0 1 auto;
	}

	.picker {
		appearance: none;
		-webkit-appearance: none;
		width: 100%;
		min-width: 0;
		padding: 4px 22px 4px 10px;
		border: 1px solid var(--border-strong);
		border-radius: 8px;
		background: var(--button-bg);
		color: var(--text-default);
		font-size: 12px;
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		cursor: pointer;
	}

	.picker:hover {
		background: var(--button-hover);
	}

	.picker:focus {
		outline: none;
		border-color: var(--accent-bg);
	}

	.pick-caret {
		position: absolute;
		right: 7px;
		display: inline-flex;
		color: var(--text-muted);
		pointer-events: none;
	}

	.head-div {
		width: 1px;
		height: 18px;
		background: var(--border-strong);
		margin: 0 4px 0 auto;
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

	.threads-toggle {
		display: inline-grid;
		place-items: center;
		padding: 4px 7px;
		min-width: 30px;
		border: 1px solid var(--border-strong);
		border-radius: 7px;
		background: var(--button-bg);
		color: var(--button-text);
		font-size: 14px;
		cursor: pointer;
	}

	.threads-toggle:hover {
		background: var(--button-hover);
	}

	.head-act {
		display: inline-grid;
		place-items: center;
		padding: 4px 7px;
		border: 1px solid var(--border-strong);
		border-radius: 7px;
		background: var(--button-bg);
		color: var(--button-text);
		cursor: pointer;
	}

	.head-act.new {
		color: var(--accent);
	}

	.head-act:hover:not(:disabled) {
		background: var(--button-hover);
		color: var(--text-default);
	}

	.head-act:disabled {
		opacity: 0.4;
		cursor: default;
	}

	.badge {
		font-size: 11px;
		color: var(--text-muted);
	}

	.badge strong {
		color: var(--text-default);
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

	.onboard {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 20px 16px;
		overflow-y: auto;
	}

	.onboard-card {
		display: flex;
		flex-direction: column;
		gap: 12px;
		max-width: 340px;
		padding: 20px;
		border: 1px solid var(--border-default);
		border-radius: 12px;
		background: var(--bg-default);
	}

	.onboard-icon {
		display: inline-grid;
		place-items: center;
		width: 40px;
		height: 40px;
		border-radius: 10px;
		background: var(--accent-bg);
		color: var(--accent);
	}

	.onboard-title {
		margin: 0;
		font-size: 15px;
		font-weight: 600;
		color: var(--text-default);
	}

	.onboard-lead {
		margin: 0;
		font-size: 12.5px;
		line-height: 1.5;
		color: var(--text-secondary);
	}

	.onboard-section {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.onboard-label {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-muted);
	}

	.onboard-list {
		margin: 0;
		padding-left: 18px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 12.5px;
		line-height: 1.5;
		color: var(--text-secondary);
	}

	.onboard-list strong {
		color: var(--text-default);
	}

	.onboard-examples {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
	}

	.onboard-example {
		padding: 3px 8px;
		border: 1px solid var(--border-default);
		border-radius: 999px;
		background: var(--bg-secondary);
		font-size: 11.5px;
		color: var(--text-secondary);
	}

	.onboard-cta {
		align-self: flex-start;
		margin-top: 2px;
		padding: 7px 12px;
		border: 1px solid var(--border-strong);
		border-radius: 8px;
		background: var(--button-bg);
		color: var(--button-text);
		font-size: 12.5px;
		font-weight: 600;
		cursor: pointer;
	}

	.onboard-cta:hover {
		background: var(--button-hover);
		color: var(--text-default);
	}

	.chat-onboard {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 20px 16px;
	}

	.chat-onboard-lead {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		color: var(--text-secondary);
	}

	.chat-onboard-lead strong {
		color: var(--text-default);
	}

	.chat-suggestions {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.chat-suggestion {
		text-align: left;
		padding: 8px 10px;
		border: 1px solid var(--border-default);
		border-radius: 8px;
		background: var(--bg-default);
		color: var(--text-default);
		font-size: 12.5px;
		cursor: pointer;
		transition: border-color 0.12s ease, background 0.12s ease;
	}

	.chat-suggestion:hover {
		border-color: var(--accent);
		background: var(--bg-hover);
	}

	.chat-onboard-hint {
		margin: 0;
		font-size: 11.5px;
		line-height: 1.6;
		color: var(--text-muted);
	}

	.chat-onboard-hint strong {
		color: var(--text-secondary);
	}

	.chat-onboard-hint kbd,
	.onboard-list kbd {
		display: inline-block;
		min-width: 16px;
		padding: 0 4px;
		border: 1px solid var(--border-default);
		border-radius: 4px;
		background: var(--bg-secondary);
		font-family: var(--font-mono);
		font-size: 10.5px;
		text-align: center;
		color: var(--text-secondary);
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

	.composer-tools {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.mode-wrap {
		position: relative;
	}

	.mode-pill {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 4px 9px;
		border: 1px solid var(--border-strong);
		border-radius: 8px;
		background: var(--button-bg);
		color: var(--text-default);
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
	}

	.mode-pill:hover {
		background: var(--button-hover);
	}

	.mode-pill.write {
		border-color: var(--warning-border);
		color: var(--warning-text);
	}

	.mode-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		background: var(--text-muted);
	}

	.mode-dot.write {
		background: var(--warning-text);
	}

	.mode-caret {
		color: var(--text-muted);
		font-size: 11px;
	}

	.mode-backdrop {
		position: fixed;
		inset: 0;
		z-index: 20;
		border: none;
		background: transparent;
		cursor: default;
	}

	.mode-menu {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 0;
		z-index: 21;
		width: 264px;
		max-height: 320px;
		overflow-y: auto;
		background: var(--bg-input);
		border: 1px solid var(--border-strong);
		border-radius: 10px;
		box-shadow: 0 16px 40px rgba(0, 0, 0, 0.5);
		padding: 4px;
	}

	.mode-item {
		display: flex;
		gap: 10px;
		width: 100%;
		padding: 8px 9px;
		border: none;
		border-radius: 7px;
		background: transparent;
		color: var(--text-default);
		text-align: left;
		cursor: pointer;
	}

	.mode-item:hover {
		background: var(--bg-hover);
	}

	.mode-item.sel {
		background: var(--bg-hover);
	}

	.mi-ic {
		margin-top: 1px;
		color: var(--text-muted);
		flex: 0 0 auto;
	}

	.mode-item.sel .mi-ic.ask {
		color: var(--accent-bg);
	}

	.mi-ic.write {
		color: var(--warning-text);
	}

	.mi-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.mi-title {
		font-size: 13px;
		font-weight: 600;
	}

	.mi-tag {
		font-size: 11px;
		font-weight: 400;
		color: var(--text-muted);
	}

	.mi-desc {
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.35;
	}

	.mode-div {
		height: 1px;
		background: var(--border-default);
		margin: 4px 2px;
	}

	.mode-hdr {
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
		padding: 6px 9px 3px;
	}
</style>
