/**
 * Suppresses `note.updated` SSE echoes of the editor's own writes so the
 * "file changed on disk" banner only fires for genuine external changes.
 *
 * Two failure modes the time-based dedup it replaces could not handle:
 *
 *   1. The filesystem watcher emits the `note.updated` SSE event before
 *      the HTTP save response reaches `onSaved`. The dedup must buffer
 *      events received while a save is in flight and resolve them once
 *      the response hash is known.
 *
 *   2. Multiple coalesced or delayed watcher events for the same save
 *      arrive after the save has settled. The dedup must remember a
 *      recent window of accepted hashes, not just the most recent.
 */
export interface ExternalChange {
	path: string;
	hash: string | undefined;
}

export type ExternalChangeOutcome =
	| { kind: 'suppress' }
	| { kind: 'reload' }
	| { kind: 'conflict' }
	| { kind: 'buffered' };

export interface DedupOptions {
	/**
	 * Number of recently-saved hashes to remember. Bounded so a slow trickle
	 * of stale watcher events cannot indefinitely block real conflict
	 * warnings, but large enough to absorb coalesced events from a few
	 * consecutive saves.
	 */
	historyCapacity?: number;
}

export interface ExternalChangeDedup {
	/**
	 * Mark a save as starting. Subsequent `handle` calls for the same path
	 * are buffered until `recordSavedHash` or `cancelSave` is called.
	 */
	beginSave(): void;
	/**
	 * Mark a save as complete with the hash the server wrote. Drains any
	 * buffered events through `handle` and returns the resulting outcomes
	 * for them (without buffering — `beginSave` is over).
	 */
	recordSavedHash(hash: string): ExternalChangeOutcome[];
	/**
	 * Abort an in-flight save without a hash (e.g. save failed). Drains
	 * the buffer and returns the outcomes for the drained events.
	 */
	cancelSave(): ExternalChangeOutcome[];
	/**
	 * Classify an incoming external-change event. Returns `buffered` if
	 * the event was queued for later resolution.
	 */
	handle(change: ExternalChange, currentDirty: boolean): ExternalChangeOutcome;
	/**
	 * Add a hash to the recently-saved ring without going through a save.
	 * Used by side-channel writes such as task toggles whose HTTP response
	 * carries a hash but does not use the auto-save pipeline.
	 */
	rememberHash(hash: string): void;
	/** Reset all state, e.g. when loading a different note. */
	reset(): void;
}

export function createExternalChangeDedup(
	getCurrentHash: () => string | null,
	options: DedupOptions = {}
): ExternalChangeDedup {
	const capacity = options.historyCapacity ?? 16;
	let recentHashes: string[] = [];
	let saveInFlight = false;
	let pending: ExternalChange[] = [];

	function remember(hash: string) {
		if (recentHashes.includes(hash)) {
			return;
		}
		recentHashes.push(hash);
		if (recentHashes.length > capacity) {
			recentHashes.shift();
		}
	}

	function isOwnEcho(hash: string | undefined): boolean {
		if (!hash) {
			return false;
		}
		if (hash === getCurrentHash()) {
			return true;
		}
		return recentHashes.includes(hash);
	}

	function classify(change: ExternalChange, currentDirty: boolean): ExternalChangeOutcome {
		if (isOwnEcho(change.hash)) {
			return { kind: 'suppress' };
		}
		return currentDirty ? { kind: 'conflict' } : { kind: 'reload' };
	}

	function drain(currentDirty: boolean): ExternalChangeOutcome[] {
		if (pending.length === 0) {
			return [];
		}
		const queue = pending;
		pending = [];
		return queue.map((change) => classify(change, currentDirty));
	}

	return {
		beginSave() {
			saveInFlight = true;
		},
		recordSavedHash(hash: string) {
			remember(hash);
			saveInFlight = false;
			// After a successful save the editor is no longer dirty for the
			// content the server just accepted; treat drained events as
			// against a clean editor so any non-echo events trigger a reload
			// rather than a conflict.
			return drain(false);
		},
		cancelSave() {
			saveInFlight = false;
			// Editor remains dirty after a failed save; drained events should
			// be evaluated against `dirty = true` semantically — caller passes
			// its current dirty state when re-running them, so we use `true`
			// to be conservative (prefer surfacing a conflict over a silent
			// reload).
			return drain(true);
		},
		handle(change: ExternalChange, currentDirty: boolean) {
			if (saveInFlight) {
				pending.push(change);
				return { kind: 'buffered' };
			}
			return classify(change, currentDirty);
		},
		rememberHash(hash: string) {
			remember(hash);
		},
		reset() {
			recentHashes = [];
			saveInFlight = false;
			pending = [];
		}
	};
}
