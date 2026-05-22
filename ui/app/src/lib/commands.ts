import {
	capture,
	createNote,
	ensureDaily,
	getNoteHtmlInline,
	instantiateTemplate,
	listTemplates,
	routeApply
} from './api';
import { goto } from '$app/navigation';
import { base } from '$app/paths';
import { OPEN_QUICK_SWITCHER_EVENT } from './command-events';
import { createOrOpenFolderNote, listFolderPickerItems } from './folder-notes';
import { inputPalette } from './input-palette.svelte';
import { tabStore } from './tab-store.svelte';
import { toastStore } from './toast-store.svelte';
import { vaultStore } from './stores.svelte';

export interface Command {
id: string;
label: string;
category: 'Notes' | 'Tasks' | 'Templates' | 'Navigation' | 'Vault' | 'Settings';
shortcut?: string;
execute: () => void | Promise<void>;
}

export { OPEN_QUICK_SWITCHER_EVENT } from './command-events';

function notifyError(message: string, cause: unknown) {
console.error(message, cause);
toastStore.add(message, 'error');
}

async function reloadAndNavigate(path?: string, onNavigate?: (path: string) => void) {
await vaultStore.loadNotes();
if (path && onNavigate) {
onNavigate(path);
}
}

export function buildCommands(vault: string, onNavigate: (path: string) => void): Command[] {
const requireVault = (): string | null => {
if (!vault) {
toastStore.add('Select a vault first.', 'warning');
return null;
}
return vault;
};

return [
{
id: 'new-note',
label: 'New Note',
category: 'Notes',
shortcut: '⌘N',
execute: () => {
const currentVault = requireVault();
if (!currentVault) return;

inputPalette.open({
steps: [
	{
		mode: 'text',
		label: 'Note title',
		placeholder: 'Enter a title...',
		required: true
	},
	{
		mode: 'text',
		label: 'Folder',
		placeholder: 'Inbox',
		defaultValue: 'Inbox'
	}
],
onComplete: async ([title, folder]) => {
	if (!title?.trim()) return;

	try {
		const created = await createNote(
			currentVault,
			title.trim(),
			'',
			folder?.trim() || undefined
		);
		await reloadAndNavigate(created.path, onNavigate);
	} catch (cause) {
		notifyError('Failed to create note.', cause);
	}
}
});
}
},
{
id: 'capture',
label: 'Quick Capture',
category: 'Notes',
shortcut: '⌘⇧N',
execute: () => {
const currentVault = requireVault();
if (!currentVault) return;

inputPalette.open({
steps: [
	{
		mode: 'text',
		label: 'Capture text',
		placeholder: "What's on your mind?",
		required: true
	},
	{
		mode: 'text',
		label: 'Title (optional)',
		placeholder: 'Auto-generated if empty'
	}
],
onComplete: async ([content, title]) => {
	if (!content?.trim()) return;

	try {
		const created = await capture(currentVault, content.trim(), title?.trim() || undefined);
		await reloadAndNavigate(created.path, onNavigate);
	} catch (cause) {
		notifyError('Failed to capture note.', cause);
	}
}
});
}
},
{
id: 'create-folder-note',
label: 'Create Folder Note',
category: 'Notes',
execute: () => {
const currentVault = requireVault();
if (!currentVault) return;

const folders = listFolderPickerItems(vaultStore.tree);
if (folders.length === 0) {
toastStore.add('No folders available.', 'warning');
return;
}

inputPalette.open({
steps: [
	{
		mode: 'list',
		label: 'Choose a folder',
		items: folders,
		placeholder: 'Search folders...'
	}
],
onComplete: async ([folderPath]) => {
	if (!folderPath) return;

	try {
		const result = await createOrOpenFolderNote({
			vault: currentVault,
			folderPath,
			notes: vaultStore.notes,
			createNote
		});
		if (!result.created) {
			toastStore.add('Folder note already exists.', 'success');
		}
		await reloadAndNavigate(result.path, onNavigate);
	} catch (cause) {
		notifyError('Failed to create folder note.', cause);
	}
}
});
}
},
{
id: 'copy-as-html',
label: 'Copy as HTML',
category: 'Notes',
execute: async () => {
	const currentVault = requireVault();
	if (!currentVault) return;
	if (!tabStore.selectedPath) {
		toastStore.add('Select a note first.', 'warning');
		return;
	}

	try {
		const html = await getNoteHtmlInline(currentVault, tabStore.selectedPath);
		await navigator.clipboard.write([
			new ClipboardItem({
				'text/html': new Blob([html], { type: 'text/html' }),
				'text/plain': new Blob([html], { type: 'text/plain' })
			})
		]);
		toastStore.add('Copied note as HTML.', 'success');
	} catch (cause) {
		notifyError('Failed to copy as HTML.', cause);
	}
}
},
{
id: 'archive-current',
label: 'Archive Current Note',
category: 'Notes',
shortcut: '⌘⇧A',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;
if (!tabStore.selectedPath) {
toastStore.add('Select a note to archive.', 'warning');
return;
}

try {
const response = await routeApply(currentVault, [tabStore.selectedPath]);
await reloadAndNavigate(response.results[0]?.to, onNavigate);
} catch (cause) {
notifyError('Failed to archive the current note.', cause);
}
}
},
{
id: 'open-daily',
label: "Open Today's Daily Note",
category: 'Navigation',
shortcut: '⌘D',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
const daily = await ensureDaily(currentVault);
await reloadAndNavigate(daily.path, onNavigate);
} catch (cause) {
notifyError('Failed to open today\'s daily note.', cause);
}
}
},
{
id: 'new-from-template',
label: 'New Note from Template',
category: 'Templates',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
const templates = await listTemplates(currentVault);
if (templates.length === 0) {
toastStore.add('No templates available.', 'warning');
return;
}

inputPalette.open({
steps: [
{
	mode: 'list',
	label: 'Choose a template',
	items: templates.map((template) => ({
		id: template.name,
		label: template.name,
		description: template.description
	})),
	placeholder: 'Search templates...'
}
],
onComplete: async ([selectedName]) => {
if (!selectedName) return;

const template = templates.find((candidate) => candidate.name === selectedName);
if (!template) return;

if (template.prompts.length > 0) {
	inputPalette.open({
		steps: template.prompts.map((prompt) => ({
			mode: 'text' as const,
			label: `${prompt.name}${prompt.required ? '' : ' (optional)'}`,
			placeholder: `Enter ${prompt.name}...`,
			required: prompt.required
		})),
		onComplete: async (promptValues) => {
			const values: Record<string, string> = {};
			template.prompts.forEach((prompt, index) => {
				const value = promptValues[index]?.trim();
				if (value) {
					values[prompt.name] = value;
				}
			});

			try {
				const created = await instantiateTemplate(
					currentVault,
					template.name,
					values
				);
				await reloadAndNavigate(created.path, onNavigate);
			} catch (cause) {
				notifyError('Failed to create note from template.', cause);
			}
		}
	});
	return;
}

try {
	const created = await instantiateTemplate(currentVault, template.name, {});
	await reloadAndNavigate(created.path, onNavigate);
} catch (cause) {
	notifyError('Failed to create note from template.', cause);
}
}
});
} catch (cause) {
notifyError('Failed to create note from template.', cause);
}
}
},
{
id: 'focus-search',
label: 'Global Search',
category: 'Navigation',
shortcut: '⌘⇧F',
execute: () => {
window.dispatchEvent(new CustomEvent(OPEN_QUICK_SWITCHER_EVENT));
}
},
{
id: 'reload-vault',
label: 'Reload Vault',
category: 'Vault',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
await vaultStore.loadNotes();
} catch (cause) {
notifyError('Failed to reload the vault.', cause);
}
}
},
{
id: 'toggle-view',
label: 'Toggle View Mode',
category: 'Navigation',
shortcut: '⌘E',
execute: () => {
tabStore.toggleViewMode();
}
},
{
id: 'open-settings',
label: 'Settings',
category: 'Settings',
shortcut: '⌘,',
execute: () => {
void goto(`${base}/settings?vault=${encodeURIComponent(vault)}`);
}
}
];
}
