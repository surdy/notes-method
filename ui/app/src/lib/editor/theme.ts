import { EditorView } from '@codemirror/view';

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
lineHeight: '1.6',
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
backgroundColor: 'var(--ns-surface-elevated)',
color: 'var(--ns-text-muted)'
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
}
},
{ dark: true }
);
