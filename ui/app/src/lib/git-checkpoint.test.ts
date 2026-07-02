import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const commitCheckpoint = vi.fn();
const getGitStatus = vi.fn();

vi.mock('./api', () => ({
	commitCheckpoint: (...args: unknown[]) => commitCheckpoint(...args),
	getGitStatus: (...args: unknown[]) => getGitStatus(...args),
	changedFileCount: (status: { changed: string[]; staged: string[]; untracked: string[] }) =>
		new Set([...status.changed, ...status.staged, ...status.untracked]).size
}));

async function loadController() {
	vi.stubGlobal('$state', <T>(value: T) => value);
	const mod = await import('./git-checkpoint.svelte.ts');
	return mod;
}

beforeEach(() => {
	vi.useFakeTimers();
	commitCheckpoint.mockResolvedValue({ committed: true, sha: 'abc', files: ['a.md'] });
	getGitStatus.mockResolvedValue({ changed: ['a.md'], staged: [], untracked: [], clean: false });
});

afterEach(() => {
	vi.useRealTimers();
	vi.unstubAllGlobals();
	vi.resetModules();
	vi.clearAllMocks();
});

describe('parseDurationSeconds', () => {
	it('parses s/m/h units', async () => {
		const { parseDurationSeconds } = await loadController();
		expect(parseDurationSeconds('120s')).toBe(120);
		expect(parseDurationSeconds('2m')).toBe(120);
		expect(parseDurationSeconds('1h')).toBe(3600);
	});

	it('rejects invalid input', async () => {
		const { parseDurationSeconds } = await loadController();
		expect(parseDurationSeconds('')).toBeNull();
		expect(parseDurationSeconds('5x')).toBeNull();
		expect(parseDurationSeconds('m')).toBeNull();
	});
});

describe('GitCheckpointController', () => {
	it('does not arm when git is disabled', async () => {
		const { GitCheckpointController } = await loadController();
		const c = new GitCheckpointController();
		c.configure('work', { enabled: false, commit_on_inactivity: '120s' });
		expect(c.inactivityArmed).toBe(false);
		c.activity();
		vi.advanceTimersByTime(200_000);
		expect(commitCheckpoint).not.toHaveBeenCalled();
	});

	it('does not arm when no inactivity window is set', async () => {
		const { GitCheckpointController } = await loadController();
		const c = new GitCheckpointController();
		c.configure('work', { enabled: true, commit_on_inactivity: null });
		expect(c.inactivityArmed).toBe(false);
	});

	it('flushes to disk then commits after the inactivity window', async () => {
		const { GitCheckpointController } = await loadController();
		const c = new GitCheckpointController();
		const order: string[] = [];
		c.registerFlush(() => {
			order.push('flush');
		});
		commitCheckpoint.mockImplementation(async () => {
			order.push('commit');
			return { committed: true, sha: 'abc', files: [] };
		});

		c.configure('work', { enabled: true, commit_on_inactivity: '120s' });
		c.activity();
		expect(commitCheckpoint).not.toHaveBeenCalled();

		await vi.advanceTimersByTimeAsync(120_000);
		expect(order).toEqual(['flush', 'commit']);
		expect(commitCheckpoint).toHaveBeenCalledWith('work', undefined);
	});

	it('resets the countdown on subsequent activity', async () => {
		const { GitCheckpointController } = await loadController();
		const c = new GitCheckpointController();
		c.configure('work', { enabled: true, commit_on_inactivity: '120s' });

		c.activity();
		await vi.advanceTimersByTimeAsync(100_000);
		c.activity(); // resets
		await vi.advanceTimersByTimeAsync(100_000);
		expect(commitCheckpoint).not.toHaveBeenCalled();
		await vi.advanceTimersByTimeAsync(20_000);
		expect(commitCheckpoint).toHaveBeenCalledTimes(1);
	});

	it('refreshes the badge count from git status', async () => {
		const { GitCheckpointController } = await loadController();
		const c = new GitCheckpointController();
		c.configure('work', { enabled: true, commit_on_inactivity: '120s' });
		await vi.runOnlyPendingTimersAsync();
		await Promise.resolve();
		expect(c.changedCount).toBe(1);
	});
});
