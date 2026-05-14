import { API_BASE } from './core.ts';

export interface DaemonStatus {
	version: string;
	started_at: string;
	uptime_seconds: number;
	vaults: Record<string, VaultStatus>;
	resources: ResourceStatus;
	watchers: { active: number; total: number };
	indexes: { caches_ok: number; search_ok: number };
}

export interface VaultStatus {
	notes: number;
	tasks: number;
	search_indexed: boolean;
	watcher_active: boolean;
}

export interface ResourceStatus {
	memory_rss_bytes: number;
	sse_connections: number;
}

export async function fetchDaemonStatus(): Promise<DaemonStatus> {
	const response = await fetch(`${API_BASE}/api/status`);
	if (!response.ok) {
		throw new Error(`Status check failed: ${response.status}`);
	}
	return response.json();
}

export async function restartDaemon(): Promise<void> {
	const response = await fetch(`${API_BASE}/admin/restart`, { method: 'POST' });
	if (!response.ok) {
		throw new Error(`Restart failed: ${response.status}`);
	}
}

export async function reindexVault(vault: string): Promise<void> {
	const response = await fetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(vault)}/reindex`, {
		method: 'POST'
	});
	if (!response.ok) {
		throw new Error(`Reindex failed: ${response.status}`);
	}
}

export async function fetchLogTail(lines = 200): Promise<string> {
	const response = await fetch(`${API_BASE}/admin/logs?tail=${lines}`);
	if (!response.ok) {
		throw new Error(`Log fetch failed: ${response.status}`);
	}
	return response.text();
}
