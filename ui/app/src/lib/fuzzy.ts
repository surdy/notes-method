export interface FuzzyMatch<T> {
item: T;
score: number;
highlights: number[];
}

const SEPARATOR_PATTERN = /[\s/._-]/;

export function fuzzyMatch(query: string, text: string): { score: number; highlights: number[] } | null {
const normalizedQuery = query.trim().toLowerCase();
if (!normalizedQuery) {
return { score: 0, highlights: [] };
}

const normalizedText = text.toLowerCase();
const highlights: number[] = [];
let searchStart = 0;
let previousIndex = -1;
let score = 0;

for (const character of normalizedQuery) {
const index = normalizedText.indexOf(character, searchStart);
if (index === -1) {
return null;
}

highlights.push(index);
score += 1;

if (index === 0) {
score += 20;
}

if (index > 0 && SEPARATOR_PATTERN.test(text[index - 1] ?? '')) {
score += 14;
}

if (previousIndex >= 0) {
const gap = index - previousIndex - 1;
if (gap === 0) {
score += 18;
} else {
score -= Math.min(gap, 8);
}
}

previousIndex = index;
searchStart = index + 1;
}

const span = highlights[highlights.length - 1] - highlights[0];
score += Math.max(0, 8 - span);
score -= Math.max(0, text.length - normalizedQuery.length) * 0.05;

return { score, highlights };
}

export function fuzzyFilter<T>(query: string, items: T[], getText: (item: T) => string): FuzzyMatch<T>[] {
const normalizedQuery = query.trim();

return items
.map((item) => {
const text = getText(item);
const match = fuzzyMatch(normalizedQuery, text);
if (!match) {
return null;
}

return {
item,
score: match.score,
highlights: match.highlights
};
})
.filter((match): match is FuzzyMatch<T> => match !== null)
.sort((left, right) => {
if (right.score !== left.score) {
return right.score - left.score;
}

return getText(left.item).localeCompare(getText(right.item));
});
}
