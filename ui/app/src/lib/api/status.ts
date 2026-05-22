import { API_BASE, apiFetch } from './core.ts';

export type WatcherHealth = 'healthy' | 'degraded' | 'polling';

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
	state: 'ready' | 'rebuilding';
	notes: number;
	tasks: number;
	search_indexed: boolean;
	watcher_active: boolean;
	watcher_health?: WatcherHealth;
	watcher_message?: string;
}

export interface ResourceStatus {
	memory_rss_bytes: number;
	sse_connections: number;
}

export interface RawDaemonStatus {
	version: string;
	started_at: string;
	vaults: Array<{ name: string; state: string; notes: number }>;
	watchers: Array<{ vault: string; state: string; message?: string | null }>;
	indexes: Array<{ vault: string; state: string }>;
	resources: { rss_bytes: number; sse_connections: number };
}

export async function fetchDaemonStatus(): Promise<DaemonStatus> {
	const response = await apiFetch(`${API_BASE}/api/status`);
	if (!response.ok) {
		throw new Error(`Status check failed: ${response.status}`);
	}
	const raw = (await response.json()) as RawDaemonStatus;
	return normalizeDaemonStatus(raw);
}

export async function restartDaemon(): Promise<void> {
	const response = await apiFetch(`${API_BASE}/admin/restart`, { method: 'POST' });
	if (!response.ok) {
		throw new Error(`Restart failed: ${response.status}`);
	}
}

export async function reindexVault(vault: string): Promise<void> {
	const response = await apiFetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(vault)}/reindex`, {
		method: 'POST'
	});
	if (!response.ok) {
		throw new Error(`Reindex failed: ${response.status}`);
	}
}

export async function fetchLogTail(lines = 200): Promise<string> {
	const response = await apiFetch(`${API_BASE}/admin/logs?tail=${lines}`);
	if (!response.ok) {
		throw new Error(`Log fetch failed: ${response.status}`);
	}
	return response.text();
}

export function normalizeDaemonStatus(raw: RawDaemonStatus): DaemonStatus {
	const watcherByVault = new Map(raw.watchers.map((watcher) => [watcher.vault, watcher]));
	const indexByVault = new Map(raw.indexes.map((index) => [index.vault, index]));

	const vaults = Object.fromEntries(
		raw.vaults.map((vault) => {
			const watcher = watcherByVault.get(vault.name);
			const index = indexByVault.get(vault.name);
			return [
				vault.name,
				{
					state: vault.state === 'rebuilding' ? 'rebuilding' : 'ready',
					notes: vault.notes,
					tasks: 0,
					search_indexed: index?.state === 'healthy',
					watcher_active: watcher !== undefined,
					watcher_health: normalizeWatcherHealth(watcher?.state),
					watcher_message: watcher?.message ?? undefined
				} satisfies VaultStatus
			];
		})
	);

	return {
		version: raw.version,
		started_at: raw.started_at,
		uptime_seconds: Math.max(
			0,
			Math.floor((Date.now() - new Date(raw.started_at).getTime()) / 1000)
		),
		vaults,
		resources: {
			memory_rss_bytes: raw.resources.rss_bytes,
			sse_connections: raw.resources.sse_connections
		},
		watchers: {
			active: raw.watchers.filter((watcher) => watcher.state !== 'degraded').length,
			total: raw.watchers.length
		},
		indexes: {
			caches_ok: raw.indexes.filter((index) => index.state === 'healthy').length,
			search_ok: raw.indexes.filter((index) => index.state === 'healthy').length
		}
	};
}

function normalizeWatcherHealth(state?: string): WatcherHealth | undefined {
	switch (state) {
		case 'healthy':
		case 'degraded':
		case 'polling':
			return state;
		default:
			return undefined;
	}
}
