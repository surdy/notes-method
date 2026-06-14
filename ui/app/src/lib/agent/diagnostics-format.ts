/**
 * Pure rendering of an agent-discovery {@link DiagnosticsReport} into a plain-text
 * block suitable for the Settings "Copy" button and bug reports (ADR 0013,
 * decision 5). Kept side-effect free so it can be unit-tested without a DOM.
 */

import type { CandidateDiagnostic, DiagnosticsReport } from './types.ts';

/** Human-readable label for a verdict string. */
export function verdictLabel(verdict: string): string {
	switch (verdict) {
		case 'available':
			return 'available';
		case 'not_found':
			return 'not found';
		case 'probe_failed':
			return 'probe failed';
		default:
			return verdict;
	}
}

function formatCandidate(candidate: CandidateDiagnostic): string[] {
	const lines: string[] = [];
	const found = candidate.foundOnPath
		? `found → ${candidate.resolvedProgram ?? '(unknown)'}`
		: 'not found on PATH';
	const args = candidate.args.length > 0 ? ` ${candidate.args.join(' ')}` : '';
	lines.push(`  - ${candidate.program}${args}: ${found}`);
	if (!candidate.foundOnPath && candidate.searchedDirs.length > 0) {
		lines.push(`    searched: ${candidate.searchedDirs.join(', ')}`);
	}
	if (candidate.probe) {
		const probe = candidate.probe;
		const exit = probe.timedOut ? 'timed out' : `exit ${probe.exitCode ?? 'none'}`;
		lines.push(`    probe: ${probe.command} (${exit})`);
		if (probe.stdoutSnippet) {
			lines.push(`    stdout: ${probe.stdoutSnippet}`);
		}
	}
	return lines;
}

/** Render a full diagnostics report into a copyable plain-text block. */
export function formatDiagnostics(report: DiagnosticsReport): string {
	const lines: string[] = [];
	lines.push('Resolved PATH:');
	if (report.resolvedPath.length === 0) {
		lines.push('  (empty)');
	} else {
		for (const dir of report.resolvedPath) lines.push(`  ${dir}`);
	}
	lines.push('');
	for (const agent of report.agents) {
		lines.push(`${agent.displayName} (${agent.id}): ${verdictLabel(agent.verdict)}`);
		for (const candidate of agent.candidates) {
			lines.push(...formatCandidate(candidate));
		}
		lines.push('');
	}
	return lines.join('\n').trimEnd();
}
