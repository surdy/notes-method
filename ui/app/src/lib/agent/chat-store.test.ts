import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { ChatStore, type TranscriptApi, type PermissionApi } from './chat-store.svelte.ts';
import type { AgentClient, PermissionEvent } from './agent-client.ts';
import type {
	AgentEvent,
	AgentInfo,
	AgentsConfigData,
	DiagEntry,
	DiagnosticsReport,
	PermissionDecision,
	StartSessionResult
} from './types.ts';
import type { Thread } from '../api/transcripts.ts';

/** In-memory agent client that records calls and lets tests push events. */
class MockAgentClient implements AgentClient {
	startCalls: unknown[] = [];
	prompts: Array<{ sessionId: string; text: string; editor?: unknown }> = [];
	modelCalls: Array<{ sessionId: string; value: string }> = [];
	readOnlyCalls: Array<{ sessionId: string; readOnly: boolean }> = [];
	answered: Array<{ requestId: string; decision: PermissionDecision }> = [];
	stopped: string[] = [];
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
	async sendPrompt(sessionId: string, text: string, editor?: unknown): Promise<void> {
		this.prompts.push({ sessionId, text, editor });
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
	async stop(sessionId: string): Promise<void> {
		this.stopped.push(sessionId);
	}
	async agentDiagnostics(): Promise<DiagnosticsReport> {
		return { resolvedPath: [], agents: [] };
	}
	async diagnosticsLog(): Promise<DiagEntry[]> {
		return [];
	}
	async setDiagnosticsVerbose(): Promise<void> {}
	async clearDiagnosticsLog(): Promise<void> {}
	async getAgentConfig(): Promise<AgentsConfigData> {
		return { debug: false, entries: [] };
	}
	async setAgentConfig(): Promise<void> {}
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

function fakePermissions(initial: string[] = []) {
	const granted: string[] = [];
	const revoked: string[] = [];
	const api: PermissionApi = {
		listGrants: vi.fn(async () => [...initial]),
		grant: vi.fn(async (_v: string, tool: string) => {
			granted.push(tool);
		}),
		revoke: vi.fn(async (_v: string, tool: string) => {
			revoked.push(tool);
		})
	};
	return { api, granted, revoked };
}

beforeEach(() => {
	// `.svelte.ts` rune fields compile to `$state()` calls; under vitest there is
	// no Svelte compiler, so make `$state` an identity function.
	vi.stubGlobal('$state', <T>(value: T) => value);
	// Tests that start a session but do not inject a fake `permissions` dep fall
	// through to the real permission client, which fetches persisted grants.
	// Stub fetch to return an empty grant list so those starts stay fast and
	// offline (the store seeds `[]` and never re-prompts).
	vi.stubGlobal(
		'fetch',
		vi.fn(async () => new Response(JSON.stringify([]), { status: 200 }))
	);
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

	it('stop cancels the in-flight turn and unlocks the composer', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();
		store.input = 'hi';
		await store.send();

		client.emit('sess-1', { type: 'agent_message_delta', text: 'partial' });
		expect(store.busy).toBe(true);

		await store.stop();

		expect(client.stopped).toEqual(['sess-1']);
		expect(store.busy).toBe(false);
	});

	it('regenerate re-runs the most recent user turn', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();
		store.input = 'summarize my week';
		await store.send();
		client.emit('sess-1', { type: 'done', result: null });

		expect(store.canRegenerate).toBe(true);
		await store.regenerate();

		const sent = client.prompts.map((p) => p.text);
		expect(sent).toEqual(['summarize my week', 'summarize my week']);
		const userMsgs = store.conversation.items.filter(
			(i) => i.kind === 'message' && i.role === 'user'
		);
		expect(userMsgs).toHaveLength(2);
	});

	it('forks a saved thread, copying prior messages into a new continuable thread', async () => {
		const client = new MockAgentClient();
		const { api, appended } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		await store.loadThreads();

		const newId = await store.forkThread('t-existing');

		expect(api.createThread).toHaveBeenCalledWith(
			'work',
			'Old chat (fork)',
			'copilot',
			null
		);
		expect(appended.filter((a) => a.threadId === newId)).toEqual([
			{ threadId: newId, role: 'user', content: 'previous q' },
			{ threadId: newId, role: 'agent', content: 'previous a' }
		]);
		expect(store.currentThreadId).toBe(newId);
	});

	it('exports a saved thread to a markdown note with metadata and roles', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const created: Array<{ title: string; content: string }> = [];
		const store = new ChatStore('work', client, {
			transcripts: api,
			createNote: async (_v, title, content) => {
				created.push({ title, content });
				return { path: 'inbox/Old chat.md' };
			}
		});
		await store.loadThreads();

		const path = await store.exportThread('t-existing');

		expect(path).toBe('inbox/Old chat.md');
		expect(created).toHaveLength(1);
		expect(created[0].title).toBe('Old chat');
		expect(created[0].content).toContain('type: chat-transcript');
		expect(created[0].content).toContain('**User:**');
		expect(created[0].content).toContain('previous q');
		expect(created[0].content).toContain('**Agent:**');
		expect(created[0].content).toContain('previous a');
	});

	it('renders the user message immediately on send, without a backend echo', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();

		store.input = 'summarize my week';
		await store.send();

		// ACP agents do not echo the prompt; the store renders it locally.
		const userMsgs = store.conversation.items.filter(
			(i) => i.kind === 'message' && i.role === 'user'
		);
		expect(userMsgs).toHaveLength(1);
		expect(userMsgs[0].kind === 'message' && userMsgs[0].text).toBe('summarize my week');
		expect(store.busy).toBe(true);
	});

	it('ignores events from a superseded session', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();
		store.input = 'hi';
		await store.send(); // sessionId = sess-1, one local user echo

		client.emit('sess-OTHER', { type: 'agent_message_delta', text: 'leak' });
		// Only the local user echo remains; the leaked agent delta is dropped.
		expect(store.conversation.items).toHaveLength(1);
		expect(
			store.conversation.items.some((i) => i.kind === 'message' && i.role === 'agent')
		).toBe(false);
	});

	it('eagerly prepares the session so the model picker is ready before sending', async () => {
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

		await store.prepareSession();
		expect(client.startCalls).toHaveLength(1);
		expect(store.modelPicker?.current).toBe('gpt-5');
		expect(store.selectedModel).toBe('gpt-5');

		// Sending reuses the established session rather than starting a new one.
		store.input = 'hi';
		await store.send();
		expect(client.startCalls).toHaveLength(1);
	});

	it('does not eagerly start a session for an unavailable agent', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.selectAgent('claude'); // unavailable in the mock list

		await store.prepareSession();
		expect(client.startCalls).toHaveLength(0);
		expect(store.modelPicker).toBeNull();
	});

	it('captures and answers a permission prompt', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const perms = fakePermissions();
		const store = new ChatStore('work', client, { transcripts: api, permissions: perms.api });
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

	it('allow_session answers without persisting a grant', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const perms = fakePermissions();
		const store = new ChatStore('work', client, { transcripts: api, permissions: perms.api });
		store.start();

		client.emitPermission({
			sessionId: 'sess-1',
			requestId: 'req-1',
			request: { tool: 'Write', kind: 'edit' }
		});
		await store.answerPermission('allow_session');

		expect(client.answered).toEqual([{ requestId: 'req-1', decision: 'allow_session' }]);
		// This-session grants are not persisted to the daemon store.
		expect(perms.api.grant).not.toHaveBeenCalled();
		expect(perms.granted).toEqual([]);
	});

	it('always allow answers AND persists the grant for the tool', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const perms = fakePermissions();
		const store = new ChatStore('work', client, { transcripts: api, permissions: perms.api });
		store.start();

		client.emitPermission({
			sessionId: 'sess-1',
			requestId: 'req-1',
			request: { tool: 'Write', kind: 'edit' }
		});
		await store.answerPermission('allow_always');

		expect(client.answered).toEqual([{ requestId: 'req-1', decision: 'allow_always' }]);
		expect(perms.api.grant).toHaveBeenCalledWith('work', 'Write');
		expect(perms.granted).toEqual(['Write']);
	});

	it('deny answers deny and persists nothing', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const perms = fakePermissions();
		const store = new ChatStore('work', client, { transcripts: api, permissions: perms.api });
		store.start();

		client.emitPermission({
			sessionId: 'sess-1',
			requestId: 'req-1',
			request: { tool: 'Write', kind: 'edit' }
		});
		await store.answerPermission('deny');

		expect(client.answered).toEqual([{ requestId: 'req-1', decision: 'deny' }]);
		expect(perms.api.grant).not.toHaveBeenCalled();
	});

	it('seeds persisted grants from listGrants into the new session', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const perms = fakePermissions(['create_note', 'append_note']);
		const store = new ChatStore('work', client, { transcripts: api, permissions: perms.api });
		await store.loadAgents();
		store.input = 'hi';
		await store.send();

		expect(perms.api.listGrants).toHaveBeenCalledWith('work');
		const opts = client.startCalls[0] as { persistedGrants?: string[] };
		expect(opts.persistedGrants).toEqual(['create_note', 'append_note']);
	});

	it('proceeds with no seed when fetching persisted grants fails', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const perms = fakePermissions();
		perms.api.listGrants = vi.fn(async () => {
			throw new Error('daemon offline');
		});
		const store = new ChatStore('work', client, { transcripts: api, permissions: perms.api });
		await store.loadAgents();
		store.input = 'hi';
		await store.send();

		// A grant-fetch failure must not block the session.
		expect(store.sessionId).toBe('sess-1');
		const opts = client.startCalls[0] as { persistedGrants?: string[] };
		expect(opts.persistedGrants).toEqual([]);
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

	it('reconciles read-write toggled while an eager session start is still in flight', async () => {
		// Repro: opening the panel eagerly starts a read-only session. If the user
		// flips to read-write before that (slow) start resolves, the toggle sees a
		// null session id and cannot rebuild — so the store must reconcile once the
		// session lands, or writes hit the read-only `/mcp-ro/` endpoint.
		const client = new MockAgentClient();
		let release!: () => void;
		const gate = new Promise<void>((resolve) => {
			release = resolve;
		});
		const startSession = client.startSession.bind(client);
		client.startSession = async (opts: unknown) => {
			await gate;
			return startSession(opts);
		};
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		await store.loadAgents();
		store.start();

		// Eager start begins (readOnly defaults true) but is gated mid-flight.
		const prepare = store.prepareSession();
		await store.toggleReadOnly();
		expect(store.readOnly).toBe(false);
		// No backend rebuild yet — the session id is still null.
		expect(client.readOnlyCalls).toEqual([]);

		release();
		await prepare;

		expect(store.sessionId).toBe('sess-1');
		expect(client.startCalls[0]).toMatchObject({ readOnly: true });
		// The store reconciled the scope to match the visible toggle.
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

describe('ChatStore inline commands', () => {
	it('runs an inline command and applies the final agent text on done', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const applyToEditor = vi.fn(() => true);
		const store = new ChatStore('work', client, { transcripts: api, applyToEditor });
		await store.loadAgents();
		store.start();

		await store.runInlineCommand({
			instruction: 'Rewrite the selected text.',
			selection: 'foo bar',
			applyMode: 'replace',
			activeNote: 'Notes/a.md'
		});

		// The instruction becomes the user turn; the selection rides via EditorContext.
		expect(client.prompts[0]).toMatchObject({
			sessionId: 'sess-1',
			text: 'Rewrite the selected text.',
			editor: { activeNote: 'Notes/a.md', selection: 'foo bar' }
		});

		client.emit('sess-1', { type: 'agent_message_delta', text: 'Foo ' });
		client.emit('sess-1', { type: 'agent_message_delta', text: 'Bar' });
		expect(applyToEditor).not.toHaveBeenCalled();
		client.emit('sess-1', { type: 'done', result: null });

		expect(applyToEditor).toHaveBeenCalledTimes(1);
		expect(applyToEditor).toHaveBeenCalledWith('replace', 'Foo Bar');
	});

	it('defaults the apply mode to replace', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const applyToEditor = vi.fn(() => true);
		const store = new ChatStore('work', client, { transcripts: api, applyToEditor });
		await store.loadAgents();
		store.start();

		await store.runInlineCommand({ instruction: 'Fix it.', selection: 'teh' });
		client.emit('sess-1', { type: 'agent_message_delta', text: 'the' });
		client.emit('sess-1', { type: 'done', result: null });

		expect(applyToEditor).toHaveBeenCalledWith('replace', 'the');
	});

	it('does not apply to the editor for an ordinary send', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const applyToEditor = vi.fn(() => true);
		const store = new ChatStore('work', client, { transcripts: api, applyToEditor });
		await store.loadAgents();
		store.start();

		store.input = 'just chatting';
		await store.send();
		client.emit('sess-1', { type: 'agent_message_delta', text: 'hi' });
		client.emit('sess-1', { type: 'done', result: null });

		expect(applyToEditor).not.toHaveBeenCalled();
	});

	it('ignores an inline command while busy or with no agent', async () => {
		const client = new MockAgentClient();
		const { api } = fakeTranscripts();
		const store = new ChatStore('work', client, { transcripts: api });
		store.start();
		// No agent selected yet.
		await store.runInlineCommand({ instruction: 'Rewrite.', selection: 'x' });
		expect(client.prompts).toHaveLength(0);
	});
});
