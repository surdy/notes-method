import { type Extension } from '@codemirror/state';
import { highlightActiveLineGutter, lineNumbers } from '@codemirror/view';

export function createLineNumberExtensions(showLineNumbers: boolean): Extension[] {
	return showLineNumbers ? [lineNumbers(), highlightActiveLineGutter()] : [];
}
