import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';

import { buildLivePreviewDecorationsForState } from './live-preview.ts';

describe('live preview decorations', () => {
	it('produces a block replacement decoration for markdown tables', () => {
		const state = EditorState.create({
			doc: `# Account

| Role | Person |
| --- | --- |
| AE | Alice |`,
			extensions: [markdown({ base: markdownLanguage })]
		});

		const decorations = buildLivePreviewDecorationsForState(state);
		const blocks: Array<{ from: number; to: number; block: boolean | undefined }> = [];
		decorations.between(0, state.doc.length, (from, to, value) => {
			blocks.push({ from, to, block: value.spec.block });
		});

		expect(blocks).toContainEqual({
			from: state.doc.line(3).from,
			to: state.doc.line(5).to,
			block: true
		});
	});
});
