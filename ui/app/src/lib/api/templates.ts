import { API_BASE } from './core';

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
	const res = await fetch(
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
