/**
 * Thin bridge from the Svelte chat panel to the desktop (Tauri) agent runner
 * (ADR 0011 Phase B). All calls funnel through the `__TAURI__` IPC adapter
 * resolved by {@link resolveTauri}; in a browser (hosted UI) the runner is
 * unavailable and {@link isAgentRunnerAvailable} returns false, so the panel
 * stays hidden — embedded agent chat is desktop-only.
 */

import { resolveTauri, type TauriAdapter } from './window-lifecycle';
import type { AgentEvent } from './agent-chat';

const AGENT_EVENT = 'notesmith://agent-event';
const AGENT_ENDED = 'notesmith://agent-ended';

export type AgentKind = 'claude-code';

export interface StartSessionOptions {
	vault: string;
	agent: AgentKind;
	bin?: string;
	/**
	 * Streamable HTTP MCP endpoint to auto-wire the agent to the active vault
	 * (ADR 0011 Phase C). Omit (or pass null) to launch without MCP.
	 */
	mcpUrl?: string | null;
}

/**
 * Resolve the per-vault MCP endpoint URL for a spawned agent.
 *
 * The daemon serves Streamable HTTP MCP at `/mcp/<vault>` (read-write) and
 * `/mcp-ro/<vault>` (read-only); the scope is encoded in the path. Returns
 * `null` when `apiBase` is empty (hosted browser, where the absolute daemon
 * origin is unknown) or `vault` is empty, in which case the agent runs without
 * MCP wiring.
 */
export function mcpEndpoint(
	apiBase: string,
	vault: string,
	readOnly: boolean
): string | null {
	if (!apiBase || !vault) {
		return null;
	}
	const base = apiBase.replace(/\/+$/, '');
	const scope = readOnly ? 'mcp-ro' : 'mcp';
	return `${base}/${scope}/${encodeURIComponent(vault)}`;
}

export interface AgentEventHandlers {
	/** Called for each normalized event belonging to `sessionId`. */
	onEvent: (event: AgentEvent) => void;
	/** Called once when the session's process ends. */
	onEnded: () => void;
}

/** True when running inside the Tauri desktop shell with the agent runner. */
export function isAgentRunnerAvailable(adapter: TauriAdapter | null = resolveTauri()): boolean {
	return adapter !== null;
}

/** Start an agent session in the desktop runner; resolves to its session id. */
export async function startAgentSession(
	options: StartSessionOptions,
	adapter: TauriAdapter | null = resolveTauri()
): Promise<string> {
	if (!adapter) {
		throw new Error('Agent chat is only available in the desktop app.');
	}
	const sessionId = await adapter.invoke('agent_start', {
		vault: options.vault,
		agent: options.agent,
		bin: options.bin ?? null,
		mcpUrl: options.mcpUrl ?? null
	});
	if (typeof sessionId !== 'string') {
		throw new Error('Agent runner did not return a session id.');
	}
	return sessionId;
}

/** Send a user message to a running session. */
export async function sendAgentMessage(
	sessionId: string,
	message: string,
	adapter: TauriAdapter | null = resolveTauri()
): Promise<void> {
	if (!adapter) {
		throw new Error('Agent chat is only available in the desktop app.');
	}
	await adapter.invoke('agent_send', { sessionId, message });
}

/** Stop a running session, terminating its child process. */
export async function stopAgentSession(
	sessionId: string,
	adapter: TauriAdapter | null = resolveTauri()
): Promise<void> {
	if (!adapter) {
		return;
	}
	await adapter.invoke('agent_stop', { sessionId });
}

/**
 * Subscribe to events for `sessionId`. Returns a teardown function that removes
 * both listeners. Events for other sessions are ignored.
 */
export async function listenForSession(
	sessionId: string,
	handlers: AgentEventHandlers,
	adapter: TauriAdapter | null = resolveTauri()
): Promise<() => void> {
	if (!adapter) {
		return () => {};
	}

	const unlistenEvent = await adapter.listen(AGENT_EVENT, (payload) => {
		const envelope = payload as Record<string, unknown> | null;
		if (!envelope || envelope.session_id !== sessionId) {
			return;
		}
		const { session_id, ...event } = envelope;
		void session_id;
		handlers.onEvent(event as unknown as AgentEvent);
	});

	const unlistenEnded = await adapter.listen(AGENT_ENDED, (payload) => {
		const envelope = payload as Record<string, unknown> | null;
		if (!envelope || envelope.session_id !== sessionId) {
			return;
		}
		handlers.onEnded();
	});

	return () => {
		unlistenEvent();
		unlistenEnded();
	};
}
