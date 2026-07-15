/**
 * Pure, framework-free reducer that folds the normalized {@link AgentEvent}
 * stream into a flat list of renderable conversation items. Keeping this pure
 * makes the streaming behaviour (delta accumulation, tool-call/result pairing,
 * turn completion) exhaustively unit-testable without a DOM, which is exactly
 * the streaming-render path the chat panel renders.
 */

import type { AgentEvent, Role } from './types.ts';
import { extractNoteSources, mergeSources, type NoteSource } from './sources.ts';

export interface MessageItem {
	kind: 'message';
	id: string;
	role: Role;
	text: string;
	/** True while assistant deltas are still streaming into this bubble. */
	streaming: boolean;
	/**
	 * Vault notes that grounded this assistant message (issue #242), captured
	 * from the turn's `vault_search`/`get_note` tool results. Absent/empty when
	 * the answer was not grounded in vault notes. Web grounding is deliberately
	 * excluded — it renders as inline citations in {@link text}, not here.
	 */
	sources?: NoteSource[];
}

export interface ToolItem {
	kind: 'tool';
	id: string;
	/** Provider tool-call id used to correlate the result, when present. */
	callId: string | null;
	name: string;
	args: unknown;
	result: { content: string; isError: boolean } | null;
}

export interface StatusItem {
	kind: 'status';
	id: string;
	message: string;
}

export interface ErrorItem {
	kind: 'error';
	id: string;
	message: string;
}

export type ConversationItem = MessageItem | ToolItem | StatusItem | ErrorItem;

export interface ConversationState {
	items: ConversationItem[];
	/** True between sending a prompt and the matching `done` event. */
	busy: boolean;
	seq: number;
	/**
	 * Vault note sources accumulated for the turn in progress (issue #242). Reset
	 * on each user message and bound onto that turn's agent message as it streams.
	 */
	pendingSources: NoteSource[];
}

export function emptyConversation(): ConversationState {
	return { items: [], busy: false, seq: 0, pendingSources: [] };
}

function nextId(state: ConversationState, prefix: string): string {
	state.seq += 1;
	return `${prefix}-${state.seq}`;
}

/**
 * Bind the turn's accumulated sources onto its agent message. Finds the last
 * agent message (streaming or just-finished) and sets a fresh `sources` array,
 * or does nothing when the turn has no agent bubble or no sources yet.
 */
function bindSources(items: ConversationItem[], sources: NoteSource[]): void {
	if (sources.length === 0) return;
	for (let i = items.length - 1; i >= 0; i -= 1) {
		const item = items[i];
		if (item.kind !== 'message') continue;
		if (item.role !== 'agent') return;
		items[i] = { ...item, sources: sources.slice() };
		return;
	}
}

/**
 * Fold a single event into the conversation, returning a **new** state object
 * (items array is rebuilt) so Svelte reactivity sees the change. Mutating the
 * passed state's `seq` counter is intentional and local.
 */
export function reduce(state: ConversationState, event: AgentEvent): ConversationState {
	const items = state.items.slice();
	const next: ConversationState = {
		items,
		busy: state.busy,
		seq: state.seq,
		pendingSources: state.pendingSources
	};

	switch (event.type) {
		case 'user_message': {
			items.push({
				kind: 'message',
				id: nextId(next, 'msg'),
				role: 'user',
				text: event.text,
				streaming: false
			});
			next.busy = true;
			next.pendingSources = [];
			break;
		}
		case 'agent_message_delta': {
			const last = items[items.length - 1];
			if (last && last.kind === 'message' && last.role === 'agent' && last.streaming) {
				items[items.length - 1] = { ...last, text: last.text + event.text };
			} else {
				items.push({
					kind: 'message',
					id: nextId(next, 'msg'),
					role: 'agent',
					text: event.text,
					streaming: true
				});
			}
			next.busy = true;
			bindSources(items, next.pendingSources);
			break;
		}
		case 'tool_call': {
			items.push({
				kind: 'tool',
				id: nextId(next, 'tool'),
				callId: event.id ?? null,
				name: event.name,
				args: event.args,
				result: null
			});
			break;
		}
		case 'tool_result': {
			// Attach to the matching call by id, else the most recent pending tool.
			let idx = -1;
			for (let i = items.length - 1; i >= 0; i -= 1) {
				const item = items[i];
				if (item.kind !== 'tool' || item.result !== null) continue;
				if (event.id == null || item.callId === event.id) {
					idx = i;
					break;
				}
			}
			if (idx >= 0) {
				const tool = items[idx] as ToolItem;
				items[idx] = {
					...tool,
					result: { content: event.content, isError: event.is_error }
				};
				const found = extractNoteSources(tool.name, event.content, event.is_error);
				if (found.length > 0) {
					next.pendingSources = mergeSources(next.pendingSources, found);
					bindSources(items, next.pendingSources);
				}
			} else {
				// Orphan result — surface it rather than dropping it.
				items.push({
					kind: 'tool',
					id: nextId(next, 'tool'),
					callId: event.id ?? null,
					name: 'result',
					args: null,
					result: { content: event.content, isError: event.is_error }
				});
			}
			break;
		}
		case 'status': {
			items.push({ kind: 'status', id: nextId(next, 'status'), message: event.message });
			break;
		}
		case 'done': {
			const last = items[items.length - 1];
			if (last && last.kind === 'message' && last.role === 'agent' && last.streaming) {
				items[items.length - 1] = { ...last, streaming: false };
			} else if (event.result) {
				items.push({
					kind: 'message',
					id: nextId(next, 'msg'),
					role: 'agent',
					text: event.result,
					streaming: false
				});
			}
			bindSources(items, next.pendingSources);
			next.busy = false;
			break;
		}
		case 'error': {
			items.push({ kind: 'error', id: nextId(next, 'error'), message: event.message });
			break;
		}
	}

	return next;
}

/** Seed a conversation from persisted transcript messages (reopen flow). */
export function fromMessages(
	messages: ReadonlyArray<{ role: Role; content: string }>
): ConversationState {
	const state = emptyConversation();
	for (const m of messages) {
		state.items.push({
			kind: 'message',
			id: nextId(state, 'msg'),
			role: m.role,
			text: m.content,
			streaming: false
		});
	}
	return state;
}
