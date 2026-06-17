export type DockSegment = 'context' | 'chat';

type ReadableStorage = Pick<Storage, 'getItem'>;
type WritableStorage = Pick<Storage, 'setItem'>;

const KEY_PREFIX = 'notesmith:dock-segment:';

function browserStorage(): Storage | null {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
}

export function normalizeDockSegment(value: unknown): DockSegment | null {
	return value === 'context' || value === 'chat' ? value : null;
}

export function dockSegmentKey(vault: string | null | undefined): string | null {
	if (!vault) return null;
	return `${KEY_PREFIX}${vault}`;
}

export function loadDockSegment(
	vault: string | null | undefined,
	storage: ReadableStorage | null = browserStorage()
): DockSegment {
	const key = dockSegmentKey(vault);
	if (!key || !storage) return 'context';
	try {
		return normalizeDockSegment(storage.getItem(key)) ?? 'context';
	} catch {
		return 'context';
	}
}

export function saveDockSegment(
	vault: string | null | undefined,
	segment: DockSegment,
	storage: WritableStorage | null = browserStorage()
): void {
	const key = dockSegmentKey(vault);
	if (!key || !storage) return;
	try {
		storage.setItem(key, segment);
	} catch {}
}

/** The Context-segment sub-tabs shown in the unified right-dock tab row. */
export type RailTab = 'metadata' | 'links' | 'toc';

const RAIL_TAB_KEY_PREFIX = 'notesmith:rail-tab:';

export function normalizeRailTab(value: unknown): RailTab | null {
	return value === 'metadata' || value === 'links' || value === 'toc' ? value : null;
}

export function railTabKey(vault: string | null | undefined): string | null {
	if (!vault) return null;
	return `${RAIL_TAB_KEY_PREFIX}${vault}`;
}

export function loadRailTab(
	vault: string | null | undefined,
	storage: ReadableStorage | null = browserStorage()
): RailTab {
	const key = railTabKey(vault);
	if (!key || !storage) return 'metadata';
	try {
		return normalizeRailTab(storage.getItem(key)) ?? 'metadata';
	} catch {
		return 'metadata';
	}
}

export function saveRailTab(
	vault: string | null | undefined,
	tab: RailTab,
	storage: WritableStorage | null = browserStorage()
): void {
	const key = railTabKey(vault);
	if (!key || !storage) return;
	try {
		storage.setItem(key, tab);
	} catch {}
}

/** A single tab in the unified right-dock tab row (Right 5 layout). */
export type DockTabId = RailTab | 'chat';

export interface DockTabView {
	id: DockTabId;
	label: string;
	/** Context sub-tabs vs. the Chat segment — chat is rendered with the accent. */
	kind: 'context' | 'chat';
	active: boolean;
}

/**
 * Build the unified tab row that drives both the dock segment (Context vs. Chat)
 * and, within Context, the active sub-tab. A context tab is active only while the
 * Context segment is showing; the Chat tab is active whenever the Chat segment is.
 */
export function dockTabs(segment: DockSegment, railTab: RailTab): DockTabView[] {
	const context = segment === 'context';
	return [
		{ id: 'metadata', label: 'Metadata', kind: 'context', active: context && railTab === 'metadata' },
		{ id: 'links', label: 'Links', kind: 'context', active: context && railTab === 'links' },
		{ id: 'toc', label: 'TOC', kind: 'context', active: context && railTab === 'toc' },
		{ id: 'chat', label: 'Chat', kind: 'chat', active: segment === 'chat' }
	];
}

/**
 * The note title shown in the right-dock toolbar: the file's basename (with
 * extension), or an empty string when no note is selected.
 */
export function dockTitle(path: string | null | undefined): string {
	if (!path) return '';
	return path.split('/').pop() ?? path;
}
