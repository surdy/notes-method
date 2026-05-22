# Paste URL into Selection

When you paste while text is selected, Notesmith intelligently creates Markdown links.

## Behaviors

| Selection | Clipboard | Result |
|-----------|-----------|--------|
| Text | URL | `[selected text](url)` |
| URL | Text | `[clipboard text](selected url)` |
| Text | URL matching image whitelist | `![selected text](url)` |
| Text | `[[wikilink]]` | `[[wikilink\|selected text]]` |
| Cursor inside `[text]()` | URL | Fills the parentheses with the URL |

## Image URL Whitelist

Configure in Settings → Editor → Image URL Whitelist.

Each line is a regex pattern. When pasting a URL onto selected text, if the URL matches any pattern, an image embed (`![]()`) is produced instead of a regular link.

Example patterns:
```
youtu.?be|vimeo
imgur\.com
.*\.(?:png|jpg|gif|webp|svg)
```

If the whitelist is empty (default), image embed syntax is never used automatically.

**Note:** Image embed syntax is only applied when the clipboard contains the URL. If the selected text is a URL and you paste text over it, a regular link is always produced.
