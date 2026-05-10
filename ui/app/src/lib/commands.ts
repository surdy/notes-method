import {
createNote,
ensureDaily,
inboxCapture,
instantiateTemplate,
listTemplates,
routeApply,
type TemplatePrompt
} from './api';
import { vaultStore } from './stores.svelte';

export interface Command {
id: string;
label: string;
category: 'Notes' | 'Tasks' | 'Templates' | 'Navigation' | 'Vault';
shortcut?: string;
execute: () => void | Promise<void>;
}

export const OPEN_QUICK_SWITCHER_EVENT = 'notesmith:open-quick-switcher';

function promptValue(message: string, defaultValue = ''): string | null {
return window.prompt(message, defaultValue);
}

function notifyError(message: string, cause: unknown) {
console.error(message, cause);
window.alert(message);
}

async function reloadAndNavigate(path?: string, onNavigate?: (path: string) => void) {
await vaultStore.loadNotes();
if (path && onNavigate) {
onNavigate(path);
}
}

async function collectTemplatePrompts(prompts: TemplatePrompt[]): Promise<Record<string, string> | null> {
const values: Record<string, string> = {};

for (const prompt of prompts) {
const label = `${prompt.name}${prompt.required ? ' (required)' : ' (optional)'}`;
const value = promptValue(`Enter ${label}:`);
if (value === null) {
return null;
}

if (!value.trim()) {
if (prompt.required) {
window.alert(`Template prompt "${prompt.name}" is required.`);
return null;
}
continue;
}

values[prompt.name] = value;
}

return values;
}

export function buildCommands(vault: string, onNavigate: (path: string) => void): Command[] {
const requireVault = (): string | null => {
if (!vault) {
window.alert('Select a vault first.');
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
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

const title = promptValue('Note title:');
if (!title?.trim()) return;

const folder = promptValue('Folder (optional):', 'Inbox')?.trim();
try {
const created = await createNote(currentVault, title.trim(), '', folder || undefined);
await reloadAndNavigate(created.path, onNavigate);
} catch (cause) {
notifyError('Failed to create note.', cause);
}
}
},
{
id: 'inbox-capture',
label: 'Quick Capture to Inbox',
category: 'Notes',
shortcut: '⌘⇧I',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

const content = promptValue('Capture text:');
if (!content?.trim()) return;

const title = promptValue('Title (optional):')?.trim();
try {
const created = await inboxCapture(currentVault, content.trim(), title || undefined);
await reloadAndNavigate(created.path, onNavigate);
} catch (cause) {
notifyError('Failed to capture note.', cause);
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
if (!vaultStore.selectedPath) {
window.alert('Select a note to archive.');
return;
}

try {
const response = await routeApply(currentVault, [vaultStore.selectedPath]);
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
shortcut: '⌘⇧N',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
const templates = await listTemplates(currentVault);
if (templates.length === 0) {
window.alert('No templates available.');
return;
}

const selection = promptValue(
`Choose a template by name:\n${templates
.map((template) => `- ${template.name}${template.description ? ` — ${template.description}` : ''}`)
.join('\n')}`
);
if (!selection?.trim()) return;

const template = templates.find((candidate) => candidate.name === selection.trim());
if (!template) {
window.alert(`Unknown template: ${selection}`);
return;
}

const prompts = await collectTemplatePrompts(template.prompts);
if (prompts === null) return;

const created = await instantiateTemplate(currentVault, template.name, prompts);
await reloadAndNavigate(created.path, onNavigate);
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
id: 'route-inbox',
label: 'Route All Inbox Notes',
category: 'Vault',
execute: async () => {
const currentVault = requireVault();
if (!currentVault) return;

try {
await routeApply(currentVault);
await vaultStore.loadNotes();
} catch (cause) {
notifyError('Failed to route inbox notes.', cause);
}
}
}
];
}
