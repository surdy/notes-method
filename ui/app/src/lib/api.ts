const API_BASE = '';

export class ApiError extends Error {
	constructor(
		message: string,
		public readonly status: number
	) {
		super(message);
		this.name = 'ApiError';
	}
}

export function encodePath(path: string): string {
return path
.split('/')
.map((segment) => encodeURIComponent(segment))
.join('/');
}

export interface NoteSummary {
path: string;
title: string;
type: string;
customer?: string;
date?: string;
archived: boolean;
}

export interface NoteDetail {
path: string;
body: string;
frontmatter: Record<string, unknown> | null;
raw_frontmatter?: string | null;
tasks?: NoteTask[];
hash: string;
}

export interface WriteNoteResponse {
path: string;
hash: string;
}

export type TaskMutationStatus =
	| 'todo'
	| 'in_progress'
	| 'blocked'
	| 'waiting'
	| 'on_hold'
	| 'done'
	| 'cancelled';

export interface SourcePosition {
	line: number;
	column: number;
	offset: number;
	length: number;
}

export interface NoteTask {
	status: string;
	content: string;
	position: SourcePosition;
	content_hash?: string | null;
}

export interface RouteResult {
from: string;
to: string;
rule_id?: string;
}

export interface RouteApplyResponse {
routed: number;
results: RouteResult[];
}

export interface TemplatePrompt {
name: string;
type: string;
required: boolean;
}

export interface TemplateSummary {
name: string;
description?: string;
output_path: string;
prompts: TemplatePrompt[];
}

export interface SidebarConfig {
views: SidebarView[];
}

export interface SidebarView {
id: string;
name: string;
icon: string;
sections: SidebarSection[];
badge_query?: string;
}

export type SidebarSection =
	| { type: 'recently-viewed'; label: string; mode: 'viewed' | 'edited' | 'both'; limit: number }
	| { type: 'custom-folders'; label: string; folders: string[] }
	| { type: 'custom-items'; label: string; items: CustomItem[] };

export interface CustomItem {
name: string;
icon: string;
source: FolderSource | QuerySource;
}

export interface FolderSource {
folder: string;
recursive?: boolean;
sort?: 'modified' | 'created' | 'name';
sort_dir?: 'asc' | 'desc';
}

export interface QuerySource {
query: string;
title_column?: string;
subtitle_column?: string;
badge_columns?: string[];
}

export interface FolderNoteItem {
path: string;
title: string;
snippet: string;
modified_at?: string;
created_at?: string;
}

export interface SqlQueryResult {
columns: string[];
rows: Record<string, unknown>[];
}

interface RawSqlQueryResult {
columns: string[];
rows: unknown[][];
row_count: number;
}

export async function listNotes(vault: string): Promise<NoteSummary[]> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes`);
if (!res.ok) throw new Error(`Failed to list notes: ${res.status}`);
return res.json();
}

export async function getNote(vault: string, path: string): Promise<NoteDetail> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes/${encodePath(path)}`);
if (!res.ok) throw new ApiError(`Failed to get note: ${res.status}`, res.status);
return res.json();
}

export async function getNoteHtml(vault: string, path: string): Promise<string> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/html/${encodePath(path)}`);
if (!res.ok) throw new Error(`Failed to render note: ${res.status}`);
return res.text();
}

export async function getNoteHtmlInline(vault: string, path: string): Promise<string> {
const res = await fetch(
`${API_BASE}/api/v/${encodeURIComponent(vault)}/html/${encodePath(path)}?inline_styles=true`
);
if (!res.ok) throw new Error(`Failed to render note HTML: ${res.status}`);
return res.text();
}

export async function searchNotes(vault: string, query: string): Promise<NoteSummary[]> {
const res = await fetch(
`${API_BASE}/api/v/${encodeURIComponent(vault)}/search?q=${encodeURIComponent(query)}`
);
if (!res.ok) throw new Error(`Search failed: ${res.status}`);
return res.json();
}

export async function createNote(
vault: string,
title: string,
content: string,
folder?: string
): Promise<WriteNoteResponse> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes`, {
method: 'POST',
headers: { 'Content-Type': 'application/json' },
body: JSON.stringify({ title, content, folder })
});
if (!res.ok) throw new Error(`Failed to create note: ${res.status}`);
return res.json();
}

export async function putNote(
	vault: string,
	path: string,
	content: string,
	expectedHash?: string | null
): Promise<WriteNoteResponse> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes/${encodePath(path)}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ content, expected_hash: expectedHash ?? undefined })
	});
	if (!res.ok) throw new ApiError(`Failed to save note: ${res.status}`, res.status);
	return res.json();
}

export async function toggleTaskStatus(
	vault: string,
	notePath: string,
	taskHash: string,
	status: TaskMutationStatus
): Promise<WriteNoteResponse> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/tasks/toggle`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({
			note_path: notePath,
			task_hash: taskHash,
			status
		})
	});
	if (!res.ok) throw new ApiError(`Failed to toggle task: ${res.status}`, res.status);
	return res.json();
}

export async function capture(
vault: string,
content: string,
title?: string
): Promise<WriteNoteResponse> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/capture`, {
method: 'POST',
headers: { 'Content-Type': 'application/json' },
body: JSON.stringify({ text: content, title })
});
if (!res.ok) throw new Error(`Failed to capture note: ${res.status}`);
return res.json();
}

export async function ensureDaily(
vault: string,
date?: string
): Promise<{ path: string; created: boolean }> {
const day = date ?? new Date().toISOString().slice(0, 10);
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/daily/${day}`, {
method: 'POST'
});
if (!res.ok) throw new Error(`Failed to ensure daily: ${res.status}`);
return res.json();
}

export async function routeApply(vault: string, paths: string[]): Promise<RouteApplyResponse> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/route/apply`, {
method: 'POST',
headers: { 'Content-Type': 'application/json' },
body: JSON.stringify({ paths })
});
if (!res.ok) throw new Error(`Failed to route: ${res.status}`);
return res.json();
}

export async function listTemplates(vault: string): Promise<TemplateSummary[]> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/templates`);
if (!res.ok) throw new Error(`Failed to list templates: ${res.status}`);
return res.json();
}

export async function instantiateTemplate(
vault: string,
name: string,
prompts?: Record<string, string>
): Promise<{ path: string }> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/templates/${encodeURIComponent(name)}/instantiate`, {
method: 'POST',
headers: { 'Content-Type': 'application/json' },
body: JSON.stringify({ prompts })
});
if (!res.ok) throw new Error(`Failed to instantiate template: ${res.status}`);
return res.json();
}

export interface SidebarConfigResponse {
	config: SidebarConfig;
	hash: string;
	path: string;
	warnings: Record<string, string>;
	etag: string;
}

export interface SidebarConfigConflictError {
	error: 'conflict';
	message: string;
	config: SidebarConfig;
	hash: string;
	warnings: Record<string, string>;
}

export async function getSidebarConfig(vault: string): Promise<SidebarConfig> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/sidebar-config`);
	if (!res.ok) throw new Error(`Failed to load sidebar config: ${res.status}`);
	const data = await res.json();
	return data.config ?? data;
}

export async function getSidebarConfigWithHash(
	vault: string
): Promise<SidebarConfigResponse> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/sidebar-config`);
	if (!res.ok) throw new Error(`Failed to load sidebar config: ${res.status}`);
	const data = await res.json();
	const etag = res.headers.get('etag')?.replace(/"/g, '') ?? data.hash;
	return { ...data, etag };
}

export async function putSidebarConfig(
	vault: string,
	config: SidebarConfig,
	etag: string
): Promise<SidebarConfigResponse> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/sidebar-config`, {
		method: 'PUT',
		headers: {
			'Content-Type': 'application/json',
			'If-Match': `"${etag}"`
		},
		body: JSON.stringify(config)
	});

	if (res.status === 409) {
		const data: SidebarConfigConflictError = await res.json();
		throw Object.assign(new ApiError('Sidebar config conflict', 409), { conflict: data });
	}
	if (res.status === 422) {
		const data = await res.json();
		throw Object.assign(new ApiError('Validation failed', 422), { validation: data });
	}
	if (res.status === 428) {
		throw new ApiError('If-Match header required', 428);
	}
	if (!res.ok) throw new ApiError(`Failed to save sidebar config: ${res.status}`, res.status);

	const saved = await res.json();
	const newEtag = res.headers.get('etag')?.replace(/"/g, '') ?? saved.hash;
	return { ...saved, etag: newEtag };
}

export async function getVaultFolders(vault: string): Promise<string[]> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/folders`);
	if (!res.ok) throw new Error(`Failed to load folders: ${res.status}`);
	return res.json();
}

export async function getFolderNotes(
	vault: string,
	params: {
		path: string;
		recursive?: boolean;
		limit?: number;
		sort?: string;
		sort_dir?: string;
	}
): Promise<FolderNoteItem[]> {
	const qs = new URLSearchParams({ path: params.path });
	if (params.recursive) qs.set('recursive', 'true');
	if (params.limit) qs.set('limit', String(params.limit));
	if (params.sort) qs.set('sort', params.sort);
	if (params.sort_dir) qs.set('sort_dir', params.sort_dir);
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/folder-notes?${qs}`);
	if (!res.ok) throw new Error(`Failed to load folder notes: ${res.status}`);
	const data = (await res.json()) as { notes: FolderNoteItem[] };
	return data.notes;
}

export async function executeSql(vault: string, sql: string): Promise<SqlQueryResult> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/query/sql`, {
	method: 'POST',
	headers: { 'Content-Type': 'application/json' },
	body: JSON.stringify({ sql })
});
if (!res.ok) throw new Error(`SQL query failed: ${res.status}`);

const raw = (await res.json()) as RawSqlQueryResult;
return {
	columns: raw.columns,
	rows: raw.rows.map((values) =>
		Object.fromEntries(raw.columns.map((column, index) => [column, values[index]]))
	)
};
}

export interface Capabilities {
	deployment_mode: 'desktop' | 'hosted';
	can_edit_global_config: boolean;
	can_edit_vault_config: boolean;
	can_open_local_paths: boolean;
	restart_required_fields: string[];
	folder_picker: boolean;
	vaults_root: string | null;
}

export interface VaultConfigData {
	name: string;
	homepage?: string | null;
	capture: { folder: string; template: string };
	daily: {
		folder: string;
		template: string;
		generate_at?: string | null;
		timezone?: string | null;
		catch_up: boolean;
	};
	editor: { live_preview: boolean; default_mode: string };
	git: {
		enabled: boolean;
		auto_commit_every?: string | null;
		auto_pull_every?: string | null;
		auto_push_every?: string | null;
		commit_message?: string | null;
	};
	hooks: {
		on_note_create?: string | null;
		on_daily_create?: string | null;
	};
}

export interface ConfigResponse {
	config: VaultConfigData;
	hash: string;
	path: string;
	warnings: Record<string, string>;
}

export interface ConfigValidationError {
	error: 'validation_failed';
	errors: Record<string, string>;
}

export interface ConfigConflictError {
	error: 'conflict';
	message: string;
	config: VaultConfigData;
	hash: string;
	warnings: Record<string, string>;
}

export async function getCapabilities(): Promise<Capabilities> {
	const res = await fetch(`${API_BASE}/api/capabilities`);
	if (!res.ok) throw new Error(`Failed to load capabilities: ${res.status}`);
	return res.json();
}

export async function getVaultConfig(vault: string): Promise<ConfigResponse & { etag: string }> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/config`);
	if (!res.ok) throw new Error(`Failed to load vault config: ${res.status}`);
	const data: ConfigResponse = await res.json();
	const etag = res.headers.get('etag')?.replace(/"/g, '') ?? data.hash;
	return { ...data, etag };
}

export async function putVaultConfig(
	vault: string,
	config: VaultConfigData,
	etag: string
): Promise<ConfigResponse & { etag: string }> {
	const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/config`, {
		method: 'PUT',
		headers: {
			'Content-Type': 'application/json',
			'If-Match': `"${etag}"`
		},
		body: JSON.stringify(config)
	});

	if (res.status === 409) {
		const data: ConfigConflictError = await res.json();
		throw Object.assign(new ApiError('Config conflict', 409), { conflict: data });
	}
	if (res.status === 422) {
		const data: ConfigValidationError = await res.json();
		throw Object.assign(new ApiError('Validation failed', 422), { validation: data });
	}
	if (res.status === 428) {
		throw new ApiError('If-Match header required', 428);
	}
	if (res.status === 403) {
		throw new ApiError('Write not allowed', 403);
	}
	if (!res.ok) throw new ApiError(`Failed to save config: ${res.status}`, res.status);

	const data: ConfigResponse = await res.json();
	const newEtag = res.headers.get('etag')?.replace(/"/g, '') ?? data.hash;
	return { ...data, etag: newEtag };
}

// ── Vault management API ────────────────────────────────────────────────────

export interface VaultInfo {
	name: string;
	path: string;
	is_default: boolean;
}

export async function listVaults(): Promise<VaultInfo[]> {
	const res = await fetch(`${API_BASE}/api/app/vaults`);
	if (!res.ok) throw new Error(`Failed to list vaults: ${res.status}`);
	return res.json();
}

export async function addVault(name: string, path: string): Promise<void> {
	const res = await fetch(`${API_BASE}/api/app/vaults`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name, path })
	});
	if (res.status === 409) {
		const data = await res.json();
		throw new ApiError(data.message ?? 'Vault already exists', 409);
	}
	if (res.status === 422) {
		const data = await res.json();
		throw new ApiError(data.message ?? 'Invalid path', 422);
	}
	if (!res.ok) throw new ApiError(`Failed to add vault: ${res.status}`, res.status);
}

export async function updateVault(
	name: string,
	newName?: string
): Promise<void> {
	const body: Record<string, string> = {};
	if (newName) body.name = newName;
	const res = await fetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(name)}`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(body)
	});
	if (res.status === 409) {
		const data = await res.json();
		throw new ApiError(data.message ?? 'Vault name conflict', 409);
	}
	if (res.status === 404) {
		throw new ApiError('Vault not found', 404);
	}
	if (!res.ok) throw new ApiError(`Failed to update vault: ${res.status}`, res.status);
}

export async function removeVault(name: string): Promise<void> {
	const res = await fetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(name)}`, {
		method: 'DELETE'
	});
	if (res.status === 422) {
		const data = await res.json();
		throw new ApiError(data.message ?? 'Cannot remove default vault', 422);
	}
	if (res.status === 404) {
		throw new ApiError('Vault not found', 404);
	}
	if (!res.ok) throw new ApiError(`Failed to remove vault: ${res.status}`, res.status);
}

export async function setDefaultVault(name: string): Promise<void> {
	const res = await fetch(`${API_BASE}/api/app/default-vault`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name })
	});
	if (!res.ok) throw new ApiError(`Failed to set default vault: ${res.status}`, res.status);
}

export async function reindexVault(name: string): Promise<{ notes: number }> {
	const res = await fetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(name)}/reindex`, {
		method: 'POST'
	});
	if (!res.ok) throw new ApiError(`Failed to reindex vault: ${res.status}`, res.status);
	return res.json();
}
