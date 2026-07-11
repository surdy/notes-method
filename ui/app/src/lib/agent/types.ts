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

/** A proposed file change to preview before deciding (issue #189). */
export interface DiffPreview {
	path: string;
	oldText: string | null;
	newText: string;
}

/** Context handed to the user when the agent asks to perform a write. */
export interface PermissionRequest {
	tool: string;
	kind?: string | null;
	/** The proposed change to preview before deciding; absent for non-file actions. */
	diff?: DiffPreview | null;
}

/**
 * The user's answer to a {@link PermissionRequest} (issue #189):
 * - `allow_once` — allow this single call, remember nothing.
 * - `allow_session` — allow + suppress re-prompts for this tool this session only.
 * - `allow_always` — allow + persist the grant so future sessions never re-prompt.
 * - `deny` — refuse.
 */
export type PermissionDecision = 'allow_once' | 'allow_session' | 'allow_always' | 'deny';

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
	/**
	 * Tools the user has already granted "Always Allow" for this vault (issue
	 * #189). Fetched from the daemon grant store and passed in to pre-seed the
	 * session permission state so they never re-prompt.
	 */
	persistedGrants?: string[];
	/**
	 * One-time session preamble assembled from always-on discovered instructions
	 * and the active persona's body (issues #210/#212). Injected as the agent's
	 * skill/preamble; omitted/`null` when there is nothing to inject.
	 */
	preamble?: string | null;
	/**
	 * The agent's ACP `sessionId` from a prior run of this chat thread, to resume
	 * the conversation via `session/load` instead of starting fresh (#262).
	 * Omitted/`null` starts a new session; a stale id degrades to a fresh session
	 * agent-side, so it is always safe to pass.
	 */
	resumeAcpSessionId?: string | null;
}

export interface StartSessionResult {
	sessionId: string;
	models: ModelPicker | null;
	/**
	 * The agent's resolved ACP `sessionId` for this session (fresh or resumed),
	 * to persist per thread so it can be resumed later (#262). `null` if the
	 * agent returned no session id.
	 */
	acpSessionId: string | null;
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
	/** Version parsed from the probe, normalized to `"major.minor.patch"` (#192). */
	detectedVersion?: string | null;
	/** Warning when the detected version is below the supported minimum (#192). */
	versionWarning?: string | null;
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

/**
 * A single external MCP server (ADR 0016 / #211). Mirrors the Rust
 * `McpServerDto` (camelCase JSON). A non-empty `command` makes it a stdio
 * server; otherwise a non-empty `url` makes it an HTTP server. `env` is an
 * array of `[key, value]` pairs so the UI can edit it as ordered rows.
 */
export interface McpServerData {
	id: string;
	command: string | null;
	args: string[];
	env: [string, string][];
	url: string | null;
	displayName: string | null;
	enabled: boolean;
}

/** The `[mcp]` config section. Mirrors the Rust `McpConfigDto` (#211). */
export interface CompanionMemoryData {
	enabled: boolean;
	serverId: string | null;
	vault: string | null;
	readOnly: boolean;
}

/** The `[mcp]` config section. Mirrors the Rust `McpConfigDto` (#211). */
export interface McpConfigData {
	servers: McpServerData[];
	companionMemory: CompanionMemoryData;
}

/** The kind of a diagnostics log entry. Mirrors Rust `DiagKind` (#192). */
export type DiagKind = 'error' | 'wire';

/**
 * A single bounded diagnostics log entry: a recent agent error or a mediated
 * ACP "wire" message. Mirrors the Rust `DiagEntry` (camelCase JSON) from
 * `diag_log.rs` (#192).
 */
export interface DiagEntry {
	/** Capture time in milliseconds since the Unix epoch. */
	timestampMs: number;
	kind: DiagKind;
	/** The agent (launched program) this entry relates to, when known. */
	agent?: string | null;
	/** A short one-line summary. */
	summary: string;
	/** Optional longer detail, shown expandable in the UI. */
	detail?: string | null;
}
