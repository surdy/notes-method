import { describe, expect, it } from 'vitest';

import { workspaceChromeLayout } from './workspace-chrome.ts';

describe('workspaceChromeLayout', () => {
	it('aligns expanded side chrome with expanded sidebars', () => {
		expect(workspaceChromeLayout({ leftSidebarCollapsed: false, rightRailCollapsed: false })).toEqual({
			leftChromeWidth: '280px',
			rightChromeWidth: '260px',
			leftToggleLabel: 'Collapse left sidebar',
			rightToggleLabel: 'Collapse right sidebar'
		});
	});

	it('keeps compact side controls visible when sidebars are collapsed', () => {
		expect(workspaceChromeLayout({ leftSidebarCollapsed: true, rightRailCollapsed: true })).toEqual({
			leftChromeWidth: '44px',
			rightChromeWidth: '44px',
			leftToggleLabel: 'Expand left sidebar',
			rightToggleLabel: 'Expand right sidebar'
		});
	});
});
