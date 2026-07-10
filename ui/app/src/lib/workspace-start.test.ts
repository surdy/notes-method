import { describe, expect, it } from 'vitest';
import {
	buildRecentList,
	formatDateLabel,
	formatRelativeTime,
	START_ACTIONS
} from './workspace-start';

describe('formatRelativeTime', () => {
	const now = 1_000_000_000_000;

	it('returns "just now" for very recent or future timestamps', () => {
		expect(formatRelativeTime(now, now)).toBe('just now');
		expect(formatRelativeTime(now - 30_000, now)).toBe('just now');
		expect(formatRelativeTime(now + 5_000, now)).toBe('just now');
	});

	it('formats minutes and hours', () => {
		expect(formatRelativeTime(now - 2 * 60_000, now)).toBe('2m ago');
		expect(formatRelativeTime(now - 3 * 3_600_000, now)).toBe('3h ago');
	});

	it('formats yesterday and day ranges', () => {
		expect(formatRelativeTime(now - 25 * 3_600_000, now)).toBe('yesterday');
		expect(formatRelativeTime(now - 3 * 86_400_000, now)).toBe('3d ago');
	});

	it('falls back to an absolute date beyond a week', () => {
		const label = formatRelativeTime(now - 30 * 86_400_000, now);
		expect(label).not.toMatch(/ago|yesterday|just now/);
		expect(label.length).toBeGreaterThan(0);
	});

	it('never throws on non-finite input', () => {
		expect(formatRelativeTime(Number.NaN, now)).toBe('');
		expect(formatRelativeTime(Number.POSITIVE_INFINITY, now)).toBe('');
	});
});

describe('formatDateLabel', () => {
	it('formats a backend timestamp into a short, tz-stable label', () => {
		expect(formatDateLabel('2026-06-25 23:40')).toBe('Jun 25');
		expect(formatDateLabel('2026-01-02')).toBe('Jan 2');
	});

	it('returns empty string for unparseable or out-of-range input', () => {
		expect(formatDateLabel('')).toBe('');
		expect(formatDateLabel('not-a-date')).toBe('');
		expect(formatDateLabel('2026-13-40')).toBe('');
	});
});

describe('buildRecentList', () => {
	const now = 1_000_000_000_000;

	it('prefers viewed entries and labels them by relative time', () => {
		const result = buildRecentList(
			[{ path: 'a.md', title: 'A', timestamp: now - 2 * 60_000 }],
			[{ path: 'b.md', title: 'B', updatedAt: '2026-06-25 23:40' }],
			5,
			now
		);
		expect(result).toEqual([
			{ path: 'a.md', title: 'A', label: '2m ago' },
			{ path: 'b.md', title: 'B', label: 'Jun 25' }
		]);
	});

	it('deduplicates by path, keeping the viewed entry', () => {
		const result = buildRecentList(
			[{ path: 'a.md', title: 'A', timestamp: now - 60_000 }],
			[{ path: 'a.md', title: 'A edited', updatedAt: '2026-06-25' }],
			5,
			now
		);
		expect(result).toHaveLength(1);
		expect(result[0]).toEqual({ path: 'a.md', title: 'A', label: '1m ago' });
	});

	it('caps at the requested limit', () => {
		const viewed = Array.from({ length: 10 }, (_, i) => ({
			path: `n${i}.md`,
			title: `N${i}`,
			timestamp: now - i * 60_000
		}));
		expect(buildRecentList(viewed, [], 3, now)).toHaveLength(3);
	});

	it('falls back to path when title is empty and skips pathless entries', () => {
		const result = buildRecentList(
			[
				{ path: '', title: 'ghost', timestamp: now },
				{ path: 'c.md', title: '', timestamp: now }
			],
			[],
			5,
			now
		);
		expect(result).toEqual([{ path: 'c.md', title: 'c.md', label: 'just now' }]);
	});
});

describe('START_ACTIONS', () => {
	it('exposes the four convergent quick-start actions with one primary', () => {
		expect(START_ACTIONS.map((a) => a.command)).toEqual([
			'new-note',
			'quick-switcher',
			'open-daily',
			'capture'
		]);
		expect(START_ACTIONS.filter((a) => a.primary)).toHaveLength(1);
		expect(START_ACTIONS.every((a) => a.shortcut && a.label && a.icon)).toBe(true);
	});
});
