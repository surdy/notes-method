import { EditorView } from '@codemirror/view';

export const notesmithTheme = EditorView.theme(
{
'&': {
color: '#e0e0e0',
backgroundColor: '#1e1e1e',
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
caretColor: '#e0e0e0'
},
'.cm-gutters': {
backgroundColor: '#1e1e1e',
color: '#555',
border: 'none'
},
'.cm-activeLineGutter': {
backgroundColor: '#2a2d2e'
},
'.cm-activeLine': {
backgroundColor: '#2a2d2e44'
},
'.cm-selectionBackground, ::selection': {
backgroundColor: '#264f78 !important'
},
'.cm-cursor': {
borderLeftColor: '#e0e0e0'
},
'.cm-line': {
padding: '0 16px'
},
'.cm-ofm-wikilink': {
color: '#7ec8e3',
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
backgroundColor: '#1f3a4a',
color: '#9cdcfe',
fontSize: '12px',
fontWeight: '700',
letterSpacing: '0.03em',
textTransform: 'uppercase'
},
'.cm-frontmatter-line': {
backgroundColor: '#252526',
color: '#888'
},
'.cm-frontmatter-delimiter': {
color: '#6a9955',
fontWeight: '600'
},
'.cm-ofm-inline-field-key': {
color: '#7ec8e3',
fontWeight: '600'
},
'.cm-ofm-inline-field-delimiter': {
color: '#888'
},
'.cm-ofm-task-toggle': {
width: '14px',
height: '14px',
margin: '0 4px 0 0',
accentColor: '#7ec8e3',
verticalAlign: 'middle',
cursor: 'pointer'
},
'.cm-ofm-task-toggle.cancelled': {
opacity: '0.6',
accentColor: '#ffb347'
},
'.cm-header-1': { fontSize: '1.6em', fontWeight: 'bold' },
'.cm-header-2': { fontSize: '1.3em', fontWeight: 'bold' },
'.cm-header-3': { fontSize: '1.1em', fontWeight: 'bold' },
'.cm-strong': { fontWeight: 'bold' },
'.cm-emphasis': { fontStyle: 'italic' },
'.cm-link': { color: '#7ec8e3', textDecoration: 'underline' },
'.cm-url': { color: '#888' },
'.cm-meta': { color: '#888' },
'.cm-comment': { color: '#6a9955' },
'.cm-monospace': { fontFamily: 'ui-monospace, monospace' }
},
{ dark: true }
);
