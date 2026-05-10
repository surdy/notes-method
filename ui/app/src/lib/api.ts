const API_BASE = '';

function encodePath(path: string): string {
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
frontmatter: Record<string, unknown>;
}

export interface WriteNoteResponse {
path: string;
hash: string;
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

export async function listNotes(vault: string): Promise<NoteSummary[]> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes`);
if (!res.ok) throw new Error(`Failed to list notes: ${res.status}`);
return res.json();
}

export async function getNote(vault: string, path: string): Promise<NoteDetail> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/notes/${encodePath(path)}`);
if (!res.ok) throw new Error(`Failed to get note: ${res.status}`);
return res.json();
}

export async function getNoteHtml(vault: string, path: string): Promise<string> {
const res = await fetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/html/${encodePath(path)}`);
if (!res.ok) throw new Error(`Failed to render note: ${res.status}`);
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
