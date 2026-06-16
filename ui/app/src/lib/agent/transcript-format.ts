import type { Role } from '../api/transcripts.ts';

/** Thread metadata needed to render an exported transcript header. */
export interface TranscriptMeta {
	title: string;
	agent?: string | null;
	model?: string | null;
	created_at?: string | null;
	updated_at?: string | null;
}

/** A single transcript line for export. */
export interface TranscriptLine {
	role: Role;
	content: string;
	created_at?: string | null;
}

function roleLabel(role: Role): string {
	switch (role) {
		case 'user':
			return 'User';
		case 'agent':
			return 'Agent';
		case 'system':
			return 'System';
		default:
			return role;
	}
}

function yamlValue(value: string | null | undefined): string {
	if (value === null || value === undefined || value === '') return '';
	// Quote to stay valid YAML even when the value contains ':' or other tokens.
	return JSON.stringify(value);
}

/**
 * Render a chat thread as a self-contained markdown note (issue #190). Includes
 * a YAML metadata block (agent, model, timestamps) and role-labelled messages so
 * the export round-trips back into the vault as a readable, indexable note.
 */
export function formatTranscriptMarkdown(meta: TranscriptMeta, messages: TranscriptLine[]): string {
	const fm: string[] = ['---', 'type: chat-transcript'];
	if (meta.agent) fm.push(`agent: ${yamlValue(meta.agent)}`);
	if (meta.model) fm.push(`model: ${yamlValue(meta.model)}`);
	if (meta.created_at) fm.push(`created: ${yamlValue(meta.created_at)}`);
	if (meta.updated_at) fm.push(`updated: ${yamlValue(meta.updated_at)}`);
	fm.push('---');

	const title = meta.title.trim() || 'Chat transcript';
	const body: string[] = [`# ${title}`, ''];

	for (const msg of messages) {
		const stamp = msg.created_at ? ` _(${msg.created_at})_` : '';
		body.push(`**${roleLabel(msg.role)}:**${stamp}`, '', msg.content.trim(), '');
	}

	return `${fm.join('\n')}\n\n${body.join('\n')}`.trimEnd() + '\n';
}
