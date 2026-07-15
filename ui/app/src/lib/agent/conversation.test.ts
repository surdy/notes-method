import { describe, expect, it } from 'vitest';

import {
	emptyConversation,
	fromMessages,
	reduce,
	type ConversationState,
	type MessageItem,
	type ToolItem
} from './conversation.ts';
import type { AgentEvent } from './types.ts';

function play(events: AgentEvent[]): ConversationState {
	return events.reduce(reduce, emptyConversation());
}

describe('conversation reducer', () => {
	it('accumulates streaming agent deltas into one bubble', () => {
		const state = play([
			{ type: 'user_message', text: 'hi' },
			{ type: 'agent_message_delta', text: 'Hel' },
			{ type: 'agent_message_delta', text: 'lo' },
			{ type: 'agent_message_delta', text: ' there' }
		]);
		expect(state.items).toHaveLength(2);
		const user = state.items[0] as MessageItem;
		const agent = state.items[1] as MessageItem;
		expect(user.role).toBe('user');
		expect(agent.role).toBe('agent');
		expect(agent.text).toBe('Hello there');
		expect(agent.streaming).toBe(true);
		expect(state.busy).toBe(true);
	});

	it('marks the agent bubble finished and clears busy on done', () => {
		const state = play([
			{ type: 'user_message', text: 'hi' },
			{ type: 'agent_message_delta', text: 'done soon' },
			{ type: 'done', result: null }
		]);
		const agent = state.items[1] as MessageItem;
		expect(agent.streaming).toBe(false);
		expect(state.busy).toBe(false);
	});

	it('synthesizes an agent bubble from done.result when nothing streamed', () => {
		const state = play([
			{ type: 'user_message', text: 'hi' },
			{ type: 'done', result: 'final answer' }
		]);
		const agent = state.items[1] as MessageItem;
		expect(agent.role).toBe('agent');
		expect(agent.text).toBe('final answer');
		expect(agent.streaming).toBe(false);
	});

	it('pairs a tool result with its call by id', () => {
		const state = play([
			{ type: 'tool_call', id: 'c1', name: 'Read', args: { path: 'n.md' } },
			{ type: 'tool_call', id: 'c2', name: 'Bash', args: { cmd: 'ls' } },
			{ type: 'tool_result', id: 'c1', content: 'file body', is_error: false }
		]);
		const tools = state.items.filter((i) => i.kind === 'tool') as ToolItem[];
		expect(tools).toHaveLength(2);
		expect(tools[0].name).toBe('Read');
		expect(tools[0].result?.content).toBe('file body');
		expect(tools[1].result).toBeNull();
	});

	it('attaches an id-less result to the most recent pending tool', () => {
		const state = play([
			{ type: 'tool_call', id: null, name: 'Write', args: {} },
			{ type: 'tool_result', id: null, content: 'ok', is_error: false }
		]);
		const tool = state.items.find((i) => i.kind === 'tool') as ToolItem;
		expect(tool.result).toEqual({ content: 'ok', isError: false });
		expect(tool.result?.isError).toBe(false);
	});

	it('surfaces an orphan tool result instead of dropping it', () => {
		const state = play([{ type: 'tool_result', id: 'x', content: 'stray', is_error: true }]);
		const tool = state.items.find((i) => i.kind === 'tool') as ToolItem;
		expect(tool).toBeTruthy();
		expect(tool.result?.isError).toBe(true);
	});

	it('keeps streaming after a recoverable error', () => {
		const state = play([
			{ type: 'agent_message_delta', text: 'partial' },
			{ type: 'error', message: 'hiccup' },
			{ type: 'agent_message_delta', text: ' continues' }
		]);
		// The error is its own item; the delta after it starts a NEW bubble
		// (the error item broke the streaming run).
		expect(state.items.some((i) => i.kind === 'error')).toBe(true);
		const messages = state.items.filter((i) => i.kind === 'message') as MessageItem[];
		expect(messages[messages.length - 1].text).toBe(' continues');
	});

	it('records status updates as their own items', () => {
		const state = play([{ type: 'status', message: 'session initialized' }]);
		expect(state.items[0]).toMatchObject({ kind: 'status', message: 'session initialized' });
	});

	it('assigns stable unique ids to every item', () => {
		const state = play([
			{ type: 'user_message', text: 'a' },
			{ type: 'agent_message_delta', text: 'b' },
			{ type: 'tool_call', id: 'c', name: 't', args: {} },
			{ type: 'status', message: 's' }
		]);
		const ids = state.items.map((i) => i.id);
		expect(new Set(ids).size).toBe(ids.length);
	});

	it('seeds a finished conversation from persisted messages', () => {
		const state = fromMessages([
			{ role: 'user', content: 'old q' },
			{ role: 'agent', content: 'old a' }
		]);
		expect(state.items).toHaveLength(2);
		expect((state.items[0] as MessageItem).text).toBe('old q');
		expect((state.items[1] as MessageItem).streaming).toBe(false);
		expect(state.busy).toBe(false);
	});
});

describe('per-message vault sources (#242)', () => {
	const searchHits = JSON.stringify([
		{
			path: 'people/Acme.md',
			title: 'Acme Corp',
			snippet: 'Acme is a customer',
			score: 0.03,
			lexical_rank: 2,
			semantic_rank: 1
		}
	]);
	const webHits = JSON.stringify([
		{ title: 'Docs', url: 'https://example.com', snippet: 'x' }
	]);

	function agentMessages(state: ConversationState): MessageItem[] {
		return state.items.filter(
			(i): i is MessageItem => i.kind === 'message' && i.role === 'agent'
		);
	}

	it('binds sources onto an agent message grounded in vault_search (notes-only)', () => {
		const state = play([
			{ type: 'user_message', text: 'who is acme?' },
			{ type: 'tool_call', id: 'c1', name: 'notesmith-people-vault_search', args: { query: 'acme' } },
			{ type: 'tool_result', id: 'c1', content: searchHits, is_error: false },
			{ type: 'agent_message_delta', text: 'Acme is a customer.' },
			{ type: 'done', result: null }
		]);
		const [agent] = agentMessages(state);
		expect(agent.sources).toHaveLength(1);
		expect(agent.sources?.[0]).toMatchObject({ path: 'people/Acme.md', score: 0.03 });
	});

	it('captures sources even when the tool runs after the text streams', () => {
		const state = play([
			{ type: 'user_message', text: 'who is acme?' },
			{ type: 'agent_message_delta', text: 'Let me check.' },
			{ type: 'tool_call', id: 'c1', name: 'vault_search', args: { query: 'acme' } },
			{ type: 'tool_result', id: 'c1', content: searchHits, is_error: false },
			{ type: 'done', result: null }
		]);
		const [agent] = agentMessages(state);
		expect(agent.sources?.[0]?.path).toBe('people/Acme.md');
	});

	it('does not attach web-search results to the note-sources control (web-only)', () => {
		const state = play([
			{ type: 'user_message', text: 'latest news?' },
			{ type: 'tool_call', id: 'c1', name: 'web_search', args: { q: 'news' } },
			{ type: 'tool_result', id: 'c1', content: webHits, is_error: false },
			{ type: 'agent_message_delta', text: 'See [1] https://example.com' },
			{ type: 'done', result: null }
		]);
		const [agent] = agentMessages(state);
		expect(agent.sources ?? []).toEqual([]);
	});

	it('captures only vault sources in a mixed web+notes answer', () => {
		const state = play([
			{ type: 'user_message', text: 'compare acme with the web' },
			{ type: 'tool_call', id: 'c1', name: 'web_search', args: { q: 'acme' } },
			{ type: 'tool_result', id: 'c1', content: webHits, is_error: false },
			{ type: 'tool_call', id: 'c2', name: 'vault_search', args: { query: 'acme' } },
			{ type: 'tool_result', id: 'c2', content: searchHits, is_error: false },
			{ type: 'agent_message_delta', text: 'Acme, per your notes and [1] the web.' },
			{ type: 'done', result: null }
		]);
		const [agent] = agentMessages(state);
		expect(agent.sources).toHaveLength(1);
		expect(agent.sources?.[0]?.path).toBe('people/Acme.md');
	});

	it('leaves an ungrounded answer without sources (none)', () => {
		const state = play([
			{ type: 'user_message', text: 'hello' },
			{ type: 'agent_message_delta', text: 'Hi there!' },
			{ type: 'done', result: null }
		]);
		const [agent] = agentMessages(state);
		expect(agent.sources).toBeUndefined();
	});

	it('resets pending sources between turns', () => {
		const state = play([
			{ type: 'user_message', text: 'who is acme?' },
			{ type: 'tool_call', id: 'c1', name: 'vault_search', args: {} },
			{ type: 'tool_result', id: 'c1', content: searchHits, is_error: false },
			{ type: 'agent_message_delta', text: 'Acme.' },
			{ type: 'done', result: null },
			{ type: 'user_message', text: 'hello' },
			{ type: 'agent_message_delta', text: 'Hi!' },
			{ type: 'done', result: null }
		]);
		const agents = agentMessages(state);
		expect(agents[0].sources?.[0]?.path).toBe('people/Acme.md');
		expect(agents[1].sources).toBeUndefined();
	});
});
