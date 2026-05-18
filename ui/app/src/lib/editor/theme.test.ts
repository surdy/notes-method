import { describe, expect, it } from 'vitest';

import { livePreviewTableTheme } from './theme.ts';

describe('live preview table theme', () => {
	it('matches the reading view table baseline while preserving editing affordances', () => {
		expect(livePreviewTableTheme['.cm-lp-table-wrapper']).toMatchObject({
			margin: '1em 16px',
			overflowX: 'auto',
			paddingTop: '28px',
			position: 'relative'
		});
		expect(livePreviewTableTheme['.cm-lp-table-toolbar']).toMatchObject({
			position: 'absolute',
			top: '0',
			right: '0'
		});
		expect(livePreviewTableTheme['.cm-lp-table']).toMatchObject({
			borderCollapse: 'collapse'
		});
		expect(livePreviewTableTheme['.cm-lp-table th']).toMatchObject({
			border: '1px solid var(--ns-editor-border)',
			padding: '6px 12px',
			textAlign: 'left',
			verticalAlign: 'top'
		});
		expect(livePreviewTableTheme['.cm-lp-table td']).toMatchObject({
			border: '1px solid var(--ns-editor-border)',
			padding: '6px 12px',
			verticalAlign: 'top'
		});
		expect(livePreviewTableTheme['.cm-lp-table-cell:focus']).toHaveProperty(
			'boxShadow',
			'0 0 0 1px var(--ns-accent) inset'
		);
	});
});
