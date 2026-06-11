# Notesmith MCP Adapter

Notesmith exposes an MCP server in two ways:

**1. Over stdio** (for stdio-only clients such as Claude Desktop):

```bash
notesmith mcp start [--vault <name>]
```

**2. Hosted by the daemon over HTTP/SSE** — when the daemon is running it mounts a streamable-HTTP MCP endpoint per vault, reusing the daemon's live indexes:

| Endpoint | Capabilities |
|----------|--------------|
| `POST/GET /mcp/{vault}` | Full read and write access |
| `POST/GET /mcp-ro/{vault}` | Read-only (write tools rejected) |

See [`docs/http-api.md`](http-api.md#agent-access-mcp-over-http) for connection details, reverse-proxy/TLS guidance, and the read-only model. Both transports expose the same tools and resources and share the same operation logic (`notesmith-ops`).

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
