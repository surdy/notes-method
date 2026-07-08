import { describe, expect, it } from 'vitest';
import { renderMarkdown } from './markdown.ts';

describe('renderMarkdown', () => {
	it('renders bold and italic', () => {
		expect(renderMarkdown('My name is **Preeto**')).toBe('<p>My name is <strong>Preeto</strong></p>');
		expect(renderMarkdown('a *word* here')).toBe('<p>a <em>word</em> here</p>');
		expect(renderMarkdown('a _word_ here')).toBe('<p>a <em>word</em> here</p>');
		expect(renderMarkdown('a __strong__ here')).toBe('<p>a <strong>strong</strong> here</p>');
	});

	it('renders inline code without applying markup inside it', () => {
		expect(renderMarkdown('use `**not bold**` here')).toBe(
			'<p>use <code>**not bold**</code> here</p>'
		);
	});

	it('escapes HTML so agent output cannot inject markup', () => {
		expect(renderMarkdown('<script>alert(1)</script>')).toBe(
			'<p>&lt;script&gt;alert(1)&lt;/script&gt;</p>'
		);
		expect(renderMarkdown('a < b && c > d')).toBe('<p>a &lt; b &amp;&amp; c &gt; d</p>');
	});

	it('renders fenced code blocks with escaped content', () => {
		const out = renderMarkdown('```rust\nlet x = 1 < 2;\n```');
		expect(out).toBe('<pre><code class="language-rust">let x = 1 &lt; 2;</code></pre>');
	});

	it('renders unordered and ordered lists', () => {
		expect(renderMarkdown('- one\n- two')).toBe('<ul><li>one</li><li>two</li></ul>');
		expect(renderMarkdown('1. first\n2. second')).toBe('<ol><li>first</li><li>second</li></ol>');
	});

	it('renders wikilinks as clickable note links', () => {
		expect(renderMarkdown('see [[work/Zero-downtime Postgres cutover.md]]')).toBe(
			'<p>see <a class="agent-notelink" data-note-target="work/Zero-downtime Postgres cutover.md" role="link" tabindex="0">Zero-downtime Postgres cutover</a></p>'
		);
	});

	it('supports an explicit wikilink label', () => {
		expect(renderMarkdown('[[work/Foo.md|the plan]]')).toBe(
			'<p><a class="agent-notelink" data-note-target="work/Foo.md" role="link" tabindex="0">the plan</a></p>'
		);
	});

	it('escapes wikilink targets and labels', () => {
		const out = renderMarkdown('[[a"b/n.md|<x>]]');
		expect(out).toContain('data-note-target="a&quot;b/n.md"');
		expect(out).toContain('>&lt;x&gt;</a>');
		expect(out).not.toContain('<x>');
	});

	it('does not treat wikilinks inside inline code as note links', () => {
		expect(renderMarkdown('use `[[a.md]]` literally')).toBe(
			'<p>use <code>[[a.md]]</code> literally</p>'
		);
	});

	it('renders headings', () => {
		expect(renderMarkdown('## Title')).toBe('<h2>Title</h2>');
	});

	it('renders safe links and rejects dangerous schemes', () => {
		expect(renderMarkdown('see [docs](https://example.com)')).toBe(
			'<p>see <a href="https://example.com" target="_blank" rel="noopener noreferrer">docs</a></p>'
		);
		// javascript: URLs are not turned into anchors
		const out = renderMarkdown('[x](javascript:alert(1))');
		expect(out).not.toContain('href');
		expect(out).toContain('x');
	});

	it('keeps multi-line paragraphs with line breaks and separates blocks', () => {
		expect(renderMarkdown('line one\nline two')).toBe('<p>line one<br>line two</p>');
		expect(renderMarkdown('para one\n\npara two')).toBe('<p>para one</p><p>para two</p>');
	});

	it('degrades gracefully on unterminated markup (streaming partials)', () => {
		// An unterminated bold marker renders literally rather than throwing.
		expect(() => renderMarkdown('partial **bo')).not.toThrow();
		expect(renderMarkdown('partial **bo')).toBe('<p>partial **bo</p>');
	});

	it('returns an empty string for empty input', () => {
		expect(renderMarkdown('')).toBe('');
		expect(renderMarkdown('   \n  ')).toBe('');
	});
});
