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
	): Promise<Array<{ role: 'user' | 'agent' | 'system'; content: string }>>;
	appendMessage(
		vault: string,
		threadId: string,
		role: 'user' | 'agent' | 'system',
		content: string
	): Promise<unknown>;
	deleteThread(vault: string, threadId: string): Promise<void>;
	renameThread(vault: string, threadId: string, title: string): Promise<Thread>;
}

export interface ChatStoreDeps {
	client: AgentClient;
	transcripts?: TranscriptApi;
	/** Reads the current break-glass setting at session start. */
	breakGlass?: () => boolean;
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

	private readonly transcripts: TranscriptApi;
	private unsubscribers: Array<() => void> = [];
	private sessionStartPromise: Promise<void> | null = null;

	constructor(
		private readonly vault: string,
		private readonly client: AgentClient,
		deps?: Partial<ChatStoreDeps>
	) {
		this.transcripts = deps?.transcripts ?? (transcriptApi as TranscriptApi);
		this.breakGlass = deps?.breakGlass ?? (() => false);
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

	private async ensureSession(): Promise<void> {
		if (this.sessionId || !this.selectedAgent) return;
		// Dedupe concurrent starts (eager prepareSession + a quick send) so the
		// agent process is spawned at most once per session.
		if (this.sessionStartPromise) return this.sessionStartPromise;
		this.sessionStartPromise = (async () => {
			const result = await this.client.startSession({
				vault: this.vault,
				agent: this.selectedAgent!,
				readOnly: this.readOnly,
				breakGlass: this.breakGlass(),
				threadId: this.currentThreadId
			});
			this.sessionId = result.sessionId;
			this.modelPicker = result.models;
			this.selectedModel = result.models?.current ?? this.selectedModel;
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

	/** Send the current input as a prompt. Persists the user message first. */
	async send(editor?: EditorContext): Promise<void> {
		const text = this.input.trim();
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
				await this.client.sendPrompt(this.sessionId, text, editor);
			}
		} catch (err) {
			this.errorMessage = err instanceof Error ? err.message : String(err);
		}
	}

	async selectModel(value: string): Promise<void> {
		this.selectedModel = value;
		if (this.sessionId) await this.client.selectModel(this.sessionId, value);
	}

	async toggleReadOnly(): Promise<void> {
		this.readOnly = !this.readOnly;
		if (this.sessionId) await this.client.setReadOnly(this.sessionId, this.readOnly);
	}

	async answerPermission(decision: PermissionDecision): Promise<void> {
		const pending = this.pendingPermission;
		if (!pending) return;
		this.pendingPermission = null;
		await this.client.answerPermission(pending.requestId, decision);
	}

	private handleEvent(sessionId: string, event: AgentEvent): void {
		// Ignore events from a superseded session.
		if (this.sessionId && sessionId !== this.sessionId) return;
		this.conversation = reduce(this.conversation, event);
		if (event.type === 'error') {
			this.errorMessage = event.message;
		}
		if (event.type === 'done' && this.currentThreadId) {
			const finalAgent = [...this.conversation.items]
				.reverse()
				.find((i): i is MessageItem => i.kind === 'message' && i.role === 'agent');
			if (finalAgent && finalAgent.text) {
				void this.transcripts
					.appendMessage(this.vault, this.currentThreadId, 'agent', finalAgent.text)
					.catch(() => {});
			}
		}
	}
}
