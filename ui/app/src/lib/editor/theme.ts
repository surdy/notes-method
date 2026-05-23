import { EditorView } from '@codemirror/view';
import { HighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { tags } from '@lezer/highlight';

/** Override defaultHighlightStyle's heading underline */
export const headingHighlightOverride = syntaxHighlighting(
	HighlightStyle.define([
		{ tag: tags.heading, textDecoration: 'none' }
	])
);

export const livePreviewTableTheme = {
'.cm-lp-table-wrapper': {
	margin: '1em 16px',
	overflowX: 'auto',
	position: 'relative'
},
'.cm-lp-table': {
	borderCollapse: 'collapse'
},
'.cm-lp-table th': {
	border: '1px solid var(--editor-border)',
	padding: '6px 12px',
	textAlign: 'left',
	verticalAlign: 'top',
	color: 'var(--editor-text)',
	fontWeight: '600'
},
'.cm-lp-table td': {
	border: '1px solid var(--editor-border)',
	padding: '6px 12px',
	verticalAlign: 'top'
},
'.cm-lp-table-cell': {
	minWidth: '72px',
	cursor: 'text',
	outline: 'none'
},
'.cm-lp-table-cell:focus': {
	backgroundColor: 'var(--editor-selection)',
	boxShadow: '0 0 0 1px var(--accent) inset'
}
} as const;

export const livePreviewTableContextMenuTheme = `
.cm-lp-table-context-menu {
	position: fixed;
	z-index: 9999;
	min-width: 180px;
	padding: 4px 0;
	background: var(--bg-default);
	border: 1px solid var(--border-default);
	border-radius: 6px;
	box-shadow: 0 4px 16px var(--shadow);
	font-size: 13px;
	color: var(--text-default);
}
.cm-lp-table-context-menu-item {
	display: block;
	width: 100%;
	padding: 6px 12px;
	border: none;
	background: none;
	text-align: left;
	cursor: pointer;
	color: var(--text-default);
	font: inherit;
	font-size: 13px;
	line-height: 1.4;
}
.cm-lp-table-context-menu-item:hover:not(:disabled) {
	background: var(--bg-hover);
}
.cm-lp-table-context-menu-item:disabled,
.cm-lp-table-context-menu-item.disabled {
	opacity: 0.4;
	cursor: not-allowed;
}
.cm-lp-table-context-menu-separator {
	height: 1px;
	margin: 4px 8px;
	background: var(--border-default);
}
`;

export const livePreviewCalloutTheme = {
'.cm-lp-callout': {
	'--callout-current': 'var(--callout-note)',
	'--callout-icon': "'✎'",
	margin: '1em 16px',
	padding: '12px 16px',
	border: '1px solid color-mix(in srgb, var(--callout-current) 42%, transparent)',
	borderLeft: '4px solid var(--callout-current)',
	borderRadius: '8px',
	backgroundColor: 'color-mix(in srgb, var(--callout-current) 13%, var(--editor-bg))',
	color: 'var(--editor-text)'
},
'.cm-lp-callout-title': {
	display: 'flex',
	alignItems: 'center',
	gap: '8px',
	color: 'var(--callout-current)',
	fontWeight: '700'
},
'.cm-lp-callout-title::before': {
	content: 'var(--callout-icon)',
	display: 'inline-flex',
	alignItems: 'center',
	justifyContent: 'center',
	width: '18px',
	flex: '0 0 18px'
},
'.cm-lp-callout[data-fold] .cm-lp-callout-title': {
	cursor: 'pointer'
},
'.cm-lp-callout[data-fold] .cm-lp-callout-title::after': {
	content: "'⌄'",
	marginLeft: 'auto',
	color: 'var(--editor-text-muted)'
},
".cm-lp-callout[data-fold='closed'] .cm-lp-callout-title::after": {
	content: "'›'"
},
".cm-lp-callout[data-fold='closed'] .cm-lp-callout-body": {
	display: 'none'
},
'.cm-lp-callout-body > :first-child': {
	marginTop: '0'
},
'.cm-lp-callout-body > :last-child': {
	marginBottom: '0'
},
'.cm-lp-callout.callout-note': {
	'--callout-current': 'var(--callout-note)',
	'--callout-icon': "'✎'"
},
'.cm-lp-callout.callout-abstract': {
	'--callout-current': 'var(--callout-abstract)',
	'--callout-icon': "'☷'"
},
'.cm-lp-callout.callout-info': {
	'--callout-current': 'var(--callout-info)',
	'--callout-icon': "'ⓘ'"
},
'.cm-lp-callout.callout-todo': {
	'--callout-current': 'var(--callout-todo)',
	'--callout-icon': "'☑'"
},
'.cm-lp-callout.callout-tip': {
	'--callout-current': 'var(--callout-tip)',
	'--callout-icon': "'🔥'"
},
'.cm-lp-callout.callout-success': {
	'--callout-current': 'var(--callout-success)',
	'--callout-icon': "'✓'"
},
'.cm-lp-callout.callout-question': {
	'--callout-current': 'var(--callout-question)',
	'--callout-icon': "'?'"
},
'.cm-lp-callout.callout-warning': {
	'--callout-current': 'var(--callout-warning)',
	'--callout-icon': "'⚠'"
},
'.cm-lp-callout.callout-failure': {
	'--callout-current': 'var(--callout-failure)',
	'--callout-icon': "'✕'"
},
'.cm-lp-callout.callout-danger': {
	'--callout-current': 'var(--callout-danger)',
	'--callout-icon': "'⚡'"
},
'.cm-lp-callout.callout-bug': {
	'--callout-current': 'var(--callout-bug)',
	'--callout-icon': "'◉'"
},
'.cm-lp-callout.callout-example': {
	'--callout-current': 'var(--callout-example)',
	'--callout-icon': "'▦'"
},
'.cm-lp-callout.callout-quote': {
	'--callout-current': 'var(--callout-quote)',
	'--callout-icon': "'❝'"
}
} as const;

export const livePreviewCodeBlockTheme = {
	'.cm-lp-code-block': {
		margin: '1em 16px',
		padding: '1em',
		border: '1px solid var(--editor-border)',
		borderRadius: '8px',
		backgroundColor: 'var(--bg-panel)',
		color: 'var(--editor-text)',
		overflowX: 'auto'
	},
	'.cm-lp-code': {
		padding: '0',
		backgroundColor: 'transparent',
		fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, Monaco, Consolas, monospace',
		fontSize: '0.9em',
		lineHeight: '1.5'
	}
} as const;

export const notesmithTheme = EditorView.theme(
{
'&': {
color: 'var(--editor-text)',
backgroundColor: 'var(--editor-bg)',
height: '100%'
},
'.cm-scroller': {
overflow: 'auto',
fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace'
},
'.cm-content': {
fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
fontSize: '14px',
lineHeight: '1.5',
padding: '16px 0',
caretColor: 'var(--editor-text)'
},
'.cm-gutters': {
backgroundColor: 'var(--editor-bg)',
color: 'var(--editor-text-secondary)',
border: 'none'
},
'.cm-activeLineGutter': {
backgroundColor: 'var(--bg-hover)'
},
'.cm-activeLine': {
backgroundColor: 'var(--editor-active-line)'
},
'.cm-selectionBackground, ::selection': {
backgroundColor: 'var(--editor-selection) !important'
},
'.cm-cursor': {
borderLeftColor: 'var(--editor-text)'
},
'.cm-line': {
padding: '0 16px'
},
'.cm-ofm-wikilink': {
color: 'var(--accent)',
textDecoration: 'underline',
textDecorationStyle: 'dotted',
cursor: 'pointer'
},
'.cm-ofm-tag': {
color: '#d7ba7d',
fontWeight: '600'
},
'.cm-ofm-callout': {
display: 'inline-block',
padding: '1px 6px',
borderRadius: '999px',
backgroundColor: 'var(--accent-bg)',
color: 'var(--accent-text)',
fontSize: '12px',
fontWeight: '700',
letterSpacing: '0.03em',
textTransform: 'uppercase'
},
'.cm-frontmatter-line': {
backgroundColor: 'var(--editor-frontmatter-bg)',
color: 'var(--text-muted)'
},
'.cm-frontmatter-gutter': {
backgroundColor: 'var(--editor-frontmatter-bg)'
},
'.cm-frontmatter-delimiter': {
color: '#6a9955',
fontWeight: '600'
},
'.cm-ofm-inline-field-key': {
color: 'var(--accent)',
fontWeight: '600'
},
'.cm-ofm-inline-field-delimiter': {
color: 'var(--text-muted)'
},
'.cm-ofm-task-toggle': {
width: '14px',
height: '14px',
margin: '0 4px 0 0',
accentColor: 'var(--accent)',
verticalAlign: 'middle',
cursor: 'pointer'
},
'.cm-ofm-task-toggle.status-blocked': {
accentColor: 'var(--color-danger)'
},
'.cm-ofm-task-toggle.status-waiting': {
accentColor: '#d7ba7d'
},
'.cm-ofm-task-toggle.status-on-hold': {
accentColor: '#b180d7'
},
'.cm-ofm-task-toggle.status-cancelled': {
opacity: '0.6',
accentColor: '#ffb347'
},
'.cm-header-1': { fontSize: '1.6em', fontWeight: 'bold' },
'.cm-header-2': { fontSize: '1.3em', fontWeight: 'bold' },
'.cm-header-3': { fontSize: '1.1em', fontWeight: 'bold' },
'.cm-strong': { fontWeight: 'bold' },
'.cm-emphasis': { fontStyle: 'italic' },
'.cm-link': { color: 'var(--accent)', textDecoration: 'underline' },
'.cm-url': { color: 'var(--text-muted)' },
'.cm-meta': { color: 'var(--text-muted)' },
'.cm-comment': { color: '#6a9955' },
'.cm-monospace': { fontFamily: 'ui-monospace, monospace' },
'.cm-sql-result': {
	margin: '8px 16px',
	padding: '8px 12px',
	border: '1px solid var(--border-default)',
	borderRadius: '6px',
	backgroundColor: 'var(--bg-elevated)',
	fontSize: '13px',
	overflowX: 'auto'
},
'.cm-sql-error': {
	color: 'var(--color-danger)',
	fontStyle: 'italic'
},
'.cm-sql-loading': {
	color: 'var(--text-muted)'
},
'.cm-sql-empty': {
	color: 'var(--text-muted)',
	fontStyle: 'italic'
},
'.cm-sql-table': {
	width: '100%',
	borderCollapse: 'collapse'
},
'.cm-sql-table th': {
	textAlign: 'left',
	padding: '6px 10px',
	borderBottom: '2px solid var(--border-strong)',
	color: 'var(--accent)',
	fontWeight: '600',
	fontSize: '12px',
	textTransform: 'uppercase'
},
'.cm-sql-table td': {
	padding: '5px 10px',
	borderBottom: '1px solid var(--border-default)'
},
'.cm-sql-table tr:hover td': {
	backgroundColor: 'var(--bg-hover)'
},
// Live preview mode styles
'.cm-lp-h1': { fontSize: '1.8em', fontWeight: '700', lineHeight: '1.3' },
'.cm-lp-h2': { fontSize: '1.5em', fontWeight: '700', lineHeight: '1.3' },
'.cm-lp-h3': { fontSize: '1.25em', fontWeight: '600', lineHeight: '1.4' },
'.cm-lp-h4': { fontSize: '1.1em', fontWeight: '600', lineHeight: '1.4' },
'.cm-lp-h5': { fontSize: '1.0em', fontWeight: '600', lineHeight: '1.5' },
'.cm-lp-h6': { fontSize: '0.9em', fontWeight: '600', lineHeight: '1.5', color: 'var(--text-faint)' },
'.cm-lp-bold': { fontWeight: 'bold' },
'.cm-lp-italic': { fontStyle: 'italic' },
'.cm-lp-strikethrough': { textDecoration: 'line-through', opacity: '0.7' },
'.cm-lp-link-text': {
	color: 'var(--accent)',
	textDecoration: 'underline',
	cursor: 'pointer'
},
'.cm-lp-link-external': {
	color: 'var(--accent)',
	textDecoration: 'underline',
	cursor: 'pointer'
},
'.cm-lp-link-external::after': {
	content: '"↗"',
	display: 'inline-block',
	marginLeft: '0.15em',
	fontSize: '0.85em',
	verticalAlign: 'baseline',
	opacity: '0.7'
},
'.cm-lp-inline-code': {
	backgroundColor: 'var(--bg-panel)',
	padding: '1px 4px',
	borderRadius: '3px',
	fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
	fontSize: '0.9em'
},
'.cm-lp-hr': {
	border: 'none',
	borderTop: '1px solid var(--border-strong)',
	margin: '8px 0',
	display: 'block'
},
'.cm-lp-bullet': {
	color: 'var(--editor-text-secondary)',
	fontSize: '1.1em',
	marginRight: '1px'
	},
	...livePreviewCalloutTheme,
	...livePreviewCodeBlockTheme,
	...livePreviewTableTheme
	},
	{ dark: true }
);
