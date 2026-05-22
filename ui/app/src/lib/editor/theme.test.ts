import { describe, expect, it } from 'vitest';

import { livePreviewCalloutTheme, livePreviewCodeBlockTheme, livePreviewTableContextMenuTheme, livePreviewTableTheme } from './theme.ts';

describe('live preview table theme', () => {
	it('matches the reading view table baseline while preserving editing affordances', () => {
		expect(livePreviewTableTheme['.cm-lp-table-wrapper']).toMatchObject({
			margin: '1em 16px',
			overflowX: 'auto',
			position: 'relative'
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

	it('has context menu CSS for fixed-position floating panel', () => {
		expect(livePreviewTableContextMenuTheme).toContain('.cm-lp-table-context-menu');
		expect(livePreviewTableContextMenuTheme).toContain('position: fixed');
		expect(livePreviewTableContextMenuTheme).toContain('.cm-lp-table-context-menu-item');
		expect(livePreviewTableContextMenuTheme).toContain('.cm-lp-table-context-menu-separator');
	});
});

describe('live preview callout theme', () => {
	it('matches reading-view callout styling and supports folded state', () => {
		expect(livePreviewCalloutTheme['.cm-lp-callout']).toMatchObject({
			borderLeft: '4px solid var(--ns-callout-current)',
			backgroundColor: 'color-mix(in srgb, var(--ns-callout-current) 13%, var(--ns-editor-bg))'
		});
		expect(livePreviewCalloutTheme['.cm-lp-callout-title::before']).toMatchObject({
			content: 'var(--ns-callout-icon)'
		});
		expect(livePreviewCalloutTheme[".cm-lp-callout[data-fold='closed'] .cm-lp-callout-body"]).toMatchObject({
			display: 'none'
		});
		expect(livePreviewCalloutTheme['.cm-lp-callout.callout-warning']).toMatchObject({
			'--ns-callout-current': 'var(--ns-callout-warning)',
			'--ns-callout-icon': "'⚠'"
		});
	});
});

describe('live preview code block theme', () => {
	it('matches reading-view code block styling', () => {
		expect(livePreviewCodeBlockTheme['.cm-lp-code-block']).toMatchObject({
			margin: '1em 16px',
			padding: '1em',
			border: '1px solid var(--ns-editor-border)',
			backgroundColor: 'var(--ns-panel-bg-strong)',
			overflowX: 'auto'
		});
		expect(livePreviewCodeBlockTheme['.cm-lp-code']).toMatchObject({
			fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Monaco, Consolas, monospace',
			backgroundColor: 'transparent'
		});
	});
});
