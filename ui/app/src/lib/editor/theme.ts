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
	border: '1px solid var(--ns-editor-border)',
	padding: '6px 12px',
	textAlign: 'left',
	verticalAlign: 'top',
	color: 'var(--ns-editor-text)',
	fontWeight: '600'
},
'.cm-lp-table td': {
	border: '1px solid var(--ns-editor-border)',
	padding: '6px 12px',
	verticalAlign: 'top'
},
'.cm-lp-table-cell': {
	minWidth: '72px',
	cursor: 'text',
	outline: 'none'
},
'.cm-lp-table-cell:focus': {
	backgroundColor: 'var(--ns-editor-selection)',
	boxShadow: '0 0 0 1px var(--ns-accent) inset'
}
} as const;

export const livePreviewTableContextMenuTheme = `
.cm-lp-table-context-menu {
	position: fixed;
	z-index: 9999;
	min-width: 180px;
	padding: 4px 0;
	background: var(--ns-surface);
	border: 1px solid var(--ns-border);
	border-radius: 6px;
	box-shadow: 0 4px 16px var(--ns-shadow);
	font-size: 13px;
	color: var(--ns-text);
}
.cm-lp-table-context-menu-item {
	display: block;
	width: 100%;
	padding: 6px 12px;
	border: none;
	background: none;
	text-align: left;
	cursor: pointer;
	color: var(--ns-text);
	font: inherit;
	font-size: 13px;
	line-height: 1.4;
}
.cm-lp-table-context-menu-item:hover:not(:disabled) {
	background: var(--ns-surface-hover);
}
.cm-lp-table-context-menu-item:disabled,
.cm-lp-table-context-menu-item.disabled {
	opacity: 0.4;
	cursor: not-allowed;
}
.cm-lp-table-context-menu-separator {
	height: 1px;
	margin: 4px 8px;
	background: var(--ns-border);
}
`;

export const livePreviewCalloutTheme = {
'.cm-lp-callout': {
	'--ns-callout-current': 'var(--ns-callout-note)',
	'--ns-callout-icon': "'✎'",
	margin: '1em 16px',
	padding: '12px 16px',
	border: '1px solid color-mix(in srgb, var(--ns-callout-current) 42%, transparent)',
	borderLeft: '4px solid var(--ns-callout-current)',
	borderRadius: '8px',
	backgroundColor: 'color-mix(in srgb, var(--ns-callout-current) 13%, var(--ns-editor-bg))',
	color: 'var(--ns-editor-text)'
},
'.cm-lp-callout-title': {
	display: 'flex',
	alignItems: 'center',
	gap: '8px',
	color: 'var(--ns-callout-current)',
	fontWeight: '700'
},
'.cm-lp-callout-title::before': {
	content: 'var(--ns-callout-icon)',
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
	color: 'var(--ns-editor-text-muted)'
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
	'--ns-callout-current': 'var(--ns-callout-note)',
	'--ns-callout-icon': "'✎'"
},
'.cm-lp-callout.callout-abstract': {
	'--ns-callout-current': 'var(--ns-callout-abstract)',
	'--ns-callout-icon': "'☷'"
},
'.cm-lp-callout.callout-info': {
	'--ns-callout-current': 'var(--ns-callout-info)',
	'--ns-callout-icon': "'ⓘ'"
},
'.cm-lp-callout.callout-todo': {
	'--ns-callout-current': 'var(--ns-callout-todo)',
	'--ns-callout-icon': "'☑'"
},
'.cm-lp-callout.callout-tip': {
	'--ns-callout-current': 'var(--ns-callout-tip)',
	'--ns-callout-icon': "'🔥'"
},
'.cm-lp-callout.callout-success': {
	'--ns-callout-current': 'var(--ns-callout-success)',
	'--ns-callout-icon': "'✓'"
},
'.cm-lp-callout.callout-question': {
	'--ns-callout-current': 'var(--ns-callout-question)',
	'--ns-callout-icon': "'?'"
},
'.cm-lp-callout.callout-warning': {
	'--ns-callout-current': 'var(--ns-callout-warning)',
	'--ns-callout-icon': "'⚠'"
},
'.cm-lp-callout.callout-failure': {
	'--ns-callout-current': 'var(--ns-callout-failure)',
	'--ns-callout-icon': "'✕'"
},
'.cm-lp-callout.callout-danger': {
	'--ns-callout-current': 'var(--ns-callout-danger)',
	'--ns-callout-icon': "'⚡'"
},
'.cm-lp-callout.callout-bug': {
	'--ns-callout-current': 'var(--ns-callout-bug)',
	'--ns-callout-icon': "'◉'"
},
'.cm-lp-callout.callout-example': {
	'--ns-callout-current': 'var(--ns-callout-example)',
	'--ns-callout-icon': "'▦'"
},
'.cm-lp-callout.callout-quote': {
	'--ns-callout-current': 'var(--ns-callout-quote)',
	'--ns-callout-icon': "'❝'"
}
} as const;

export const livePreviewCodeBlockTheme = {
	'.cm-lp-code-block': {
		margin: '1em 16px',
		padding: '1em',
		border: '1px solid var(--ns-editor-border)',
		borderRadius: '8px',
		backgroundColor: 'var(--ns-panel-bg-strong)',
		color: 'var(--ns-editor-text)',
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
color: 'var(--ns-editor-text)',
backgroundColor: 'var(--ns-editor-bg)',
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
caretColor: 'var(--ns-editor-text)'
},
'.cm-gutters': {
backgroundColor: 'var(--ns-editor-bg)',
color: 'var(--ns-editor-text-secondary)',
border: 'none'
},
'.cm-activeLineGutter': {
backgroundColor: 'var(--ns-surface-hover)'
},
'.cm-activeLine': {
backgroundColor: 'var(--ns-editor-active-line)'
},
'.cm-selectionBackground, ::selection': {
backgroundColor: 'var(--ns-editor-selection) !important'
},
'.cm-cursor': {
borderLeftColor: 'var(--ns-editor-text)'
},
'.cm-line': {
padding: '0 16px'
},
'.cm-ofm-wikilink': {
color: 'var(--ns-accent)',
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
backgroundColor: 'var(--ns-accent-surface)',
color: 'var(--ns-accent-surface-text)',
fontSize: '12px',
fontWeight: '700',
letterSpacing: '0.03em',
textTransform: 'uppercase'
},
'.cm-frontmatter-line': {
backgroundColor: 'var(--ns-editor-frontmatter-bg)',
color: 'var(--ns-text-muted)'
},
'.cm-frontmatter-gutter': {
backgroundColor: 'var(--ns-editor-frontmatter-bg)'
},
'.cm-frontmatter-delimiter': {
color: '#6a9955',
fontWeight: '600'
},
'.cm-ofm-inline-field-key': {
color: 'var(--ns-accent)',
fontWeight: '600'
},
'.cm-ofm-inline-field-delimiter': {
color: 'var(--ns-text-muted)'
},
'.cm-ofm-task-toggle': {
width: '14px',
height: '14px',
margin: '0 4px 0 0',
accentColor: 'var(--ns-accent)',
verticalAlign: 'middle',
cursor: 'pointer'
},
'.cm-ofm-task-toggle.status-blocked': {
accentColor: 'var(--ns-danger)'
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
'.cm-link': { color: 'var(--ns-accent)', textDecoration: 'underline' },
'.cm-url': { color: 'var(--ns-text-muted)' },
'.cm-meta': { color: 'var(--ns-text-muted)' },
'.cm-comment': { color: '#6a9955' },
'.cm-monospace': { fontFamily: 'ui-monospace, monospace' },
'.cm-sql-result': {
	margin: '8px 16px',
	padding: '8px 12px',
	border: '1px solid var(--ns-border)',
	borderRadius: '6px',
	backgroundColor: 'var(--ns-surface-elevated)',
	fontSize: '13px',
	overflowX: 'auto'
},
'.cm-sql-error': {
	color: 'var(--ns-danger)',
	fontStyle: 'italic'
},
'.cm-sql-loading': {
	color: 'var(--ns-text-muted)'
},
'.cm-sql-empty': {
	color: 'var(--ns-text-muted)',
	fontStyle: 'italic'
},
'.cm-sql-table': {
	width: '100%',
	borderCollapse: 'collapse'
},
'.cm-sql-table th': {
	textAlign: 'left',
	padding: '6px 10px',
	borderBottom: '2px solid var(--ns-border-strong)',
	color: 'var(--ns-accent)',
	fontWeight: '600',
	fontSize: '12px',
	textTransform: 'uppercase'
},
'.cm-sql-table td': {
	padding: '5px 10px',
	borderBottom: '1px solid var(--ns-border)'
},
'.cm-sql-table tr:hover td': {
	backgroundColor: 'var(--ns-surface-hover)'
},
// Live preview mode styles
'.cm-lp-h1': { fontSize: '1.8em', fontWeight: '700', lineHeight: '1.3' },
'.cm-lp-h2': { fontSize: '1.5em', fontWeight: '700', lineHeight: '1.3' },
'.cm-lp-h3': { fontSize: '1.25em', fontWeight: '600', lineHeight: '1.4' },
'.cm-lp-h4': { fontSize: '1.1em', fontWeight: '600', lineHeight: '1.4' },
'.cm-lp-h5': { fontSize: '1.0em', fontWeight: '600', lineHeight: '1.5' },
'.cm-lp-h6': { fontSize: '0.9em', fontWeight: '600', lineHeight: '1.5', color: 'var(--ns-text-faint)' },
'.cm-lp-bold': { fontWeight: 'bold' },
'.cm-lp-italic': { fontStyle: 'italic' },
'.cm-lp-strikethrough': { textDecoration: 'line-through', opacity: '0.7' },
'.cm-lp-link-text': {
	color: 'var(--ns-link)',
	textDecoration: 'underline',
	cursor: 'pointer'
},
'.cm-lp-link-external': {
	color: 'var(--ns-link)',
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
	backgroundColor: 'var(--ns-panel-bg-strong)',
	padding: '1px 4px',
	borderRadius: '3px',
	fontFamily: 'ui-monospace, SFMono-Regular, "SF Mono", Menlo, monospace',
	fontSize: '0.9em'
},
'.cm-lp-hr': {
	border: 'none',
	borderTop: '1px solid var(--ns-border-strong)',
	margin: '8px 0',
	display: 'block'
},
'.cm-lp-bullet': {
	color: 'var(--ns-editor-text-secondary)',
	fontSize: '1.1em',
	marginRight: '1px'
	},
	...livePreviewCalloutTheme,
	...livePreviewCodeBlockTheme,
	...livePreviewTableTheme
	},
	{ dark: true }
);
