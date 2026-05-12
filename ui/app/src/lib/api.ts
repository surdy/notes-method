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

export async function inboxCapture(
vault: string,
content: string,
title?: string
): Promise<WriteNoteResponse> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/inbox`, {
method: 'POST',
headers: { 'Content-Type': 'application/json' },
body: JSON.stringify({ text: content, title })
});
if (!res.ok) throw new Error(`Failed to capture to inbox: ${res.status}`);
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

export async function routeApply(vault: string, paths?: string[]): Promise<RouteApplyResponse> {
const body = paths && paths.length > 0 ? { paths } : { inbox: true };
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/route/apply`, {
method: 'POST',
headers: { 'Content-Type': 'application/json' },
body: JSON.stringify(body)
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

export async function getSidebarConfig(vault: string): Promise<SidebarConfig> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/sidebar-config`);
if (!res.ok) throw new Error(`Failed to load sidebar config: ${res.status}`);
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
