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
