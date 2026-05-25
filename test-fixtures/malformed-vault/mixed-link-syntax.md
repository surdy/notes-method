---
type: note
created: 2025-01-15
---

# Mixed Link Syntax Edge Cases

## Broken wikilinks
- Unclosed: [[this has no closing
- Empty: [[]]
- Only pipe: [[|]]
- Double pipe: [[target||display]]
- Nested brackets: [[outer [[inner]] link]]

## Broken embeds
- Unclosed: ![[embed without close
- Empty embed: ![[]]

## Inline fields edge cases
- [field:: ]
- [:: value without key]
- [key with spaces:: value]
- (field:: parenthetical)
- [field:: value with [[wikilink]] inside]
