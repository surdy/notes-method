/**
 * App-level "break-glass" toggle (ADR 0012). When OFF (the default) the agent is
 * never offered filesystem or terminal capabilities; vault access is mediated
 * solely through MCP. When ON, the Tauri shell advertises vault-scoped fs read/
 * write + terminal to the agent (still permission-gated and blocked in read-only
 * mode). This is a desktop-app security setting, not vault config, so it is
 * persisted locally rather than in the server-side vault config.
 */

const STORAGE_KEY = 'notesmith:agent-break-glass';

function readPersisted(): boolean {
	try {
		return localStorage.getItem(STORAGE_KEY) === 'true';
	} catch {
		return false;
	}
}

function writePersisted(value: boolean): void {
	try {
		localStorage.setItem(STORAGE_KEY, value ? 'true' : 'false');
	} catch {
		// Ignore storage failures (private mode, quota) — defaults to off.
	}
}

class BreakGlassStore {
	enabled = $state(false);

	/** Hydrate from localStorage; call once on mount. */
	load(): void {
		this.enabled = readPersisted();
	}

	set(value: boolean): void {
		this.enabled = value;
		writePersisted(value);
	}

	toggle(): void {
		this.set(!this.enabled);
	}
}

export const breakGlassStore = new BreakGlassStore();
export { STORAGE_KEY as BREAK_GLASS_STORAGE_KEY };
