import { API_BASE, ApiError, apiFetch } from './core.ts';

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
	editor: {
		live_preview: boolean;
		default_mode: string;
		strict_line_breaks: boolean;
		show_line_numbers: boolean;
		hide_duplicate_h1: boolean;
		paste_url_image_whitelist: string;
	};
	appearance: {
		theme: string;
	};
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
	const res = await apiFetch(`${API_BASE}/api/capabilities`);
	if (!res.ok) throw new Error(`Failed to load capabilities: ${res.status}`);
	return res.json();
}

export async function getVaultConfig(vault: string): Promise<ConfigResponse & { etag: string }> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/config`);
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
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/config`, {
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
