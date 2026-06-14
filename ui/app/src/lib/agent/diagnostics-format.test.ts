import { describe, expect, it } from 'vitest';

import { formatDiagnostics, verdictLabel } from './diagnostics-format.ts';
import type { DiagnosticsReport } from './types.ts';

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
