import { describe, expect, it } from 'vitest';
import {
	connectionPillLabel,
	connectionVisualState,
	type StatusVisualInputs
} from './status-visual';

function inputs(overrides: Partial<StatusVisualInputs> = {}): StatusVisualInputs {
	return {
		isRebuilding: false,
		restartRequired: false,
		currentVault: 'work',
		connectionState: 'connected',
		hasRecentStatus: true,
		firstStatusResolved: true,
		...overrides
	};
}

describe('connectionVisualState', () => {
	it('is live when connected with a recent status', () => {
		expect(connectionVisualState(inputs())).toBe('live');
	});

	it('shows connecting (not offline) during the initial establishment window', () => {
		// Right after a connection switch reloads the page: SSE not yet open and
		// the first status poll has not resolved.
		expect(
			connectionVisualState(
				inputs({
					connectionState: 'disconnected',
					hasRecentStatus: false,
					firstStatusResolved: false
				})
			)
		).toBe('connecting');
	});

	it('stays connecting while SSE is up but the first status poll is pending', () => {
		expect(
			connectionVisualState(
				inputs({
					connectionState: 'connected',
					hasRecentStatus: false,
					firstStatusResolved: false
				})
			)
		).toBe('connecting');
	});

	it('reports offline only after connecting once and losing the status', () => {
		expect(
			connectionVisualState(
				inputs({
					connectionState: 'connected',
					hasRecentStatus: false,
					firstStatusResolved: true
				})
			)
		).toBe('offline');
	});

	it('reports reconnecting when the SSE stream drops', () => {
		expect(
			connectionVisualState(
				inputs({ connectionState: 'reconnecting', hasRecentStatus: false })
			)
		).toBe('reconnecting');
	});

	it('prioritises rebuilding and restart-required over connection state', () => {
		expect(connectionVisualState(inputs({ isRebuilding: true }))).toBe('rebuilding');
		expect(
			connectionVisualState(inputs({ restartRequired: true, connectionState: 'reconnecting' }))
		).toBe('restart-required');
	});

	describe('no vault selected', () => {
		it('is connecting until the first status poll resolves', () => {
			expect(
				connectionVisualState(
					inputs({ currentVault: '', hasRecentStatus: false, firstStatusResolved: false })
				)
			).toBe('connecting');
		});

		it('is no-vault once a recent status arrives', () => {
			expect(
				connectionVisualState(inputs({ currentVault: '', hasRecentStatus: true }))
			).toBe('no-vault');
		});

		it('is offline when the poll resolved without a recent status', () => {
			expect(
				connectionVisualState(
					inputs({ currentVault: '', hasRecentStatus: false, firstStatusResolved: true })
				)
			).toBe('offline');
		});
	});
});

describe('connectionPillLabel', () => {
	const base = { isRebuilding: false, restarting: false, daemonShuttingDown: false };

	it('labels each visual state', () => {
		expect(connectionPillLabel('live', base)).toBe('Live');
		expect(connectionPillLabel('connecting', base)).toBe('Connecting…');
		expect(connectionPillLabel('reconnecting', base)).toBe('Reconnecting');
		expect(connectionPillLabel('no-vault', base)).toBe('No vault open');
		expect(connectionPillLabel('restart-required', base)).toBe('Restart required');
		expect(connectionPillLabel('rebuilding', base)).toBe('Rebuilding index');
		expect(connectionPillLabel('offline', base)).toBe('Offline');
	});

	it('overrides with restarting and rebuilding regardless of visual state', () => {
		expect(connectionPillLabel('live', { ...base, isRebuilding: true })).toBe('Rebuilding index');
		expect(connectionPillLabel('connecting', { ...base, restarting: true })).toBe('Restarting…');
		expect(connectionPillLabel('live', { ...base, daemonShuttingDown: true })).toBe('Restarting…');
	});
});
