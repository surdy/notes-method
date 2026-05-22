import { describe, expect, it } from 'vitest';

import { highlightCodeToHtml, languageFromClassName, parseFencedCodeBlock } from './code-highlighting.ts';

describe('code highlighting', () => {
	it('highlights supported languages with token spans', async () => {
		const html = await highlightCodeToHtml('const answer: number = 42;', 'ts');

		expect(html).toContain('<span class="tok-keyword">const</span>');
		expect(html).toContain('<span class="tok-typeName">number</span>');
		expect(html).toContain('<span class="tok-number">42</span>');
	});

	it('escapes code text while preserving highlighted tokens', async () => {
		const html = await highlightCodeToHtml('const tag = "<script>";', 'ts');

		expect(html).toContain('&quot;&lt;script&gt;&quot;');
		expect(html).not.toContain('<script>');
	});

	it('falls back to escaped plain text for unsupported languages', async () => {
		const html = await highlightCodeToHtml('<unsafe>', 'not-a-language');

		expect(html).toBe('&lt;unsafe&gt;');
	});

	it('reads the language from comrak code element classes', () => {
		expect(languageFromClassName('language-rust')).toBe('rust');
		expect(languageFromClassName('foo language-ts bar')).toBe('ts');
		expect(languageFromClassName('')).toBeNull();
	});

	it('parses fenced code block info strings and code body', () => {
		expect(parseFencedCodeBlock('```ts title="demo"\nconst answer = 42;\n```')).toEqual({
			language: 'ts',
			code: 'const answer = 42;'
		});
	});
});
