import { API_BASE, apiFetch } from './core.ts';

export interface RouteLogEntry {
	note_path: string;
	from_path: string;
	to_path: string;
	rule_id?: string;
	mutations_json: Record<string, unknown>;
}

export interface RouteResult {
	from: string;
	to: string;
	rule_id?: string;
	route_log?: RouteLogEntry;
}

export interface RouteApplyResponse {
	routed: number;
	results: RouteResult[];
}

export async function routeApply(vault: string, paths: string[]): Promise<RouteApplyResponse> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/route/apply`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ paths })
	});
	if (!res.ok) throw new Error(`Failed to route: ${res.status}`);
	return res.json();
}
