import type { CommitDiff, GitLogEntry } from './types';

// Mock vault history used by the harness to prove the React-island integration
// and native theming. Replaced by real data from the daemon / git-core later.

export const MOCK_COMMITS: GitLogEntry[] = [
	{
		sha: '9f3a1c4e7b2d5a8f0c1e4b7a9d2f6c3e8b1a4d70',
		shortSha: '9f3a1c4',
		author: 'surdy',
		authorEmail: 'surdy@example.com',
		timestampSecs: 1782846000,
		subject: 'notesmith: auto-commit (Daily/2026-06-29.md, Inbox/idea.md)',
		filesChanged: 2,
		insertions: 14,
		deletions: 3
	},
	{
		sha: '2b8e6d10f4c7a93e5b1d8c2f7a0e3b6d9c4f1a25',
		shortSha: '2b8e6d1',
		author: 'surdy',
		authorEmail: 'surdy@example.com',
		timestampSecs: 1782842400,
		subject: 'Customers/Acme: file external meeting notes',
		filesChanged: 1,
		insertions: 22,
		deletions: 0
	},
	{
		sha: 'c1d4f7a90b3e6c28d5a1f8b4e7c0a3d6f9b2e5c8',
		shortSha: 'c1d4f7a',
		author: 'surdy',
		authorEmail: 'surdy@example.com',
		timestampSecs: 1782835200,
		subject: 'notesmith: auto-commit (General/OFM Edge Cases.md)',
		filesChanged: 1,
		insertions: 5,
		deletions: 5
	}
];

const DIFFS: Record<string, CommitDiff> = {
	'9f3a1c4e7b2d5a8f0c1e4b7a9d2f6c3e8b1a4d70': {
		sha: '9f3a1c4e7b2d5a8f0c1e4b7a9d2f6c3e8b1a4d70',
		files: [
			{
				path: 'Daily/2026-06-29.md',
				status: 'modified',
				added: 9,
				removed: 3,
				lines: [
					{ kind: 'hunk', oldLine: null, newLine: null, text: '@@ -1,6 +1,9 @@ # 2026-06-29' },
					{ kind: 'context', oldLine: 1, newLine: 1, text: '# 2026-06-29' },
					{ kind: 'context', oldLine: 2, newLine: 2, text: '' },
					{ kind: 'context', oldLine: 3, newLine: 3, text: '## Open tasks' },
					{ kind: 'removed', oldLine: 4, newLine: null, text: '- [ ] review git integration' },
					{ kind: 'added', oldLine: null, newLine: 4, text: '- [x] review git integration' },
					{ kind: 'added', oldLine: null, newLine: 5, text: '- [ ] scaffold surdy/git-core' },
					{ kind: 'added', oldLine: null, newLine: 6, text: '- [ ] prototype react island' },
					{ kind: 'context', oldLine: 5, newLine: 7, text: '' },
					{ kind: 'context', oldLine: 6, newLine: 8, text: '## Notes' }
				]
			},
			{
				path: 'Inbox/idea.md',
				status: 'modified',
				added: 5,
				removed: 0,
				lines: [
					{ kind: 'hunk', oldLine: null, newLine: null, text: '@@ -0,0 +1,5 @@' },
					{ kind: 'added', oldLine: null, newLine: 1, text: '# Reusable git history component' },
					{ kind: 'added', oldLine: null, newLine: 2, text: '' },
					{ kind: 'added', oldLine: null, newLine: 3, text: 'Share Rust core + TS types across pterm/madari/notesmith.' },
					{ kind: 'added', oldLine: null, newLine: 4, text: 'Notesmith embeds it as a React island for now.' },
					{ kind: 'added', oldLine: null, newLine: 5, text: '' }
				]
			}
		]
	},
	'2b8e6d10f4c7a93e5b1d8c2f7a0e3b6d9c4f1a25': {
		sha: '2b8e6d10f4c7a93e5b1d8c2f7a0e3b6d9c4f1a25',
		files: [
			{
				path: 'Customers/Acme Corp/External Meetings/2026-06-28.md',
				status: 'added',
				added: 22,
				removed: 0,
				lines: [
					{ kind: 'hunk', oldLine: null, newLine: null, text: '@@ -0,0 +1,4 @@' },
					{ kind: 'added', oldLine: null, newLine: 1, text: '---' },
					{ kind: 'added', oldLine: null, newLine: 2, text: 'type: meeting' },
					{ kind: 'added', oldLine: null, newLine: 3, text: 'customer: "[[Acme Corp]]"' },
					{ kind: 'added', oldLine: null, newLine: 4, text: '---' }
				]
			}
		]
	},
	'c1d4f7a90b3e6c28d5a1f8b4e7c0a3d6f9b2e5c8': {
		sha: 'c1d4f7a90b3e6c28d5a1f8b4e7c0a3d6f9b2e5c8',
		files: [
			{
				path: 'General/OFM Edge Cases.md',
				status: 'modified',
				added: 5,
				removed: 5,
				lines: [
					{ kind: 'hunk', oldLine: null, newLine: null, text: '@@ -10,5 +10,5 @@ ## Wikilinks' },
					{ kind: 'removed', oldLine: 10, newLine: null, text: '- [[Note With Spaces]]' },
					{ kind: 'added', oldLine: null, newLine: 10, text: '- [[Note With Spaces|Alias]]' },
					{ kind: 'context', oldLine: 11, newLine: 11, text: '- ![[embed.png]]' }
				]
			}
		]
	}
};

export function mockDiffForCommit(sha: string): CommitDiff | undefined {
	return DIFFS[sha];
}
