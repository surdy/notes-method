export {
	API_BASE,
	CLIENT_SCHEMA_VERSION,
	CLIENT_VERSION,
	ApiError,
	apiFetch,
	checkVersionHeaders,
	classifyVersionCompatibility,
	encodePath,
	versionMismatch
} from './core.ts';
export type { VersionInfo, VersionMismatchDirection } from './core.ts';

export {
	createNote,
	ensureDaily,
	getNote,
	getNoteHtml,
	getNoteHtmlInline,
	listNotes,
	putNote,
	renameFolder,
	searchNotes,
	toggleTaskStatus
} from './notes.ts';
export type {
	NoteDetail,
	NoteSummary,
	NoteTask,
	RenameFolderResponse,
	SourcePosition,
	TaskMutationStatus,
	WriteNoteResponse
} from './notes.ts';

export { capture } from './capture.ts';

export { getCapabilities, getVaultConfig, putVaultConfig } from './config.ts';
export type {
	Capabilities,
	ConfigConflictError,
	ConfigResponse,
	ConfigValidationError,
	VaultConfigData
} from './config.ts';

export { addVault, listVaults, reindexVault, removeVault, setDefaultVault, updateVault } from './vaults.ts';
export type { VaultInfo } from './vaults.ts';

export { fetchDaemonStatus, fetchLogTail, restartDaemon } from './status.ts';
export type { DaemonStatus, ResourceStatus, VaultStatus } from './status.ts';

export {
	getFolderNotes,
	getSidebarConfig,
	getSidebarConfigWithHash,
	getVaultFolders,
	putSidebarConfig
} from './sidebar.ts';
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
} from './sidebar.ts';

export { instantiateTemplate, listTemplates } from './templates.ts';
export type { TemplatePrompt, TemplateSummary } from './templates.ts';

export { routeApply } from './routing.ts';
export type { RouteApplyResponse, RouteResult } from './routing.ts';

export { executeSql } from './sql.ts';
export type { SqlQueryResult } from './sql.ts';
