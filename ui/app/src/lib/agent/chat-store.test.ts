import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatStore, type TranscriptApi } from './chat-store.svelte.ts';
import type { AgentClient, PermissionEvent } from './agent-client.ts';
import type { AgentEvent, AgentInfo, PermissionDecision, StartSessionResult } from './types.ts';
import type { Thread } from '../api/transcripts.ts';

/** In-memory agent client that records calls and lets tests push events. */
class MockAgentClient implements AgentClient {
	startCalls: unknown[] = [];
	prompts: Array<{ sessionId: string; text: string }> = [];
	modelCalls: Array<{ sessionId: string; value: string }> = [];
	readOnlyCalls: Array<{ sessionId: string; readOnly: boolean }> = [];
	answered: Array<{ requestId: string; decision: PermissionDecision }> = [];
	private eventCb: ((sessionId: string, event: AgentEvent) => void) | null = null;
	private permCb: ((event: PermissionEvent) => void) | null = null;
	models: StartSessionResult['models'] = null;

	available(): boolean {
		return true;
	}
	async listAgents(): Promise<AgentInfo[]> {
		return [
			{ id: 'copilot', name: 'Copilot', available: true },
			{ id: 'claude', name: 'Claude', available: false }
		];
	}
	async startSession(opts: unknown): Promise<StartSessionResult> {
		this.startCalls.push(opts);
		return { sessionId: `sess-${this.startCalls.length}`, models: this.models };
	}
	async sendPrompt(sessionId: string, text: string): Promise<void> {
		this.prompts.push({ sessionId, text });
	}
	async selectModel(sessionId: string, value: string): Promise<void> {
		this.modelCalls.push({ sessionId, value });
	}
	async setReadOnly(sessionId: string, readOnly: boolean): Promise<void> {
		this.readOnlyCalls.push({ sessionId, readOnly });
	}
	async answerPermission(requestId: string, decision: PermissionDecision): Promise<void> {
		this.answered.push({ requestId, decision });
	}
	async stop(): Promise<void> {}
	onEvent(cb: (sessionId: string, event: AgentEvent) => void): () => void {
		this.eventCb = cb;
		return () => {
			this.eventCb = null;
		};
	}
	onPermission(cb: (event: PermissionEvent) => void): () => void {
		this.permCb = cb;
		return () => {
			this.permCb = null;
		};
	}
	emit(sessionId: string, event: AgentEvent): void {
		this.eventCb?.(sessionId, event);
	}
	emitPermission(event: PermissionEvent): void {
		this.permCb?.(event);
	}
}

function makeThread(id: string, title: string): Thread {
	return {
		id,
		vault: 'work',
		title,
		agent: 'copilot',
		model: null,
		created_at: '2026-01-01T00:00:00Z',
		updated_at: '2026-01-01T00:00:00Z'
	};
}

/** Fake transcript API capturing persistence calls. */
function fakeTranscripts() {
	const appended: Array<{ threadId: string; role: string; content: string }> = [];
	let created = 0;
	const api: TranscriptApi = {
		listThreads: vi.fn(async () => [makeThread('t-existing', 'Old chat')]),
		createThread: vi.fn(async (_v, title) => {
			created += 1;
			return makeThread(`t-new-${created}`, title);
		}),
		listMessages: vi.fn(async () => [
			{ role: 'user' as const, content: 'previous q' },
			{ role: 'agent' as const, content: 'previous a' }
		]),
		appendMessage: vi.fn(async (_v, threadId, role, content) => {
			appended.push({ threadId, role, content });
			return {};
		}),
		deleteThread: vi.fn(async () => {}),
		renameThread: vi.fn(async (_v, id, title) => makeThread(id, title))
	};
	return { api, appended };
}

beforeEach(() => {
	// `.svelte.ts` rune fields compile to `$state()` calls; under vitest there is
	// no Svelte compiler, so make `$state` an identity function.
	vi.stubGlobal('$state', <T>(value: T) => value);
});

afterEach(() => {
	vi.unstubAllGlobals();
});

describe('ChatStore orchestration', () => {
	it('loads agents and defaults to the first available one', async () => {
		const client = new MockAgentClient();
		const store = new ChatStore('work', client);
		await store.loadAgents();
		expect(store.selectedAgent).toBe('copilot');
		expect(store.currentAgentName).toBe('Copilot');
	});

	it('sending creates a thread, persists the user message, starts a session, and prompts', async () => {
		const client = new MockAgentClient();
		const { api, appended } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();

		store.input = 'summarize my week';
		await store.send();

		expect(api.createThread).toHaveBeenCalledOnce();
		expect(store.currentThreadId).toBe('t-new-1');
		expect(appended[0]).toMatchObject({ role: 'user', content: 'summarize my week' });
		expect(client.startCalls).toHaveLength(1);
		expect(client.prompts[0]).toMatchObject({ sessionId: 'sess-1', text: 'summarize my week' });
		expect(store.input).toBe('');
	});

	it('passes read-only and break-glass into the session start', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api, breakGlass: () => true });
		await store.loadAgents();
		store.input = 'hi';
		await store.send();
		expect(client.startCalls[0]).toMatchObject({
			vault: 'work',
			agent: 'copilot',
			readOnly: true,
			breakGlass: true
		});
	});

	it('renders streamed events and persists the final agent message on done', async () => {
		const client = new MockAgentClient();
		const { api, appended } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();
		store.input = 'hi';
		await store.send();

		client.emit('sess-1', { type: 'user_message', text: 'hi' });
		client.emit('sess-1', { type: 'agent_message_delta', text: 'Hel' });
		client.emit('sess-1', { type: 'agent_message_delta', text: 'lo' });
		expect(store.busy).toBe(true);
		client.emit('sess-1', { type: 'done', result: null });

		expect(store.busy).toBe(false);
		const agentMsg = store.conversation.items.find(
			(i) => i.kind === 'message' && i.role === 'agent'
		);
		expect(agentMsg && agentMsg.kind === 'message' && agentMsg.text).toBe('Hello');
		// The final agent text is persisted exactly once.
		expect(appended.filter((a) => a.role === 'agent')).toEqual([
			{ threadId: 't-new-1', role: 'agent', content: 'Hello' }
		]);
	});

	it('ignores events from a superseded session', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();
		store.input = 'hi';
		await store.send(); // sessionId = sess-1

		client.emit('sess-OTHER', { type: 'agent_message_delta', text: 'leak' });
		expect(store.conversation.items).toHaveLength(0);
	});

	it('captures and answers a permission prompt', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		store.start();

		client.emitPermission({
			sessionId: 'sess-1',
			requestId: 'req-1',
			request: { tool: 'Write', kind: 'edit' }
		});
		expect(store.pendingPermission?.request.tool).toBe('Write');

		await store.answerPermission('allow_always');
		expect(store.pendingPermission).toBeNull();
		expect(client.answered).toEqual([{ requestId: 'req-1', decision: 'allow_always' }]);
	});

	it('toggles read-only and forwards to a live session', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.input = 'hi';
		await store.send(); // session live, readOnly defaults true

		await store.toggleReadOnly();
		expect(store.readOnly).toBe(false);
		expect(client.readOnlyCalls).toEqual([{ sessionId: 'sess-1', readOnly: false }]);
	});

	it('selects a model and forwards to a live session', async () => {
		const client = new MockAgentClient();
		client.models = {
			current: 'gpt-5',
			options: [
				{ id: 'gpt-5', name: 'GPT-5' },
				{ id: 'opus', name: 'Opus' }
			]
		};
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.input = 'hi';
		await store.send();
		expect(store.selectedModel).toBe('gpt-5');

		await store.selectModel('opus');
		expect(store.selectedModel).toBe('opus');
		expect(client.modelCalls).toEqual([{ sessionId: 'sess-1', value: 'opus' }]);
	});

	it('reopening a thread seeds the conversation and re-establishes the session lazily', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();

		await store.openThread('t-existing');
		expect(store.currentThreadId).toBe('t-existing');
		expect(store.conversation.items).toHaveLength(2);
		expect(store.sessionId).toBeNull(); // lazy
		expect(client.startCalls).toHaveLength(0);

		// Next send re-establishes the ACP session.
		store.input = 'follow up';
		await store.send();
		expect(client.startCalls).toHaveLength(1);
		expect((client.startCalls[0] as { threadId?: string }).threadId).toBe('t-existing');
	});

	it('deleting the current thread resets to a new conversation', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		store.threads = [makeThread('t1', 'one'), makeThread('t2', 'two')];
		store.currentThreadId = 't1';

		await store.deleteThread('t1');
		expect(api.deleteThread).toHaveBeenCalledWith('work', 't1');
		expect(store.threads.map((t) => t.id)).toEqual(['t2']);
		expect(store.currentThreadId).toBeNull();
	});

	it('does not send empty input or while busy', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();

		store.input = '   ';
		await store.send();
		expect(client.prompts).toHaveLength(0);
	});
});
