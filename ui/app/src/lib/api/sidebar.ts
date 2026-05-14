import { API_BASE, ApiError } from './core.ts';

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
