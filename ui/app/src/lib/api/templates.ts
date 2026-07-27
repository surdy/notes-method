import { API_BASE, apiFetch } from './core.ts';

export interface TemplatePrompt {
	name: string;
	type: string;
	required: boolean;
	/**
	 * For `type: field-picker`, the `fields.toml` key whose values to suggest.
	 * The daemon defaults it to the prompt name, so it is always present.
	 */
	field?: string;
}

export interface TemplateSummary {
	name: string;
	description?: string;
	output_path: string;
	prompts: TemplatePrompt[];
}

export async function listTemplates(vault: string): Promise<TemplateSummary[]> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/templates`);
	if (!res.ok) throw new Error(`Failed to list templates: ${res.status}`);
	return res.json();
}

export async function instantiateTemplate(
	vault: string,
	name: string,
	prompts?: Record<string, string>
): Promise<{ path: string }> {
	const res = await apiFetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/templates/${encodeURIComponent(name)}/instantiate`,
		{
			method: 'POST',
			headers: { 'Content-Type': 'application/json' },
			body: JSON.stringify({ prompts })
		}
	);
	if (!res.ok) throw new Error(`Failed to instantiate template: ${res.status}`);
	return res.json();
}
