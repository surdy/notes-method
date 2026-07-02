import { API_BASE, ApiError, apiFetch } from './core.ts';
import type { CommitDiff, GitLogEntry } from '../git-island/types.ts';

/** Working-tree status for a vault's git repository. */
export interface GitStatus {
	changed: string[];
	staged: string[];
	untracked: string[];
	clean: boolean;
}

/** Result of a checkpoint commit. */
export interface GitCommitResult {
	committed: boolean;
	sha: string | null;
	files: string[];
}

/**
 * Number of distinct files with pending changes (changed + staged + untracked),
 * used for the status-bar badge.
 */
export function changedFileCount(status: GitStatus): number {
	const files = new Set<string>([...status.changed, ...status.staged, ...status.untracked]);
	return files.size;
}

/** Fetch the git working-tree status for a vault. */
export async function getGitStatus(vault: string): Promise<GitStatus> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/git/status`);
	if (!res.ok) {
		throw new ApiError(`Failed to load git status: ${res.status}`, res.status);
	}
	return res.json();
}

/**
 * Stage and commit the working tree (a "checkpoint"). When `message` is omitted,
 * the daemon uses the vault's configured message or generates one from the
 * changed-file list.
 */
export async function commitCheckpoint(
	vault: string,
	message?: string
): Promise<GitCommitResult> {
	const res = await apiFetch(`${API_BASE}/api/v/${encodeURIComponent(vault)}/git/commit`, {
		method: 'POST',
		headers: { 'Content-Type': 'application/json' },
		body: JSON.stringify(message ? { message } : {})
	});
	if (!res.ok) {
		throw new ApiError(`Failed to commit: ${res.status}`, res.status);
	}
	return res.json();
}

/** Fetch rich commit history (with per-commit stats) for the git-history UI. */
export async function getGitLog(vault: string, limit = 50): Promise<GitLogEntry[]> {
	const res = await apiFetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/git/log?limit=${limit}`
	);
	if (!res.ok) {
		throw new ApiError(`Failed to load git log: ${res.status}`, res.status);
	}
	return res.json();
}

/** Fetch the full file-level diff for a single commit. */
export async function getCommitDiff(vault: string, sha: string): Promise<CommitDiff> {
	const res = await apiFetch(
		`${API_BASE}/api/v/${encodeURIComponent(vault)}/git/diff/${encodeURIComponent(sha)}`
	);
	if (!res.ok) {
		throw new ApiError(`Failed to load commit diff: ${res.status}`, res.status);
	}
	return res.json();
}
