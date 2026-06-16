import { describe, expect, it } from 'vitest';

import { ALLOW_OPTIONS } from './permission-options.ts';

describe('ALLOW_OPTIONS', () => {
	it('exposes exactly the three allow tiers in order', () => {
		expect(ALLOW_OPTIONS.map((o) => o.decision)).toEqual([
			'allow_once',
			'allow_session',
			'allow_always'
		]);
	});

	it('labels each tier for the prompt', () => {
		const byDecision = Object.fromEntries(ALLOW_OPTIONS.map((o) => [o.decision, o.label]));
		expect(byDecision.allow_once).toMatch(/once/i);
		expect(byDecision.allow_session).toMatch(/session/i);
		expect(byDecision.allow_always).toMatch(/always/i);
	});
});
