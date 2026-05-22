import { API_BASE, ApiError, apiFetch } from './core.ts';

export interface VaultInfo {
	name: string;
	path: string;
	is_default: boolean;
}

export async function listVaults(): Promise<VaultInfo[]> {
	const res = await apiFetch(`${API_BASE}/api/app/vaults`);
	if (!res.ok) throw new Error(`Failed to list vaults: ${res.status}`);
	return res.json();
}

export async function addVault(name: string, path: string): Promise<void> {
	const res = await apiFetch(`${API_BASE}/api/app/vaults`, {
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
	const res = await apiFetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(name)}`, {
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
	const res = await apiFetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(name)}`, {
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
	const res = await apiFetch(`${API_BASE}/api/app/default-vault`, {
		method: 'PUT',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify({ name })
	});
	if (!res.ok) throw new ApiError(`Failed to set default vault: ${res.status}`, res.status);
}

export async function reindexVault(name: string): Promise<{ notes: number }> {
	const res = await apiFetch(`${API_BASE}/api/app/vaults/${encodeURIComponent(name)}/reindex`, {
		method: 'POST'
	});
	if (!res.ok) throw new ApiError(`Failed to reindex vault: ${res.status}`, res.status);
	return res.json();
}
