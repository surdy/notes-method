import { describe, expect, it, vi } from 'vitest';
import {
	isAgentRunnerAvailable,
	listenForSession,
	mcpEndpoint,
	sendAgentMessage,
	startAgentSession,
	stopAgentSession
} from './agent-bridge';
import type { TauriAdapter } from './window-lifecycle';

type Listener = (payload: unknown) => void;

function fakeAdapter(invoke = vi.fn()) {
	const listeners = new Map<string, Listener>();
	const adapter: TauriAdapter = {
		invoke,
		listen: vi.fn(async (event: string, handler: Listener) => {
			listeners.set(event, handler);
			return () => listeners.delete(event);
		})
	};
	return { adapter, listeners, invoke };
}

describe('agent-bridge', () => {
	it('reports runner unavailable when no adapter resolves', () => {
		expect(isAgentRunnerAvailable(null)).toBe(false);
	});

	it('reports runner available with an adapter', () => {
		const { adapter } = fakeAdapter();
		expect(isAgentRunnerAvailable(adapter)).toBe(true);
	});

	it('starts a session and returns the session id', async () => {
		const { adapter, invoke } = fakeAdapter(vi.fn().mockResolvedValue('agent-1'));
		const id = await startAgentSession({ vault: 'notes', agent: 'claude-code' }, adapter);
		expect(id).toBe('agent-1');
		expect(invoke).toHaveBeenCalledWith('agent_start', {
			vault: 'notes',
			agent: 'claude-code',
			bin: null,
			mcpUrl: null
		});
	});

	it('passes the resolved mcp url to the runner', async () => {
		const { adapter, invoke } = fakeAdapter(vi.fn().mockResolvedValue('agent-1'));
		await startAgentSession(
			{
				vault: 'notes',
				agent: 'claude-code',
				mcpUrl: 'http://127.0.0.1:27183/mcp-ro/notes'
			},
			adapter
		);
		expect(invoke).toHaveBeenCalledWith('agent_start', {
			vault: 'notes',
			agent: 'claude-code',
			bin: null,
			mcpUrl: 'http://127.0.0.1:27183/mcp-ro/notes'
		});
	});

	it('throws when starting without a desktop adapter', async () => {
		await expect(
			startAgentSession({ vault: 'notes', agent: 'claude-code' }, null)
		).rejects.toThrow(/desktop app/);
	});

	it('forwards the copilot-acp agent kind to the runner', async () => {
		const { adapter, invoke } = fakeAdapter(vi.fn().mockResolvedValue('agent-1'));
		await startAgentSession({ vault: 'notes', agent: 'copilot-acp' }, adapter);
		expect(invoke).toHaveBeenCalledWith('agent_start', {
			vault: 'notes',
			agent: 'copilot-acp',
			bin: null,
			mcpUrl: null
		});
	});

	it('sends a message with camelCase session id arg', async () => {
		const { adapter, invoke } = fakeAdapter(vi.fn().mockResolvedValue(undefined));
		await sendAgentMessage('agent-1', 'hello', adapter);
		expect(invoke).toHaveBeenCalledWith('agent_send', {
			sessionId: 'agent-1',
			message: 'hello'
		});
	});

	it('stop is a no-op without an adapter', async () => {
		await expect(stopAgentSession('agent-1', null)).resolves.toBeUndefined();
	});

	it('delivers only events for the subscribed session and strips session_id', async () => {
		const { adapter, listeners } = fakeAdapter();
		const onEvent = vi.fn();
		const onEnded = vi.fn();
		await listenForSession('agent-1', { onEvent, onEnded }, adapter);

		const emit = listeners.get('notesmith://agent-event')!;
		emit({ session_id: 'agent-2', type: 'status', message: 'other' });
		emit({ session_id: 'agent-1', type: 'status', message: 'mine' });

		expect(onEvent).toHaveBeenCalledTimes(1);
		expect(onEvent).toHaveBeenCalledWith({ type: 'status', message: 'mine' });
	});

	it('invokes onEnded only for the subscribed session', async () => {
		const { adapter, listeners } = fakeAdapter();
		const onEnded = vi.fn();
		await listenForSession('agent-1', { onEvent: vi.fn(), onEnded }, adapter);

		const ended = listeners.get('notesmith://agent-ended')!;
		ended({ session_id: 'agent-2' });
		ended({ session_id: 'agent-1' });

		expect(onEnded).toHaveBeenCalledTimes(1);
	});

	it('teardown removes both listeners', async () => {
		const { adapter, listeners } = fakeAdapter();
		const teardown = await listenForSession('agent-1', { onEvent: vi.fn(), onEnded: vi.fn() }, adapter);
		expect(listeners.size).toBe(2);
		teardown();
		expect(listeners.size).toBe(0);
	});

	describe('mcpEndpoint', () => {
		it('builds a read-only endpoint by default scope', () => {
			expect(mcpEndpoint('http://127.0.0.1:27183', 'notes', true)).toBe(
				'http://127.0.0.1:27183/mcp-ro/notes'
			);
		});

		it('builds a read-write endpoint when not read-only', () => {
			expect(mcpEndpoint('http://127.0.0.1:27183', 'notes', false)).toBe(
				'http://127.0.0.1:27183/mcp/notes'
			);
		});

		it('trims trailing slashes from the api base', () => {
			expect(mcpEndpoint('http://127.0.0.1:27183/', 'notes', true)).toBe(
				'http://127.0.0.1:27183/mcp-ro/notes'
			);
		});

		it('encodes vault names with special characters', () => {
			expect(mcpEndpoint('http://h', 'my notes', false)).toBe('http://h/mcp/my%20notes');
		});

		it('returns null when the api base is empty (hosted browser)', () => {
			expect(mcpEndpoint('', 'notes', true)).toBeNull();
		});

		it('returns null when the vault is empty', () => {
			expect(mcpEndpoint('http://h', '', true)).toBeNull();
		});
	});
});
