import { changedFileCount, commitCheckpoint, getGitStatus, type GitCommitResult } from './api';

type FlushFn = () => Promise<void> | void;

/** Parse a duration string like `"120s"`, `"2m"`, `"1h"` into seconds. */
export function parseDurationSeconds(value: string): number | null {
	const match = value.trim().match(/^(\d+)([smh])$/);
	if (!match) return null;
	const amount = Number.parseInt(match[1], 10);
	switch (match[2]) {
		case 's':
			return amount;
		case 'm':
			return amount * 60;
		case 'h':
			return amount * 3600;
		default:
			return null;
	}
}

/**
 * Drives Tolaria-style inactivity checkpoints for the active vault.
 *
 * The editor calls {@link activity} on every change; once edits have been idle
 * for the configured `commit_on_inactivity` window, the controller flushes any
 * unsaved editor buffer to disk (via the registered flush hook) and then commits
 * a checkpoint through the daemon. It also tracks the changed-file count for the
 * status-bar badge.
 */
export class GitCheckpointController {
	/** Number of files with pending changes (for the status-bar badge). */
	changedCount = $state(0);
	/** Whether git integration is enabled for the active vault. */
	gitEnabled = $state(false);
	/** Whether a checkpoint commit is currently in flight. */
	committing = $state(false);

	private vault = '';
	private inactivityMs = 0;
	private timer: ReturnType<typeof setTimeout> | null = null;
	private flushFn: FlushFn | null = null;
	private statusTimer: ReturnType<typeof setTimeout> | null = null;

	/** Register the editor's flush-to-disk callback (or `null` to clear it). */
	registerFlush(fn: FlushFn | null): void {
		this.flushFn = fn;
	}

	/** True when inactivity checkpoints are armed (enabled + valid window). */
	get inactivityArmed(): boolean {
		return this.gitEnabled && this.inactivityMs > 0;
	}

	/** Apply the active vault's git config; (re)arms the timer and badge. */
	configure(vault: string, git: { enabled: boolean; commit_on_inactivity?: string | null }): void {
		this.vault = vault;
		this.gitEnabled = git.enabled;
		const seconds =
			git.enabled && git.commit_on_inactivity
				? parseDurationSeconds(git.commit_on_inactivity)
				: null;
		this.inactivityMs = seconds && seconds > 0 ? seconds * 1000 : 0;
		this.clearTimer();
		if (!git.enabled) {
			this.changedCount = 0;
			return;
		}
		void this.refreshStatus();
	}

	/** Signal editor activity; resets the inactivity countdown. */
	activity(): void {
		if (!this.inactivityArmed) return;
		this.clearTimer();
		this.timer = setTimeout(() => {
			this.timer = null;
			void this.commitNow().catch(() => {
				// Automatic checkpoint failure is non-fatal; badge reflects state.
			});
		}, this.inactivityMs);
	}

	/** Called after a successful editor save; refreshes the badge (debounced). */
	notifySaved(): void {
		if (!this.gitEnabled) return;
		if (this.statusTimer) clearTimeout(this.statusTimer);
		this.statusTimer = setTimeout(() => {
			this.statusTimer = null;
			void this.refreshStatus();
		}, 1500);
	}

	/**
	 * Flush the editor buffer to disk, then commit a checkpoint. Best-effort:
	 * failures are swallowed (the badge reflects the resulting state). Safe to
	 * call manually (e.g. a "commit now" action).
	 */
	async commitNow(message?: string): Promise<GitCommitResult | null> {
		if (!this.vault || this.committing) return null;
		this.committing = true;
		let result: GitCommitResult | null = null;
		try {
			await this.flushFn?.();
			result = await commitCheckpoint(this.vault, message);
		} catch (err) {
			this.committing = false;
			await this.refreshStatus();
			throw err;
		}
		this.committing = false;
		await this.refreshStatus();
		return result;
	}

	/** Refresh the changed-file count from the daemon. */
	async refreshStatus(): Promise<void> {
		if (!this.vault || !this.gitEnabled) {
			this.changedCount = 0;
			return;
		}
		try {
			this.changedCount = changedFileCount(await getGitStatus(this.vault));
		} catch {
			// Not a repo / disabled — leave the last known count.
		}
	}

	/** Cancel any pending timers (e.g. on teardown). */
	stop(): void {
		this.clearTimer();
		if (this.statusTimer) {
			clearTimeout(this.statusTimer);
			this.statusTimer = null;
		}
	}

	private clearTimer(): void {
		if (this.timer) {
			clearTimeout(this.timer);
			this.timer = null;
		}
	}
}

export const gitCheckpoint = new GitCheckpointController();
