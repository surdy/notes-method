import { describe, expect, it } from 'vitest';

import { formatDiagEntry, formatDiagnostics, formatTimestamp, verdictLabel } from './diagnostics-format.ts';
import type { DiagEntry, DiagnosticsReport } from './types.ts';

const report: DiagnosticsReport = {
	resolvedPath: ['/opt/homebrew/bin', '/usr/bin'],
	agents: [
		{
			id: 'copilot',
			displayName: 'GitHub Copilot',
			verdict: 'available',
			setupHint: 'Install the Copilot CLI',
			docsUrl: 'https://example.com/copilot',
			candidates: [
				{
					program: 'copilot',
					args: ['--acp'],
					resolvedProgram: '/opt/homebrew/bin/copilot',
					foundOnPath: true,
					searchedDirs: ['/opt/homebrew/bin'],
					probe: {
						command: '/opt/homebrew/bin/copilot --version',
						exitCode: 0,
						stdoutSnippet: 'copilot 1.2.3',
						timedOut: false
					}
				}
			]
		},
		{
			id: 'gemini',
			displayName: 'Gemini',
			verdict: 'not_found',
			setupHint: 'Install the Gemini CLI',
			docsUrl: 'https://example.com/gemini',
			candidates: [
				{
					program: 'gemini',
					args: ['--experimental-acp'],
					resolvedProgram: null,
					foundOnPath: false,
					searchedDirs: ['/opt/homebrew/bin', '/usr/bin'],
					probe: null
				}
			]
		}
	]
};

describe('verdictLabel', () => {
	it('maps known verdicts to human labels', () => {
		expect(verdictLabel('available')).toBe('available');
		expect(verdictLabel('not_found')).toBe('not found');
		expect(verdictLabel('probe_failed')).toBe('probe failed');
		expect(verdictLabel('package_missing')).toBe('package not installed');
	});

	it('passes through unknown verdicts unchanged', () => {
		expect(verdictLabel('weird')).toBe('weird');
	});
});

describe('formatDiagnostics', () => {
	it('renders the resolved PATH and every agent candidate', () => {
		const text = formatDiagnostics(report);
		expect(text).toContain('Resolved PATH:');
		expect(text).toContain('/opt/homebrew/bin');
		expect(text).toContain('GitHub Copilot (copilot): available');
		expect(text).toContain('found → /opt/homebrew/bin/copilot');
		expect(text).toContain('probe: /opt/homebrew/bin/copilot --version (exit 0)');
		expect(text).toContain('stdout: copilot 1.2.3');
		expect(text).toContain('Gemini (gemini): not found');
		expect(text).toContain('not found on PATH');
		expect(text).toContain('searched: /opt/homebrew/bin, /usr/bin');
	});

	it('handles an empty resolved PATH without throwing', () => {
		const text = formatDiagnostics({ resolvedPath: [], agents: [] });
		expect(text).toContain('(empty)');
	});

	it('marks a timed-out probe', () => {
		const text = formatDiagnostics({
			resolvedPath: ['/usr/bin'],
			agents: [
				{
					id: 'codex',
					displayName: 'Codex',
					verdict: 'probe_failed',
					setupHint: '',
					docsUrl: '',
					candidates: [
						{
							program: 'codex-acp',
							args: [],
							resolvedProgram: '/usr/bin/codex-acp',
							foundOnPath: true,
							searchedDirs: [],
							probe: {
								command: 'codex-acp --version',
								exitCode: null,
								stdoutSnippet: '',
								timedOut: true
							}
						}
					]
				}
			]
		});
		expect(text).toContain('probe: codex-acp --version (timed out)');
		expect(text).toContain('Codex (codex): probe failed');
	});
});

describe('formatDiagnostics version info', () => {
	it('renders a detected version and an outdated warning', () => {
		const withVersion: DiagnosticsReport = {
			resolvedPath: ['/usr/bin'],
			agents: [
				{
					id: 'copilot',
					displayName: 'GitHub Copilot',
					verdict: 'available',
					setupHint: '',
					docsUrl: '',
					detectedVersion: '1.2.3',
					versionWarning: 'Detected version 1.2.3 is older than the supported minimum 2.0.0.',
					candidates: []
				}
			]
		};
		const text = formatDiagnostics(withVersion);
		expect(text).toContain('version: 1.2.3');
		expect(text).toContain('warning: Detected version 1.2.3 is older');
	});
});

describe('formatTimestamp', () => {
	it('formats a millisecond epoch as deterministic UTC', () => {
		// 2026-06-16T18:27:47Z
		expect(formatTimestamp(Date.UTC(2026, 5, 16, 18, 27, 47, 500))).toBe('2026-06-16 18:27:47');
	});

	it('renders a dash for missing/invalid timestamps', () => {
		expect(formatTimestamp(0)).toBe('—');
		expect(formatTimestamp(Number.NaN)).toBe('—');
	});
});

describe('formatDiagEntry', () => {
	const ts = Date.UTC(2026, 5, 16, 18, 27, 47);

	it('formats an error entry with agent and detail', () => {
		const entry: DiagEntry = {
			timestampMs: ts,
			kind: 'error',
			agent: 'copilot',
			summary: 'prompt failed: boom',
			detail: 'stack trace'
		};
		expect(formatDiagEntry(entry)).toBe(
			'[2026-06-16 18:27:47] ERROR copilot: prompt failed: boom — stack trace'
		);
	});

	it('formats a wire entry without agent or detail', () => {
		const entry: DiagEntry = {
			timestampMs: ts,
			kind: 'wire',
			agent: null,
			summary: 'prompt → (5 chars)',
			detail: null
		};
		expect(formatDiagEntry(entry)).toBe('[2026-06-16 18:27:47] WIRE: prompt → (5 chars)');
	});
});
