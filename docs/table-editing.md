# Table Editing (Source Mode)

Notesmith provides keyboard-driven table editing in Source mode, inspired by the Obsidian Advanced Tables plugin. When your cursor is inside a markdown table, special keybindings activate for navigation, formatting, and structural operations.

> These features only work in **Source mode**. Live Preview has its own table widget with a right-click context menu.

---

## Auto-Formatting

Every navigation action (Tab, Enter, etc.) automatically reformats the table so columns align visually. Each column is padded to the width of its longest cell.

Before:
```
| Name|Role |
|---|---|
|Jane|CTO|
```

After pressing Tab:
```
| Name | Role |
| ---- | ---- |
| Jane | CTO  |
```

---

## Cell Navigation

| Key | Action |
|-----|--------|
| Tab | Move to the next cell. At the last cell of the last row, creates a new row. |
| Shift+Tab | Move to the previous cell. |
| Enter | Move to the first cell of the next row. Creates a new row if at the end. |
| Escape | Move the cursor below the table (exit table editing). |

Navigation wraps: Tab at the end of a row moves to the first cell of the next row. Shift+Tab at the start of a row moves to the last cell of the previous row.

---

## Table Bootstrap

You can create a table from scratch without manually typing the delimiter row:

1. Type `| Name | Role` and press **Tab**
2. Notesmith generates the full table structure:

```
| Name | Role |
| ---- | ---- |
|      |      |
```

3. Your cursor lands in the first body cell, ready to type.

This works with any number of headers. A single `| Heading` + Tab creates a one-column table.

---

## Structure Shortcuts

These shortcuts modify the table structure. They only fire when the cursor is inside a table.

| Shortcut | Action |
|----------|--------|
| ⌘⇧↑ | Move current row up |
| ⌘⇧↓ | Move current row down |
| ⌘⇧← | Move current column left |
| ⌘⇧→ | Move current column right |
| ⌘⇧Enter | Insert a new row below the current row |
| ⌘⇧Backspace | Delete the current row |
| ⌘⇧\\ | Insert a new column after the current column |
| ⌘⇧Delete | Delete the current column |

All structure operations auto-format the table and keep the cursor in the logical cell position.

### Boundary guards

- Move row up/down is a no-op at the first/last body row.
- Move column left/right is a no-op at the first/last column.
- Delete column is a no-op if only one column remains.
- Undo (⌘Z) reverts any structural operation.
