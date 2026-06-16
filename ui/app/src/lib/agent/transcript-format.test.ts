import { describe, it, expect } from 'vitest';
import { formatTranscriptMarkdown } from './transcript-format.ts';

describe('formatTranscriptMarkdown', () => {
	it('renders metadata frontmatter and role-labelled messages', () => {
		const md = formatTranscriptMarkdown(
			{
				title: 'Planning chat',
				agent: 'copilot',
				model: 'gpt-4',
				created_at: '2026-01-01T00:00:00Z',
				updated_at: '2026-01-02T00:00:00Z'
			},
			[
				{ role: 'user', content: 'How do I tag notes?' },
				{ role: 'agent', content: 'Use #tags in the body.' }
			]
		);

		expect(md).toContain('type: chat-transcript');
		expect(md).toContain('agent: "copilot"');
		expect(md).toContain('model: "gpt-4"');
		expect(md).toContain('created: "2026-01-01T00:00:00Z"');
		expect(md).toContain('updated: "2026-01-02T00:00:00Z"');
		expect(md).toContain('# Planning chat');
		expect(md).toContain('**User:**');
		expect(md).toContain('How do I tag notes?');
		expect(md).toContain('**Agent:**');
		expect(md).toContain('Use #tags in the body.');
		expect(md.endsWith('\n')).toBe(true);
	});

	it('omits absent metadata and falls back to a default title', () => {
		const md = formatTranscriptMarkdown({ title: '   ' }, [
			{ role: 'system', content: 'context' }
		]);
		expect(md).not.toContain('agent:');
		expect(md).not.toContain('model:');
		expect(md).toContain('# Chat transcript');
		expect(md).toContain('**System:**');
	});
});
