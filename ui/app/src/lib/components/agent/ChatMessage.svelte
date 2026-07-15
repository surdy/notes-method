<script lang="ts">
	import type { MessageItem } from '$lib/agent/conversation';
	import { renderMarkdown } from '$lib/agent/markdown';
	import MessageSources from './MessageSources.svelte';
	import { activeEditorStore } from '$lib/editor/active-editor.svelte';
	import type { ApplyMode } from '$lib/editor/apply-output';
	import { toastStore } from '$lib/toast-store.svelte';
	import { vaultStore } from '$lib/stores.svelte';
	import { tabStore } from '$lib/tab-store.svelte';

	let { item }: { item: MessageItem } = $props();

	let isAgent = $derived(item.role !== 'user');
	let html = $derived(isAgent && item.text ? renderMarkdown(item.text) : '');
	// Only finished assistant turns with text can be applied to the note.
	let canApply = $derived(isAgent && !item.streaming && item.text.trim().length > 0);

	/** Open the note a chat wikilink (`[[path]]`) points at, resolving the target
	 * against the loaded note list the same way the editor's wikilinks do. */
	function openNoteLink(rawTarget: string) {
		const target = rawTarget.trim();
		if (!target) return;
		const base = target.replace(/\.md$/i, '');
		const notes = vaultStore.notes;
		const match =
			notes.find((n) => n.path === target) ??
			notes.find((n) => n.path === `${base}.md`) ??
			notes.find((n) => n.path.endsWith(`/${target}`) || n.path.endsWith(`/${base}.md`)) ??
			notes.find((n) => n.title === target || n.title === base) ??
			notes.find((n) => n.path.includes(target));
		if (match) {
			tabStore.selectNote(match.path);
		} else {
			toastStore.add(`Couldn't find a note matching "${target}".`, 'warning');
		}
	}

	function onNoteLink(event: MouseEvent | KeyboardEvent) {
		const el = (event.target as HTMLElement | null)?.closest('a.agent-notelink') as HTMLElement | null;
		if (!el) return;
		if (event instanceof KeyboardEvent && event.key !== 'Enter' && event.key !== ' ') return;
		event.preventDefault();
		openNoteLink(el.dataset.noteTarget ?? '');
	}

	function apply(mode: ApplyMode) {
		const applied = activeEditorStore.applyOutput(mode, item.text);
		if (!applied) {
			toastStore.add('Open a note in the editor to apply agent output.', 'warning');
		}
	}

	let copied = $state(false);
	let copyTimer: ReturnType<typeof setTimeout> | undefined;

	async function copy() {
		try {
			await navigator.clipboard.writeText(item.text);
			copied = true;
			clearTimeout(copyTimer);
			copyTimer = setTimeout(() => (copied = false), 1500);
		} catch {
			toastStore.add('Could not copy to clipboard.', 'warning');
		}
	}
</script>

<div class="message" class:user={item.role === 'user'} class:agent={item.role !== 'user'}>
	<span class="role">{item.role === 'user' ? 'You' : 'Agent'}</span>
	<div class="bubble">
		{#if item.text}
			{#if isAgent}
				<!-- renderMarkdown HTML-escapes all input before injecting tags -->
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="text markdown" onclick={onNoteLink} onkeydown={onNoteLink}>{@html html}</div>
			{:else}
				<p class="text">{item.text}</p>
			{/if}
		{:else if item.streaming}
			<span class="thinking" aria-label="Agent is responding">…</span>
		{/if}
	</div>
	{#if isAgent && item.sources && item.sources.length > 0}
		<MessageSources sources={item.sources} />
	{/if}
	{#if canApply}
		<div class="actions">
			<button
				type="button"
				class="action"
				title="Insert / Replace at cursor — replaces the selected text, or inserts at the cursor when nothing is selected"
				aria-label="Insert or replace at cursor"
				onclick={() => apply('cursor')}
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
					<path d="M12 20h-1a2 2 0 0 1-2-2 2 2 0 0 1-2 2H6" />
					<path d="M13 8h7a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2h-7" />
					<path d="M5 16H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2h1" />
					<path d="M6 4h1a2 2 0 0 1 2 2 2 2 0 0 1 2-2h1" />
					<path d="M9 6v12" />
				</svg>
			</button>
			<button
				type="button"
				class="action"
				title="Apply to note — append to the end of the active note"
				aria-label="Apply to note"
				onclick={() => apply('append')}
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
					<path
						d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z"
					/>
					<path d="M14 2v5a1 1 0 0 0 1 1h5" />
					<path d="M12 18v-6" />
					<path d="m9 15 3 3 3-3" />
				</svg>
			</button>
			<button
				type="button"
				class="action"
				title="Copy — copy this message to the clipboard"
				aria-label={copied ? 'Copied' : 'Copy message'}
				onclick={copy}
			>
				{#if copied}
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
						<path d="M20 6 9 17l-5-5" />
					</svg>
				{:else}
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
						<rect width="14" height="14" x="8" y="8" rx="2" ry="2" />
						<path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" />
					</svg>
				{/if}
			</button>
		</div>
	{/if}
</div>

<style>
	.message {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 8px 12px;
	}

	.message.user {
		align-items: flex-end;
	}

	.role {
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.03em;
		color: var(--text-muted);
	}

	.bubble {
		max-width: 90%;
		padding: 8px 12px;
		border-radius: 10px;
		border: 1px solid var(--border-default);
		background: var(--bg-surface);
		color: var(--text-default);
	}

	.message.user .bubble {
		background: var(--accent-bg);
		color: var(--accent-text);
		border-color: var(--accent-bg);
	}

	.text {
		margin: 0;
		font-size: 13px;
		line-height: 1.5;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.markdown {
		white-space: normal;
	}

	.markdown :global(p) {
		margin: 0 0 8px;
	}

	.markdown :global(p:last-child) {
		margin-bottom: 0;
	}

	.markdown :global(ul),
	.markdown :global(ol) {
		margin: 4px 0;
		padding-left: 20px;
	}

	.markdown :global(li) {
		margin: 2px 0;
	}

	.markdown :global(h1),
	.markdown :global(h2),
	.markdown :global(h3),
	.markdown :global(h4),
	.markdown :global(h5),
	.markdown :global(h6) {
		margin: 8px 0 4px;
		font-size: 13px;
		font-weight: 700;
	}

	.markdown :global(a) {
		color: var(--accent-bg);
		text-decoration: underline;
	}

	.markdown :global(a.agent-notelink) {
		cursor: pointer;
	}

	.markdown :global(code) {
		font-family: var(--font-mono);
		font-size: 0.92em;
		padding: 1px 4px;
		border-radius: 4px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-default);
	}

	.markdown :global(pre) {
		margin: 6px 0;
		padding: 8px 10px;
		border-radius: 8px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-default);
		overflow-x: auto;
	}

	.markdown :global(pre code) {
		padding: 0;
		border: none;
		background: none;
	}

	.thinking {
		color: var(--text-muted);
		font-size: 16px;
		letter-spacing: 2px;
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 2px;
	}

	.action {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 4px;
		border-radius: 6px;
		border: 1px solid var(--border-default);
		background: var(--button-bg);
		color: var(--button-text);
		cursor: pointer;
	}

	.action svg {
		display: block;
	}

	.action:hover,
	.action:focus-visible {
		background: var(--button-hover);
		border-color: var(--accent);
		color: var(--text-default);
		outline: none;
	}
</style>
