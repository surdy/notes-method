/**
 * Shared types for the agent chat feature. These mirror the Rust contract in
 * `crates/notesmith-agent` (event.rs, permission.rs, model.rs) exactly, so the
 * Tauri IPC payloads deserialize without translation.
 */

export type Role = 'user' | 'agent' | 'system';

/** A tool invocation requested by the agent. Mirrors `notesmith_agent::ToolCall`. */
export interface ToolCall {
	id?: string | null;
	name: string;
	args: unknown;
}

/** The result of a tool call. Mirrors `notesmith_agent::ToolResult`. */
export interface ToolResult {
	id?: string | null;
	content: string;
	is_error: boolean;
}

/**
 * The normalized agent event stream. Internally tagged with `type`
 * (snake_case), matching `#[serde(tag = "type", rename_all = "snake_case")]`
 * on `notesmith_agent::AgentEvent`.
 */
export type AgentEvent =
	| { type: 'user_message'; text: string }
	| { type: 'agent_message_delta'; text: string }
	| { type: 'tool_call'; id?: string | null; name: string; args: unknown }
	| { type: 'tool_result'; id?: string | null; content: string; is_error: boolean }
	| { type: 'status'; message: string }
	| { type: 'done'; result?: string | null }
	| { type: 'error'; message: string };

/** One selectable model advertised by the agent. */
export interface ModelOption {
	id: string;
	name: string;
	description?: string | null;
}

/** A normalized model selector (Phase 6). `null` means the agent advertised none. */
export interface ModelPicker {
	current: string;
	options: ModelOption[];
}

/** An agent the user can pick (e.g. Copilot, Claude, Codex). */
export interface AgentInfo {
	id: string;
	name: string;
	available: boolean;
}

/** Context handed to the user when the agent asks to perform a write. */
export interface PermissionRequest {
	tool: string;
	kind?: string | null;
}

/** The user's answer to a {@link PermissionRequest}. */
export type PermissionDecision = 'allow_once' | 'allow_always' | 'deny';

/** Editor context injected with each turn (Phase 5). */
export interface EditorContext {
	activeNote?: string | null;
	selection?: string | null;
	openTabs?: string[];
}

export interface StartSessionOptions {
	vault: string;
	agent: string;
	readOnly: boolean;
	/** Advertise vault-scoped fs/terminal capabilities (Settings break-glass, default off). */
	breakGlass?: boolean;
	/** Reopen an existing transcript thread, re-establishing the ACP session lazily. */
	threadId?: string | null;
}

export interface StartSessionResult {
	sessionId: string;
	models: ModelPicker | null;
}

/**
 * On-demand agent-discovery diagnostics (ADR 0013, decision 5). Mirrors the Rust
 * `DiagnosticsReport` (camelCase JSON) from `agent_diag.rs`.
 */
export interface DiagnosticsReport {
	resolvedPath: string[];
	agents: AgentDiagnostic[];
}

/** Per-agent discovery trace. Mirrors Rust `AgentDiagnostic`. */
export interface AgentDiagnostic {
	id: string;
	displayName: string;
	/** `"available" | "not_found" | "probe_failed"`. */
	verdict: string;
	candidates: CandidateDiagnostic[];
	setupHint: string;
	docsUrl: string;
}

/** Per-candidate discovery trace. Mirrors Rust `CandidateDiagnostic`. */
export interface CandidateDiagnostic {
	program: string;
	args: string[];
	resolvedProgram: string | null;
	foundOnPath: boolean;
	searchedDirs: string[];
	probe: ProbeResult | null;
}

/** The bounded result of a version probe. Mirrors Rust `ProbeResult`. */
export interface ProbeResult {
	command: string;
	exitCode: number | null;
	stdoutSnippet: string;
	timedOut: boolean;
}

/**
 * A single `[agents.<id>]` entry (override or custom agent). Mirrors the Rust
 * `AgentEntryDto` (camelCase JSON). `env` is an array of `[key, value]` pairs so
 * the UI can edit it as ordered rows.
 */
export interface AgentEntryData {
	id: string;
	command: string | null;
	args: string[];
	env: [string, string][];
	displayName: string | null;
	enabled: boolean;
}

/** The `[agents]` config section. Mirrors the Rust `AgentsConfigDto`. */
export interface AgentsConfigData {
	debug: boolean;
	entries: AgentEntryData[];
}
