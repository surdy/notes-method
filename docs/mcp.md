# Notesmith MCP Adapter

Notesmith exposes an MCP server over stdio via:

```bash
notesmith mcp start [--vault <name>]
```

The MCP adapter wraps the existing vault engine, SQLite cache, search index, routing engine, task toggling, daily note creation, and template instantiation.

## Tools

| Tool | Parameters |
|------|------------|
| `create_note` | `title`, `content?`, `folder?`, `frontmatter?` |
| `get_note` | `path` |
| `update_note` | `path`, `content` |
| `append_to_note` | `path`, `content` |
| `archive_note` | `path` |
| `search_notes` | `query`, `limit?` |
| `query_sql` | `sql` |
| `list_notes` | `type?`, `customer?`, `archived?` |
| `list_tasks` | `status?`, `customer?` |
| `update_task_status` | `note_path`, `task_hash`, `status` |
| `capture` | `content`, `title?` |
| `create_daily_note` | `date?` (`YYYY-MM-DD`) |
| `create_from_template` | `template_name`, `prompts?` |

## Resources

| Resource URI | Description |
|--------------|-------------|
| `note:///{vault-path}` | Read an individual note |
| `note:///daily/{date}` | Read a daily note by date |
| `note:///vault/structure` | List all note paths in the vault |

## Claude Desktop example

```json
{
  "mcpServers": {
    "notesmith": {
      "command": "notesmith",
      "args": ["mcp", "start", "--vault", "work"]
    }
  }
}
```
