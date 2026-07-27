import { API_BASE, apiFetch } from './core.ts';

/**
 * Autocomplete values for a registered field, from `fields.toml` — either its
 * enum `values` or the rows its `suggest_from` query returns.
 *
 * The daemon answers `[]` for unknown fields or fields with no suggestion
 * source, so callers can treat "no suggestions" as ordinary rather than an error.
 */
export async function suggestFieldValues(
	vault: string,
	field: string,
	query = ''
): Promise<string[]> {
	const res = await apiFetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/fields/${encodeURIComponent(field)}/suggest?q=${encodeURIComponent(query)}`
	);
	if (!res.ok) throw new Error(`Failed to fetch suggestions for ${field}: ${res.status}`);
	return res.json();
}
