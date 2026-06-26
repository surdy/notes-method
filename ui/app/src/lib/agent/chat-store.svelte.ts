/**
 * Orchestration store for the agent chat panel. Coordinates three things:
 *
 * 1. the {@link AgentClient} transport (Tauri IPC in the app, a fake in tests),
 * 2. the per-vault transcript store over HTTP (persistence + reopen), and
 * 3. the pure {@link reduce} conversation reducer (streaming render).
 *
 * Dependencies are injected so the orchestration is unit-testable without a
 * DOM, Tauri, or a running daemon. Reactive fields use `$state`; derived values
 * are plain getters (the codebase tests `.svelte.ts` stores by stubbing
 * `$state` as identity, so we avoid `$derived`).
 */

import * as transcriptApi from '../api/transcripts.ts';
import type { Thread } from '../api/transcripts.ts';
import * as permissionApi from '../api/permissions.ts';
import * as customizationApi from '../api/customizations.ts';
import {
	emptyCustomizations,
	type Customizations,
	type CustomAgent
} from '../api/customizations.ts';
import { assembleSessionPreamble, parseAgentMention } from './persona.ts';
import { createNote as createVaultNote } from '../api/notes.ts';
import type { ApplyMode } from '../editor/apply-output.ts';
import { formatTranscriptMarkdown } from './transcript-format.ts';
import type { AgentClient, PermissionEvent } from './agent-client.ts';
import {
	emptyConversation,
	fromMessages,
	reduce,
	type ConversationState,
	type MessageItem
} from './conversation.ts';
import type {
	AgentEvent,
	AgentInfo,
	EditorContext,
	ModelPicker,
	PermissionDecision
} from './types.ts';

/** The subset of the transcript API the store needs; injectable for tests. */
export interface TranscriptApi {
	listThreads(vault: string): Promise<Thread[]>;
	createThread(
		vault: string,
		title: string,
		agent?: string | null,
		model?: string | null
	): Promise<Thread>;
	listMessages(
		vault: string,
		threadId: string
	): Promise<Array<{ role: 'user' | 'agent' | 'system'; content: string; created_at?: string }>>;
	appendMessage(
		vault: string,
		threadId: string,
		role: 'user' | 'agent' | 'system',
		content: string
	): Promise<unknown>;
	deleteThread(vault: string, threadId: string): Promise<void>;
	renameThread(vault: string, threadId: string, title: string): Promise<Thread>;
}

/** The subset of the permission API the store needs; injectable for tests. */
export interface PermissionApi {
	listGrants(vault: string): Promise<string[]>;
	grant(vault: string, tool: string): Promise<void>;
	revoke(vault: string, tool: string): Promise<void>;
}

/** The subset of the customization API the store needs; injectable for tests. */
export interface CustomizationsApi {
	listCustomizations(vault: string): Promise<Customizations>;
}

export interface ChatStoreDeps {
	client: AgentClient;
	transcripts?: TranscriptApi;
	permissions?: PermissionApi;
	customizations?: CustomizationsApi;
	/** Reads the current break-glass setting at session start. */
	breakGlass?: () => boolean;
	/**
	 * Applies an inline command's result to the active editor (issue #195).
	 * Injected so the store stays DOM-free and testable; defaults to a no-op that
	 * reports "no editor".
	 */
	applyToEditor?: (mode: ApplyMode, text: string) => boolean;
	/**
	 * Creates a vault note (issue #190 export). Injected so the store stays
	 * API-agnostic and testable; defaults to the real notes client.
	 */
	createNote?: (
		vault: string,
		title: string,
		content: string,
		folder?: string
	) => Promise<{ path: string }>;
}

export class ChatStore {
	agents = $state<AgentInfo[]>([]);
	selectedAgent = $state<string | null>(null);
	sessionId = $state<string | null>(null);
	modelPicker = $state<ModelPicker | null>(null);
	selectedModel = $state<string | null>(null);
	/** Safe by default: writes are hard-denied until the user enables read-write. */
	readOnly = $state(true);
	conversation = $state<ConversationState>(emptyConversation());
	threads = $state<Thread[]>([]);
	currentThreadId = $state<string | null>(null);
	pendingPermission = $state<PermissionEvent | null>(null);
	input = $state('');
	errorMessage = $state<string | null>(null);

	/** Discovered custom agents (personas), skills, and instructions (#210). */
	customizations = $state<Customizations>(emptyCustomizations());
	/** The active persona id, applied as a preamble + backend/model (#212). */
	activePersonaId = $state<string | null>(null);

	private readonly transcripts: TranscriptApi;
	private readonly permissions: PermissionApi;
	private readonly customizationsApi: CustomizationsApi;
	private readonly applyToEditor: (mode: ApplyMode, text: string) => boolean;
	private readonly createNote: (
		vault: string,
		title: string,
		content: string,
		folder?: string
	) => Promise<{ path: string }>;
	private pendingInlineApply: ApplyMode | null = null;
	private unsubscribers: Array<() => void> = [];
	private sessionStartPromise: Promise<void> | null = null;

	constructor(
		private readonly vault: string,
		private readonly client: AgentClient,
		deps?: Partial<ChatStoreDeps>
	) {
		this.transcripts = deps?.transcripts ?? (transcriptApi as TranscriptApi);
		this.permissions = deps?.permissions ?? (permissionApi as PermissionApi);
		this.customizationsApi = deps?.customizations ?? (customizationApi as CustomizationsApi);
		this.breakGlass = deps?.breakGlass ?? (() => false);
		this.applyToEditor = deps?.applyToEditor ?? (() => false);
		this.createNote = deps?.createNote ?? createVaultNote;
	}

	private breakGlass: () => boolean;

	get available(): boolean {
		return this.client.available();
	}

	get busy(): boolean {
		return this.conversation.busy;
	}

	get currentAgentName(): string | null {
		return this.agents.find((a) => a.id === this.selectedAgent)?.name ?? this.selectedAgent;
	}

	/** Subscribe to the transport's event + permission streams. */
	start(): void {
		this.unsubscribers.push(this.client.onEvent((sid, ev) => this.handleEvent(sid, ev)));
		this.unsubscribers.push(
			this.client.onPermission((p) => {
				this.pendingPermission = p;
			})
		);
	}

	dispose(): void {
		for (const u of this.unsubscribers) u();
		this.unsubscribers = [];
	}

	async loadAgents(): Promise<void> {
		this.agents = await this.client.listAgents();
		if (!this.selectedAgent) {
			this.selectedAgent = this.agents.find((a) => a.available)?.id ?? this.agents[0]?.id ?? null;
		}
	}

	async loadThreads(): Promise<void> {
		this.threads = await this.transcripts.listThreads(this.vault);
	}

	/**
	 * Load discovered personas/skills/instructions for this vault (#210).
	 * Resilient: a fetch failure degrades to an empty set so the panel still
	 * works without any customizations.
	 */
	async loadCustomizations(): Promise<void> {
		try {
			this.customizations = await this.customizationsApi.listCustomizations(this.vault);
		} catch {
			this.customizations = emptyCustomizations();
		}
	}

	/** Discovered personas (custom agents). */
	get personas(): CustomAgent[] {
		return this.customizations.agents;
	}

	/** The currently active persona, or `null` when none is selected. */
	get activePersona(): CustomAgent | null {
		return this.customizations.agents.find((p) => p.id === this.activePersonaId) ?? null;
	}

	/**
	 * The one-time session preamble: always-on discovered instructions plus the
	 * active persona's body (ADR 0016). `null` when there is nothing to inject.
	 */
	get sessionPreamble(): string | null {
		return assembleSessionPreamble(this.customizations.instructions, this.activePersona);
	}

	/**
	 * Switch the session's active persona (#212, session-switch routing). Applies
	 * the persona's backend agent (when discovered), model, and read-only access,
	 * and resets the session so the new preamble takes effect on the next turn.
	 * Passing `null` clears the persona (and leaves the read-only mode untouched).
	 * A persona referencing an unavailable backend keeps the current agent (the
	 * preamble still applies).
	 */
	selectPersona(id: string | null): void {
		if (id === this.activePersonaId) return;
		this.activePersonaId = id;
		const persona = this.activePersona;
		if (persona?.backend) {
			const backend = this.agents.find((a) => a.id === persona.backend && a.available);
			if (backend) this.selectedAgent = backend.id;
		}
		if (persona?.model) this.selectedModel = persona.model;
		// A persona carries its own read/write capability. Apply it synchronously
		// here so the fresh session below is prepared with the right scope (the
		// session reset below means there is no live session to forward to).
		if (persona) this.readOnly = persona.readOnly;
		// Persona/instructions change ⇒ fresh ACP session so the new preamble and
		// any backend/model switch take effect on the next turn.
		this.sessionId = null;
		this.sessionStartPromise = null;
		this.modelPicker = null;
		void this.prepareSession();
	}

	selectAgent(id: string): void {
		if (id === this.selectedAgent) return;
		this.selectedAgent = id;
		// A different agent needs a fresh ACP session.
		this.sessionId = null;
		this.sessionStartPromise = null;
		this.modelPicker = null;
		this.selectedModel = null;
		// Re-establish eagerly so the model picker reflects the new agent.
		void this.prepareSession();
	}

	/** Begin a brand-new conversation (no persisted thread yet). */
	newThread(): void {
		this.currentThreadId = null;
		this.conversation = emptyConversation();
		this.sessionId = null;
		this.sessionStartPromise = null;
		this.errorMessage = null;
	}

	/** Reopen a persisted thread; the ACP session is re-established lazily on send. */
	async openThread(threadId: string): Promise<void> {
		this.currentThreadId = threadId;
		this.sessionId = null;
		this.sessionStartPromise = null;
		this.errorMessage = null;
		const messages = await this.transcripts.listMessages(this.vault, threadId);
		this.conversation = fromMessages(messages);
	}

	async deleteThread(threadId: string): Promise<void> {
		await this.transcripts.deleteThread(this.vault, threadId);
		this.threads = this.threads.filter((t) => t.id !== threadId);
		if (this.currentThreadId === threadId) this.newThread();
	}

	/**
	 * Fork a saved thread into a new, independently continuable thread (issue
	 * #190). Copies every prior message in order; the new thread inherits the
	 * source agent/model so the conversation can resume with the same context.
	 */
	async forkThread(threadId: string): Promise<string> {
		const source = this.threads.find((t) => t.id === threadId);
		const messages = await this.transcripts.listMessages(this.vault, threadId);
		const title = `${source?.title ?? 'Conversation'} (fork)`;
		const created = await this.transcripts.createThread(
			this.vault,
			title,
			source?.agent ?? this.selectedAgent,
			source?.model ?? this.selectedModel
		);
		for (const m of messages) {
			await this.transcripts.appendMessage(this.vault, created.id, m.role, m.content);
		}
		await this.loadThreads();
		await this.openThread(created.id);
		return created.id;
	}

	/**
	 * Export a saved thread to a markdown note in the vault (issue #190). The note
	 * carries agent/model/timestamp metadata and role-labelled messages.
	 */
	async exportThread(threadId: string): Promise<string> {
		const source = this.threads.find((t) => t.id === threadId);
		const messages = await this.transcripts.listMessages(this.vault, threadId);
		const title = source?.title?.trim() || 'Chat transcript';
		const markdown = formatTranscriptMarkdown(
			{
				title,
				agent: source?.agent ?? null,
				model: source?.model ?? null,
				created_at: source?.created_at ?? null,
				updated_at: source?.updated_at ?? null
			},
			messages
		);
		const result = await this.createNote(this.vault, title, markdown);
		return result.path;
	}

	private async ensureSession(): Promise<void> {
		if (this.sessionId || !this.selectedAgent) return;
		// Dedupe concurrent starts (eager prepareSession + a quick send) so the
		// agent process is spawned at most once per session.
		if (this.sessionStartPromise) return this.sessionStartPromise;
		this.sessionStartPromise = (async () => {
			const startedReadOnly = this.readOnly;
			// Pre-seed the session with the user's persisted "Always Allow" grants
			// for this vault (issue #189) so granted tools never re-prompt — even
			// after a daemon/app restart. Best-effort: a fetch failure must not
			// block the session, so fall back to no seed.
			let persistedGrants: string[] = [];
			try {
				persistedGrants = await this.permissions.listGrants(this.vault);
			} catch {
				persistedGrants = [];
			}
			const result = await this.client.startSession({
				vault: this.vault,
				agent: this.selectedAgent!,
				readOnly: startedReadOnly,
				breakGlass: this.breakGlass(),
				threadId: this.currentThreadId,
				persistedGrants,
				preamble: this.sessionPreamble
			});
			this.sessionId = result.sessionId;
			this.modelPicker = result.models;
			this.selectedModel = result.models?.current ?? this.selectedModel;
			// The session binds its MCP scope (read-only `/mcp-ro/` vs read-write
			// `/mcp/`) at start. If the user flipped the toggle while this slow
			// start was in flight, `toggleReadOnly` no-opped on the still-null
			// session id — so reconcile now, or writes would hit the read-only
			// endpoint despite the UI showing read-write.
			if (this.readOnly !== startedReadOnly) {
				await this.client.setReadOnly(this.sessionId, this.readOnly);
			}
		})();
		try {
			await this.sessionStartPromise;
		} finally {
			this.sessionStartPromise = null;
		}
	}

	/**
	 * Eagerly establish the session for the selected agent so the model picker is
	 * available before the first message. Best-effort: only attempts it for an
	 * agent that detection marked available, and swallows failures so a missing
	 * or slow agent never raises a scary error on panel open — `send()` still
	 * surfaces real errors when the user actually prompts.
	 */
	async prepareSession(): Promise<void> {
		if (this.sessionId) return;
		const agent = this.agents.find((a) => a.id === this.selectedAgent);
		if (!agent?.available) return;
		try {
			await this.ensureSession();
		} catch {
			// Leave the model picker hidden; send() will retry and surface errors.
		}
	}

	/**
	 * Send the current input as a prompt. Persists the user message first.
	 *
	 * `contextPreamble` (issue #197) is an optional block of attached references
	 * (note/folder/tag/url) that the agent resolves via its MCP read/list tools.
	 * It is prepended to the text sent to the agent ONLY — the rendered bubble and
	 * the persisted transcript keep the user's original message, so context
	 * plumbing never clutters the conversation history.
	 */
	async send(editor?: EditorContext, contextPreamble?: string): Promise<void> {
		// #212 (session-switch routing): a leading `@persona-id` matching a
		// discovered persona switches the active persona for the rest of the
		// session and is stripped from the message. A bare switch (no text after
		// the mention) just switches and stops; an unknown id is left untouched
		// and sent as ordinary text.
		const mention = parseAgentMention(this.input);
		let text = this.input.trim();
		if (mention && this.personas.some((p) => p.id === mention.id)) {
			this.selectPersona(mention.id);
			text = mention.rest;
			if (!text) {
				this.input = '';
				return;
			}
		}
		if (!text || this.busy || !this.selectedAgent) return;
		this.input = '';
		this.errorMessage = null;

		// Render the user's message immediately. ACP agents do not echo the
		// prompt back as a session/update, so the UI owns this turn's user bubble.
		this.conversation = reduce(this.conversation, { type: 'user_message', text });

		try {
			if (!this.currentThreadId) {
				const title = text.length > 60 ? `${text.slice(0, 57)}…` : text;
				const thread = await this.transcripts.createThread(
					this.vault,
					title,
					this.selectedAgent,
					this.selectedModel
				);
				this.currentThreadId = thread.id;
				this.threads = [thread, ...this.threads];
			}
			await this.transcripts.appendMessage(this.vault, this.currentThreadId, 'user', text);
			await this.ensureSession();
			if (this.sessionId) {
				const preamble = contextPreamble?.trim();
				const outgoing = preamble ? `${preamble}\n\n${text}` : text;
				await this.client.sendPrompt(this.sessionId, outgoing, editor);
			}
		} catch (err) {
			this.errorMessage = err instanceof Error ? err.message : String(err);
		}
	}

	/**
	 * Run an inline editor command (issue #195) as a normal chat turn: the short
	 * instruction becomes the user message, the editor selection rides via
	 * {@link EditorContext.selection}, and the agent's final reply is applied back
	 * to the active editor when `done` fires. Reuses {@link send} so the turn is
	 * persisted and rendered like any other.
	 */
	async runInlineCommand(opts: {
		instruction: string;
		selection: string;
		applyMode?: ApplyMode;
		activeNote?: string | null;
	}): Promise<void> {
		if (this.busy || !this.selectedAgent) return;
		this.pendingInlineApply = opts.applyMode ?? 'replace';
		this.input = opts.instruction;
		await this.send({ activeNote: opts.activeNote ?? null, selection: opts.selection });
	}

	/** The text of the most recent user turn, or null if none yet. */
	private lastUserMessageText(): string | null {
		const last = [...this.conversation.items]
			.reverse()
			.find((i): i is MessageItem => i.kind === 'message' && i.role === 'user');
		return last && last.text ? last.text : null;
	}

	/**
	 * Cancel the in-flight generation (issue #191). Asks the transport to stop the
	 * current turn, then unlocks the composer even if the agent never emits a
	 * terminal event by feeding a synthetic `done` through the reducer.
	 */
	async stop(): Promise<void> {
		if (!this.sessionId || !this.busy) return;
		try {
			await this.client.stop(this.sessionId);
		} catch (err) {
			this.errorMessage = err instanceof Error ? err.message : String(err);
		} finally {
			this.conversation = reduce(this.conversation, { type: 'done', result: null });
		}
	}

	/**
	 * Re-run the most recent user turn (issue #191). Resends the last user message
	 * verbatim; the agent produces a fresh response. No-op while busy or when the
	 * conversation has no user turn yet.
	 */
	async regenerate(): Promise<void> {
		if (this.busy) return;
		const text = this.lastUserMessageText();
		if (!text) return;
		this.input = text;
		await this.send();
	}

	/** Whether {@link regenerate} has a user turn to re-run. */
	get canRegenerate(): boolean {
		return !this.busy && this.lastUserMessageText() !== null;
	}

	async selectModel(value: string): Promise<void> {
		this.selectedModel = value;
		if (this.sessionId) await this.client.selectModel(this.sessionId, value);
	}

	async toggleReadOnly(): Promise<void> {
		await this.setReadOnly(!this.readOnly);
	}

	/**
	 * Set the read-only capability explicitly (Ask = read-only, Agent =
	 * read-write). Forwards to a live session; when the session start is still
	 * in flight the flag is reconciled once it lands (see `prepareSession`).
	 */
	async setReadOnly(value: boolean): Promise<void> {
		if (this.readOnly === value) return;
		this.readOnly = value;
		if (this.sessionId) await this.client.setReadOnly(this.sessionId, value);
	}

	async answerPermission(decision: PermissionDecision): Promise<void> {
		const pending = this.pendingPermission;
		if (!pending) return;
		this.pendingPermission = null;
		// "Always Allow" persists the grant to the daemon store so a future
		// session (even after a restart) is pre-seeded and never re-prompts
		// (issue #189). Best-effort: a persistence failure must never block the
		// agent, so swallow it — the in-session grant still suppresses re-prompts
		// for the rest of this session. "allow_session" suppresses for this
		// session only; "allow_once"/"deny" persist nothing.
		if (decision === 'allow_always') {
			try {
				await this.permissions.grant(this.vault, pending.request.tool);
			} catch {
				// Swallow: persistence is best-effort, the ACP answer still applies.
			}
		}
		await this.client.answerPermission(pending.requestId, decision);
	}

	private lastAgentMessage(): MessageItem | undefined {
		return [...this.conversation.items]
			.reverse()
			.find((i): i is MessageItem => i.kind === 'message' && i.role === 'agent');
	}

	private handleEvent(sessionId: string, event: AgentEvent): void {
		// Ignore events from a superseded session.
		if (this.sessionId && sessionId !== this.sessionId) return;
		this.conversation = reduce(this.conversation, event);
		if (event.type === 'error') {
			this.errorMessage = event.message;
		}
		if (event.type === 'done' && this.currentThreadId) {
			const finalAgent = this.lastAgentMessage();
			if (finalAgent && finalAgent.text) {
				void this.transcripts
					.appendMessage(this.vault, this.currentThreadId, 'agent', finalAgent.text)
					.catch(() => {});
			}
		}
		// Apply an inline command's result to the editor. NOT gated on
		// `currentThreadId` — an inline command may run on a fresh conversation.
		if (event.type === 'done' && this.pendingInlineApply) {
			const mode = this.pendingInlineApply;
			this.pendingInlineApply = null;
			const finalAgent = this.lastAgentMessage();
			if (finalAgent && finalAgent.text) this.applyToEditor(mode, finalAgent.text);
		}
	}
}
