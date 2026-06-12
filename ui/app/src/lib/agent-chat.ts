/**
 * Pure reduction of the normalized agent event stream (ADR 0011) into a
 * renderable chat transcript.
 *
 * The Tauri runner (`crates/notesmith-tauri/src/agent.rs`) emits one
 * {@link AgentEvent} per IPC message, tagged with a session id. This module
 * keeps the reduction pure and framework-agnostic so it can be unit-tested
 * without Tauri or Svelte; the chat component holds the state and feeds events
 * through {@link applyAgentEvent}.
 *
 * Resilience (ADR 0009): unknown or malformed events are ignored rather than
 * throwing, so a single bad line never breaks the panel.
 */

/** A normalized event mirroring `notesmith_agent::AgentEvent` (serde-tagged). */
export type AgentEvent =
	| { type: 'user_message'; text: string }
	| { type: 'agent_message_delta'; text: string }
	| { type: 'tool_call'; id: string | null; name: string; args: unknown }
	| { type: 'tool_result'; id: string | null; content: string; is_error: boolean }
	| { type: 'status'; message: string }
	| { type: 'done'; result: string | null }
	| { type: 'error'; message: string };

export type ChatRole = 'user' | 'agent';

export interface TextMessage {
	kind: 'text';
	role: ChatRole;
	text: string;
}

export interface ToolMessage {
	kind: 'tool';
	id: string | null;
	name: string;
	args: unknown;
	result: { content: string; isError: boolean } | null;
}

export interface StatusMessage {
	kind: 'status';
	text: string;
	isError: boolean;
}

export type ChatMessage = TextMessage | ToolMessage | StatusMessage;

export interface ChatState {
	messages: ChatMessage[];
	/** True while the session is alive and may still produce events. */
	running: boolean;
}

/** A fresh, empty chat transcript with no running session. */
export function emptyChatState(): ChatState {
	return { messages: [], running: false };
}

/** A transcript for a session that has just started (running, no messages). */
export function startedChatState(): ChatState {
	return { messages: [], running: true };
}

/**
 * Apply a single agent event, returning a new {@link ChatState}. Never mutates
 * `state`. Unknown event shapes are returned unchanged.
 */
export function applyAgentEvent(state: ChatState, event: AgentEvent): ChatState {
	switch (event?.type) {
		case 'user_message':
			return appendMessage(state, { kind: 'text', role: 'user', text: event.text ?? '' });

		case 'agent_message_delta':
			return appendAgentDelta(state, event.text ?? '');

		case 'tool_call':
			return appendMessage(state, {
				kind: 'tool',
				id: event.id ?? null,
				name: event.name ?? '',
				args: event.args ?? null,
				result: null
			});

		case 'tool_result':
			return attachToolResult(state, event.id ?? null, {
				content: event.content ?? '',
				isError: Boolean(event.is_error)
			});

		case 'status':
			return appendMessage(state, { kind: 'status', text: event.message ?? '', isError: false });

		case 'error':
			// Errors are recoverable (ADR 0009): surface them but keep the
			// session running; the stream ends only on `done`/session-ended.
			return appendMessage(state, { kind: 'status', text: event.message ?? '', isError: true });

		case 'done':
			return { ...state, running: false };

		default:
			return state;
	}
}

/** Mark the session as ended (process exited or was stopped). */
export function endSession(state: ChatState): ChatState {
	if (!state.running) {
		return state;
	}
	return { ...state, running: false };
}

function appendMessage(state: ChatState, message: ChatMessage): ChatState {
	return { ...state, messages: [...state.messages, message] };
}

function appendAgentDelta(state: ChatState, text: string): ChatState {
	if (text.length === 0) {
		return state;
	}
	const last = state.messages[state.messages.length - 1];
	if (last && last.kind === 'text' && last.role === 'agent') {
		const updated: TextMessage = { ...last, text: last.text + text };
		return { ...state, messages: [...state.messages.slice(0, -1), updated] };
	}
	return appendMessage(state, { kind: 'text', role: 'agent', text });
}

function attachToolResult(
	state: ChatState,
	id: string | null,
	result: { content: string; isError: boolean }
): ChatState {
	// Correlate with the most recent matching tool call that has no result yet.
	for (let i = state.messages.length - 1; i >= 0; i -= 1) {
		const message = state.messages[i];
		if (message.kind !== 'tool' || message.result !== null) {
			continue;
		}
		if (id === null || message.id === null || message.id === id) {
			const updated: ToolMessage = { ...message, result };
			const messages = state.messages.slice();
			messages[i] = updated;
			return { ...state, messages };
		}
	}
	// No matching call: surface the orphan result as its own card.
	return appendMessage(state, {
		kind: 'tool',
		id,
		name: '(result)',
		args: null,
		result
	});
}
