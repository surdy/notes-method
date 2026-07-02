// Contract types for the git history / diff UI.
//
// These intentionally mirror the shapes emitted by pterm's Rust git module
// (GitLogEntry, diff content) so that, once the shared `surdy/git-core` crate
// and `@surdy/git-core-types` package exist, this file is replaced by an import
// from that package with no change to the React component below.

export interface GitLogEntry {
	sha: string;
	shortSha: string;
	author: string;
	authorEmail: string;
	timestampSecs: number;
	subject: string;
	filesChanged: number;
	insertions: number;
	deletions: number;
}

export type DiffLineKind = 'context' | 'added' | 'removed' | 'hunk';

export interface DiffLine {
	kind: DiffLineKind;
	oldLine: number | null;
	newLine: number | null;
	text: string;
}

export type DiffFileStatus = 'modified' | 'added' | 'deleted' | 'renamed';

export interface DiffFile {
	path: string;
	status: DiffFileStatus;
	added: number;
	removed: number;
	lines: DiffLine[];
}

export interface CommitDiff {
	sha: string;
	files: DiffFile[];
}

export interface GitHistoryPanelProps {
	commits: GitLogEntry[];
	/**
	 * Resolve the diff for a commit from what the host has already loaded.
	 * Returns `undefined` while the diff is still being fetched.
	 */
	diffForCommit: (sha: string) => CommitDiff | undefined;
	/**
	 * Notify the host that a commit is selected so it can lazily fetch the diff.
	 * Fired on the initial selection and on every subsequent selection change.
	 */
	onSelectCommit?: (sha: string) => void;
	initialSelectedSha?: string;
}
