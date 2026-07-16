import { describe, expect, it } from 'vitest';

import { normalizeDaemonStatus, type RawDaemonStatus } from './status.ts';

function rawStatus(overrides: Partial<RawDaemonStatus['vaults'][number]> = {}): RawDaemonStatus {
	return {
		version: '1.0.0',
		started_at: new Date().toISOString(),
		vaults: [{ name: 'work', state: 'ready', notes: 12, ...overrides }],
		watchers: [{ vault: 'work', state: 'healthy' }],
		indexes: [{ vault: 'work', state: 'healthy' }],
		resources: { rss_bytes: 1000, sse_connections: 0 }
	};
}

describe('normalizeDaemonStatus parse warnings', () => {
	it('defaults to no warnings when the daemon omits the fields', () => {
		const status = normalizeDaemonStatus(rawStatus());
		const vault = status.vaults.work;
		expect(vault.parse_warning_count).toBe(0);
		expect(vault.parse_warnings_truncated).toBe(false);
		expect(vault.parse_warnings).toEqual([]);
	});

	it('threads through per-vault parse warnings', () => {
		const status = normalizeDaemonStatus(
			rawStatus({
				parse_warning_count: 2,
				parse_warnings_truncated: false,
				parse_warnings: [
					{
						path: 'daily/2026-07-15.md',
						stage: 'frontmatter',
						reason: 'invalid YAML: mapping values are not allowed here',
						occurred_at: '2026-07-15T09:12:00Z'
					},
					{
						path: 'refs/broken.md',
						stage: 'frontmatter',
						reason: 'invalid YAML',
						occurred_at: '2026-07-15T09:13:00Z'
					}
				]
			})
		);
		const vault = status.vaults.work;
		expect(vault.parse_warning_count).toBe(2);
		expect(vault.parse_warnings).toHaveLength(2);
		expect(vault.parse_warnings[0].path).toBe('daily/2026-07-15.md');
		expect(vault.parse_warnings[0].stage).toBe('frontmatter');
	});

	it('surfaces the truncated flag when the bound is exceeded', () => {
		const status = normalizeDaemonStatus(
			rawStatus({
				parse_warning_count: 100,
				parse_warnings_truncated: true,
				parse_warnings: []
			})
		);
		expect(status.vaults.work.parse_warnings_truncated).toBe(true);
	});
});
