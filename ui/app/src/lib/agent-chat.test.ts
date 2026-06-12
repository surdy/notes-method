import { describe, expect, it } from 'vitest';
import {
	applyAgentEvent,
	emptyChatState,
	endSession,
	startedChatState,
	type ChatState
} from './agent-chat';

function reduce(events: Parameters<typeof applyAgentEvent>[1][]): ChatState {
	return events.reduce(applyAgentEvent, startedChatState());
}

describe('agent-chat reducer', () => {
	it('starts empty and not running', () => {
		const state = emptyChatState();
		expect(state.messages).toEqual([]);
		expect(state.running).toBe(false);
	});

	it('appends a user message', () => {
		const state = reduce([{ type: 'user_message', text: 'hello' }]);
		expect(state.messages).toEqual([{ kind: 'text', role: 'user', text: 'hello' }]);
	});

	it('accumulates consecutive agent deltas into one bubble', () => {
		const state = reduce([
			{ type: 'agent_message_delta', text: 'Hel' },
			{ type: 'agent_message_delta', text: 'lo ' },
			{ type: 'agent_message_delta', text: 'there' }
		]);
		expect(state.messages).toEqual([{ kind: 'text', role: 'agent', text: 'Hello there' }]);
	});

	it('starts a new agent bubble after a user message', () => {
		const state = reduce([
			{ type: 'agent_message_delta', text: 'first' },
			{ type: 'user_message', text: 'next?' },
			{ type: 'agent_message_delta', text: 'second' }
		]);
		expect(state.messages).toEqual([
			{ kind: 'text', role: 'agent', text: 'first' },
			{ kind: 'text', role: 'user', text: 'next?' },
			{ kind: 'text', role: 'agent', text: 'second' }
		]);
	});

	it('ignores empty agent deltas', () => {
		const state = reduce([{ type: 'agent_message_delta', text: '' }]);
		expect(state.messages).toEqual([]);
	});

	it('correlates a tool result with its call by id', () => {
		const state = reduce([
			{ type: 'tool_call', id: 'toolu_1', name: 'Read', args: { path: 'a.md' } },
			{ type: 'tool_result', id: 'toolu_1', content: 'file body', is_error: false }
		]);
		expect(state.messages).toEqual([
			{
				kind: 'tool',
				id: 'toolu_1',
				name: 'Read',
				args: { path: 'a.md' },
				result: { content: 'file body', isError: false }
			}
		]);
	});

	it('marks a failed tool result as an error', () => {
		const state = reduce([
			{ type: 'tool_call', id: 'toolu_1', name: 'Bash', args: {} },
			{ type: 'tool_result', id: 'toolu_1', content: 'boom', is_error: true }
		]);
		const tool = state.messages[0];
		expect(tool.kind).toBe('tool');
		if (tool.kind === 'tool') {
			expect(tool.result).toEqual({ content: 'boom', isError: true });
		}
	});

	it('correlates a null-id result with the most recent open tool call', () => {
		const state = reduce([
			{ type: 'tool_call', id: 'toolu_1', name: 'Read', args: {} },
			{ type: 'tool_result', id: null, content: 'data', is_error: false }
		]);
		const tool = state.messages[0];
		expect(tool.kind === 'tool' && tool.result?.content).toBe('data');
	});

	it('surfaces an orphan tool result as its own card', () => {
		const state = reduce([{ type: 'tool_result', id: 'x', content: 'lonely', is_error: false }]);
		expect(state.messages).toEqual([
			{
				kind: 'tool',
				id: 'x',
				name: '(result)',
				args: null,
				result: { content: 'lonely', isError: false }
			}
		]);
	});

	it('renders a status message', () => {
		const state = reduce([{ type: 'status', message: 'session initialized' }]);
		expect(state.messages).toEqual([
			{ kind: 'status', text: 'session initialized', isError: false }
		]);
	});

	it('renders an error but keeps the session running (ADR 0009)', () => {
		const state = reduce([{ type: 'error', message: 'bad line' }]);
		expect(state.messages).toEqual([{ kind: 'status', text: 'bad line', isError: true }]);
		expect(state.running).toBe(true);
	});

	it('stops running on done', () => {
		const state = reduce([
			{ type: 'agent_message_delta', text: 'answer' },
			{ type: 'done', result: 'answer' }
		]);
		expect(state.running).toBe(false);
		expect(state.messages).toEqual([{ kind: 'text', role: 'agent', text: 'answer' }]);
	});

	it('endSession stops a running session', () => {
		const state = endSession(startedChatState());
		expect(state.running).toBe(false);
	});

	it('endSession is a no-op when already stopped', () => {
		const stopped = emptyChatState();
		expect(endSession(stopped)).toBe(stopped);
	});

	it('ignores unknown event shapes without throwing', () => {
		const state = startedChatState();
		// @ts-expect-error — deliberately malformed event
		const next = applyAgentEvent(state, { type: 'nonsense', foo: 1 });
		expect(next).toBe(state);
	});

	it('does not mutate the previous state', () => {
		const initial = startedChatState();
		const next = applyAgentEvent(initial, { type: 'user_message', text: 'hi' });
		expect(initial.messages).toEqual([]);
		expect(next.messages).toHaveLength(1);
	});
});
