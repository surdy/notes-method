export { API_BASE, ApiError, encodePath } from './core';

export {
	createNote,
	ensureDaily,
	getNote,
	getNoteHtml,
	getNoteHtmlInline,
	listNotes,
	putNote,
	searchNotes,
	toggleTaskStatus
} from './notes';
export type {
	NoteDetail,
	NoteSummary,
	NoteTask,
	SourcePosition,
	TaskMutationStatus,
	WriteNoteResponse
} from './notes';

export { capture } from './capture';

export { getCapabilities, getVaultConfig, putVaultConfig } from './config';
export type {
	Capabilities,
	ConfigConflictError,
	ConfigResponse,
	ConfigValidationError,
	VaultConfigData
} from './config';

export { addVault, listVaults, reindexVault, removeVault, setDefaultVault, updateVault } from './vaults';
export type { VaultInfo } from './vaults';

export {
	getFolderNotes,
	getSidebarConfig,
	getSidebarConfigWithHash,
	getVaultFolders,
	putSidebarConfig
} from './sidebar';
export type {
	CustomItem,
	FolderNoteItem,
	FolderSource,
	QuerySource,
	SidebarConfig,
	SidebarConfigConflictError,
	SidebarConfigResponse,
	SidebarSection,
	SidebarView
} from './sidebar';

export { instantiateTemplate, listTemplates } from './templates';
export type { TemplatePrompt, TemplateSummary } from './templates';

export { routeApply } from './routing';
export type { RouteApplyResponse, RouteResult } from './routing';

export { executeSql } from './sql';
export type { SqlQueryResult } from './sql';
