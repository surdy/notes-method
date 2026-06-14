/**
 * The agent transport boundary. Per ADR 0012 Decision 4 the Tauri shell hosts
 * the ACP client, spawns the agent process, and streams events to this panel
 * over Tauri IPC. This module abstracts that transport behind {@link AgentClient}
 * so the chat store can be driven by a real Tauri client in the app and a fake
 * in tests, and so the panel degrades cleanly when not running inside Tauri.
 */

import type {
	AgentEvent,
	AgentInfo,
	EditorContext,
	PermissionDecision,
	PermissionRequest,
	StartSessionOptions,
	StartSessionResult
} from './types.ts';

/** Payload delivered when the agent asks permission for a write. */
export interface PermissionEvent {
	sessionId: string;
	requestId: string;
	request: PermissionRequest;
}

export interface AgentClient {
	/** Whether an agent transport is actually available (false in a plain browser). */
	available(): boolean;
	listAgents(): Promise<AgentInfo[]>;
	startSession(opts: StartSessionOptions): Promise<StartSessionResult>;
	sendPrompt(sessionId: string, text: string, editor?: EditorContext): Promise<void>;
	selectModel(sessionId: string, value: string): Promise<void>;
	setReadOnly(sessionId: string, readOnly: boolean): Promise<void>;
	answerPermission(requestId: string, decision: PermissionDecision): Promise<void>;
	stop(sessionId: string): Promise<void>;
	/** Subscribe to the normalized event stream. Returns an unsubscribe fn. */
	onEvent(cb: (sessionId: string, event: AgentEvent) => void): () => void;
	/** Subscribe to permission prompts. Returns an unsubscribe fn. */
	onPermission(cb: (event: PermissionEvent) => void): () => void;
}

const AGENT_EVENT = 'notesmith://agent-event';
const AGENT_PERMISSION = 'notesmith://agent-permission';

interface TauriBridge {
	invoke: (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;
	listen: (event: string, handler: (payload: unknown) => void) => Promise<() => void>;
}

/** Resolve the global Tauri bridge, or `null` when not running inside Tauri. */
export function resolveTauriBridge(): TauriBridge | null {
	const w = globalThis as unknown as {
		__TAURI__?: {
			core?: { invoke?: TauriBridge['invoke'] };
			event?: { listen?: TauriBridge['listen'] };
		};
	};
	const invoke = w.__TAURI__?.core?.invoke;
	const listen = w.__TAURI__?.event?.listen;
	if (!invoke || !listen) return null;
	return { invoke: invoke.bind(w.__TAURI__!.core), listen: listen.bind(w.__TAURI__!.event) };
}

/** Agent client backed by Tauri IPC commands + events. */
export class TauriAgentClient implements AgentClient {
	constructor(private readonly bridge: TauriBridge) {}

	available(): boolean {
		return true;
	}

	async listAgents(): Promise<AgentInfo[]> {
		return (await this.bridge.invoke('agent_list')) as AgentInfo[];
	}

	async startSession(opts: StartSessionOptions): Promise<StartSessionResult> {
		return (await this.bridge.invoke('agent_start', { opts })) as StartSessionResult;
	}

	async sendPrompt(sessionId: string, text: string, editor?: EditorContext): Promise<void> {
		await this.bridge.invoke('agent_prompt', { sessionId, text, editor: editor ?? null });
	}

	async selectModel(sessionId: string, value: string): Promise<void> {
		await this.bridge.invoke('agent_select_model', { sessionId, value });
	}

	async setReadOnly(sessionId: string, readOnly: boolean): Promise<void> {
		await this.bridge.invoke('agent_set_read_only', { sessionId, readOnly });
	}

	async answerPermission(requestId: string, decision: PermissionDecision): Promise<void> {
		await this.bridge.invoke('agent_answer_permission', { requestId, decision });
	}

	async stop(sessionId: string): Promise<void> {
		await this.bridge.invoke('agent_stop', { sessionId });
	}

	onEvent(cb: (sessionId: string, event: AgentEvent) => void): () => void {
		let unlisten: (() => void) | null = null;
		let disposed = false;
		void this.bridge
			.listen(AGENT_EVENT, (payload) => {
				const p = payload as { sessionId: string; event: AgentEvent } | undefined;
				if (p) cb(p.sessionId, p.event);
			})
			.then((fn) => {
				if (disposed) fn();
				else unlisten = fn;
			});
		return () => {
			disposed = true;
			unlisten?.();
		};
	}

	onPermission(cb: (event: PermissionEvent) => void): () => void {
		let unlisten: (() => void) | null = null;
		let disposed = false;
		void this.bridge
			.listen(AGENT_PERMISSION, (payload) => {
				const p = payload as PermissionEvent | undefined;
				if (p) cb(p);
			})
			.then((fn) => {
				if (disposed) fn();
				else unlisten = fn;
			});
		return () => {
			disposed = true;
			unlisten?.();
		};
	}
}

/** No-op client used when the panel runs outside Tauri (e.g. browser dev). */
export class UnavailableAgentClient implements AgentClient {
	available(): boolean {
		return false;
	}
	async listAgents(): Promise<AgentInfo[]> {
		return [];
	}
	async startSession(): Promise<StartSessionResult> {
		throw new Error('Agent transport is unavailable outside the desktop app.');
	}
	async sendPrompt(): Promise<void> {}
	async selectModel(): Promise<void> {}
	async setReadOnly(): Promise<void> {}
	async answerPermission(): Promise<void> {}
	async stop(): Promise<void> {}
	onEvent(): () => void {
		return () => {};
	}
	onPermission(): () => void {
		return () => {};
	}
}

/** Build the appropriate client for the current runtime. */
export function createAgentClient(): AgentClient {
	const bridge = resolveTauriBridge();
	return bridge ? new TauriAgentClient(bridge) : new UnavailableAgentClient();
}
