# Command Palette

The unified command palette provides quick access to notes and commands from a single keyboard-driven interface.

## Opening the Palette

| Shortcut | Mode | Description |
|----------|------|-------------|
| `⌘P` | Files | Search and open notes |
| `⌘⇧P` | Commands | Search and run commands |
| `⌘K` | Commands | Alias for `⌘⇧P` |

## Modes

### File Mode (default)

Type to fuzzy-search notes by title or folder path.

- **Empty state:** Shows your 10 most recently viewed notes.
- **Search:** Matches against note title and full path (e.g., typing `kick` finds `Project Kickoff` in `Work/Projects/`).
- **Create:** When no exact match exists, a "Create '{query}'" option appears at the bottom. Selecting it creates a new note in your Inbox folder.
- **Open behavior:** Selected notes always open in a new tab.

### Command Mode (`>` prefix)

Type `>` followed by a search term to find commands.

- **Empty state:** Shows your most recently used commands.
- **Search:** Fuzzy matches on command names.
- **Execution:** Selecting a command runs it immediately.

You can switch modes inline by typing or deleting the `>` prefix.

## Keyboard Navigation

| Key | Action |
|-----|--------|
| `↑` / `↓` | Move selection |
| `Enter` | Open note / run command |
| `Escape` | Close palette |
| `>` | Switch to command mode (when typed at start) |

## Available Commands

Commands are grouped by category. The category and keyboard shortcut (if any) are shown on each row.

| Command | Category | Shortcut |
|---------|----------|----------|
| New Note | Notes | `⌘N` |
| Capture | Notes | `⌘⇧N` |
| Create Folder Note | Notes | — |
| Copy as HTML | Notes | — |
| Archive Current | Notes | `⌘⇧A` |
| Open Daily | Navigation | `⌘D` |
| New from Template | Templates | — |
| Reload Vault | Vault | — |
| Toggle View | Navigation | `⌘E` |
| Open Settings | Settings | `⌘,` |
