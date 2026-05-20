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

	it('renders markdown callouts as block replacements when the cursor is outside the callout', () => {
		const state = EditorState.create({
			doc: `# Callouts

> [!warning] Watch out
> Check the migration plan.`,
			extensions: [markdown({ base: markdownLanguage })]
		});

		const decorations = buildLivePreviewDecorationsForState(state);
		const blocks: Array<{ from: number; to: number; block: boolean | undefined }> = [];
		decorations.between(0, state.doc.length, (from, to, value) => {
			blocks.push({ from, to, block: value.spec.block });
		});

		expect(blocks).toContainEqual({
			from: state.doc.line(3).from,
			to: state.doc.line(4).to,
			block: true
		});
	});

	it('keeps markdown callouts in source form when the cursor is inside the callout', () => {
		const state = EditorState.create({
			doc: `# Callouts

> [!warning] Watch out
> Check the migration plan.`,
			selection: { anchor: 15 },
			extensions: [markdown({ base: markdownLanguage })]
		});

		const decorations = buildLivePreviewDecorationsForState(state);
		const blocks: Array<{ from: number; to: number; block: boolean | undefined }> = [];
		decorations.between(0, state.doc.length, (from, to, value) => {
			blocks.push({ from, to, block: value.spec.block });
		});

		expect(blocks).not.toContainEqual({
			from: state.doc.line(3).from,
			to: state.doc.line(4).to,
			block: true
		});
	});

	it('allows rendered callouts to receive mouse input so clicking can move the cursor into source', () => {
		const state = EditorState.create({
			doc: `# Callouts

> [!warning] Watch out
> Check the migration plan.`,
			extensions: [markdown({ base: markdownLanguage })]
		});

		const decorations = buildLivePreviewDecorationsForState(state);
		const widgets: Array<{ ignoreEvent: (event: Event) => boolean }> = [];
		decorations.between(0, state.doc.length, (_from, _to, value) => {
			if (value.spec.widget) {
				widgets.push(value.spec.widget as { ignoreEvent: (event: Event) => boolean });
			}
		});

		expect(widgets[0]?.ignoreEvent(new Event('mousedown'))).toBe(false);
	});

	it('renders fenced code blocks as block replacements when the cursor is outside the code block', () => {
		const state = EditorState.create({
			doc: `# Code

\`\`\`ts
const answer = 42;
\`\`\``,
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

	it('keeps fenced code blocks in source form when the cursor is inside the code block', () => {
		const state = EditorState.create({
			doc: `# Code

\`\`\`ts
const answer = 42;
\`\`\``,
			selection: { anchor: 15 },
			extensions: [markdown({ base: markdownLanguage })]
		});

		const decorations = buildLivePreviewDecorationsForState(state);
		const blocks: Array<{ from: number; to: number; block: boolean | undefined }> = [];
		decorations.between(0, state.doc.length, (from, to, value) => {
			blocks.push({ from, to, block: value.spec.block });
		});

		expect(blocks).not.toContainEqual({
			from: state.doc.line(3).from,
			to: state.doc.line(5).to,
			block: true
		});
	});

	it('allows rendered fenced code blocks to receive mouse input so clicking can move the cursor into source', () => {
		const state = EditorState.create({
			doc: `# Code

\`\`\`ts
const answer = 42;
\`\`\``,
			extensions: [markdown({ base: markdownLanguage })]
		});

		const decorations = buildLivePreviewDecorationsForState(state);
		const widgets: Array<{ ignoreEvent: (event: Event) => boolean }> = [];
		decorations.between(0, state.doc.length, (_from, _to, value) => {
			if (value.spec.widget) {
				widgets.push(value.spec.widget as { ignoreEvent: (event: Event) => boolean });
			}
		});

		expect(widgets[0]?.ignoreEvent(new Event('mousedown'))).toBe(false);
	});

	it('marks markdown links with web URLs as external', () => {
		const state = EditorState.create({
			doc: `# Heading\n\nSee [docs](https://example.com) and [note](some-note.md).`,
			selection: { anchor: 0 },
			extensions: [markdown({ base: markdownLanguage })]
		});

		const decorations = buildLivePreviewDecorationsForState(state);
		const marks: Array<{ from: number; to: number; class?: string }> = [];
		decorations.between(0, state.doc.length, (from, to, value) => {
			if (value.spec.class) {
				marks.push({ from, to, class: value.spec.class as string });
			}
		});

		const linkMarks = marks.filter(
			(m) => m.class === 'cm-lp-link-text' || m.class === 'cm-lp-link-external'
		);
		const externalDocs = linkMarks.find((m) => state.doc.sliceString(m.from, m.to) === 'docs');
		const internalNote = linkMarks.find((m) => state.doc.sliceString(m.from, m.to) === 'note');

		expect(externalDocs?.class).toBe('cm-lp-link-external');
		expect(internalNote?.class).toBe('cm-lp-link-text');
	});
});
