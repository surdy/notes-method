import type { Heading } from '$lib/heading-store.svelte';

const HEADING_RE = /^(#{1,6})\s+(.+)/;

export function parseHeadings(doc: string): Heading[] {
	const headings: Heading[] = [];
	let offset = 0;

	for (const line of doc.split('\n')) {
		const match = line.match(HEADING_RE);
		if (match) {
			headings.push({
				level: match[1].length,
				text: match[2].trim(),
				from: offset
			});
		}

		offset += line.length + 1;
	}

	return headings;
}

export function findActiveHeadingIndex(headings: Heading[], cursorPos: number): number {
	for (let index = headings.length - 1; index >= 0; index -= 1) {
		if (headings[index].from <= cursorPos) {
			return index;
		}
	}

	return -1;
}
