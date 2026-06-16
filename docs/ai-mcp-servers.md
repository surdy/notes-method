# AI Agent: MCP Servers

MCP (Model Context Protocol) is the interface through which the AI agent calls tools — reading notes, running searches, querying SQL views, or calling any external capability you hook up. Notesmith's daemon exposes the active vault as a built-in MCP server; the **Settings → MCP Servers** screen lets you add further external servers alongside it. For details on the daemon's own MCP endpoint, the stdio bridge, and the tool catalogue, see [MCP Adapter](mcp.md).

---

## Built-in vault tools

Every chat session automatically exposes the active vault's notes to the agent. These tools are always on — they do not appear in the external servers list and cannot be removed. In the **Settings → MCP Servers** screen you will see them listed under **Built-in vault tools** as **Notesmith vault** with a green **✓ always on** badge.

Whether the agent can write to your vault is controlled by the **scope toggle** in the chat panel, not here. Read-only mode limits the agent to vault read tools; read-write mode unlocks write tools. See [AI Chat](ai-chat.md) for the scope toggle.

> **Tip:** You never need to add the vault yourself — it is already there in every session.

---

## Opening the MCP Servers screen

Open the desktop app, then go to **Settings → MCP Servers**. The screen is divided into two sections:

- **Built-in vault tools** — the always-on vault entry described above.
- **External MCP servers** — the list of servers you have added, plus the **Add MCP server** form.

Changes are saved to your global `~/.config/notesmith/config.toml` and apply across all vaults.

---

## Adding an external server

Fill in the **Add MCP server** form at the bottom of the External MCP servers section:

| Field | Required | Description |
|-------|----------|-------------|
| **Id** | Yes | Stable identifier used internally and surfaced to the agent as the server name. Must be unique. |
| **Transport** | Yes | **Command (stdio)** or **URL (HTTP)** — choose one. |
| **Command** | stdio only | Program to launch (a PATH-resolved name or an absolute path). `~` and `$VAR` / `${VAR}` are expanded. |
| **Args** | stdio only | Space-separated arguments passed to the command. `~` and `$VAR` / `${VAR}` are expanded per element. |
| **URL** | HTTP only | Full `http://` or `https://` endpoint of a Streamable HTTP MCP server. |
| **Display name** | No | Human-readable label shown in the Settings list. Falls back to the id if left empty. |

Click **Add server** to save. The new entry appears in the list immediately.

### stdio (command) server

A stdio server is a local process Notesmith spawns on demand. Set **Transport** to **Command (stdio)**, then fill in **Command** and **Args**.

**Command** is the program to run — a PATH-resolved name like `npx` or an absolute path like `/usr/local/bin/my-server`. **Args** is a single text field whose content is split on whitespace into individual arguments. For example, entering `-y @modelcontextprotocol/server-filesystem ~/notes` in **Args** passes four separate arguments to the command.

`~` at the start of a value is expanded to `$HOME`. `$VAR` and `${VAR}` anywhere in **Command**, **Args**, or environment variable values are replaced with the corresponding process environment variable. Unknown variable names are left verbatim; expansion never panics on malformed input.

### HTTP(S) server

An HTTP server is a remote or local process reachable over the network. Set **Transport** to **URL (HTTP)** and enter the full endpoint URL (e.g. `https://tools.example.com/mcp`). Notesmith connects to it using the Streamable HTTP MCP protocol.

---

## Managing existing servers

Each saved server is shown as a card with its display name (or id) and a transport badge (**stdio** or **http**).

- **Enabled checkbox** — uncheck to disable the server without removing it. Disabled servers are skipped when sessions start.
- **Command / Args / URL / Display name** — edit any field directly in the card and click **Save**.
- **Environment** (stdio servers only) — click **Add variable** to add a `KEY` / `value` row; click **Remove** next to a row to delete it. Environment values support the same `~` / `$VAR` expansion as **Command** and **Args**.
- **Remove** — deletes the entry from the list and from `config.toml`.

---

## How servers map to `config.toml`

The MCP Servers screen reads and writes the `[mcp]` section of `~/.config/notesmith/config.toml`. Each server you add becomes one `[[mcp.servers]]` entry:

```toml
[[mcp.servers]]                        # stdio server
id = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "~/notes"]
display_name = "Files"
enabled = true

[[mcp.servers]]                        # HTTP server
id = "remote-tools"
url = "https://tools.example.com/mcp"
enabled = true
```

You can also edit this file by hand; changes are picked up the next time a session starts. See the [`[mcp]` section in Vault Configuration](vault-configuration.md#external-mcp-servers-mcp) for the full field reference. The server list is **global** — it applies to every vault.

Disabled entries (`enabled = false`) and entries with neither a `command` nor a `url` are silently skipped when the agent session is built.

---

## Walkthroughs

### 1. Add the filesystem MCP server (stdio)

The official `@modelcontextprotocol/server-filesystem` package gives the agent read/write access to a folder on disk via `npx`.

1. Open **Settings → MCP Servers**.
2. In the **Add MCP server** form, fill in:
   - **Id**: `filesystem`
   - **Transport**: **Command (stdio)**
   - **Command**: `npx`
   - **Args**: `-y @modelcontextprotocol/server-filesystem ~/notes`
   - **Display name**: `Files` (optional)
3. Click **Add server**.

The resulting `config.toml` entry:

```toml
[[mcp.servers]]
id = "filesystem"
command = "npx"
args = ["-y", "@modelcontextprotocol/server-filesystem", "~/notes"]
display_name = "Files"
enabled = true
```

The `~/notes` path is expanded to your home directory when the server is launched.

### 2. Add an HTTP MCP server by URL

If you have an MCP server running at a fixed URL (self-hosted or a cloud service):

1. Open **Settings → MCP Servers**.
2. In the **Add MCP server** form, fill in:
   - **Id**: `web-tools`
   - **Transport**: **URL (HTTP)**
   - **URL**: `https://tools.example.com/mcp`
   - **Display name**: `Web Tools` (optional)
3. Click **Add server**.

The resulting `config.toml` entry:

```toml
[[mcp.servers]]
id = "web-tools"
url = "https://tools.example.com/mcp"
display_name = "Web Tools"
enabled = true
```

Notesmith connects to the URL using the Streamable HTTP MCP protocol at the start of each agent session.

---

## Related docs

- [MCP Adapter](mcp.md) — daemon MCP endpoint, tools, resources, and the stdio bridge
- [AI Chat](ai-chat.md) — scope toggle (read-only vs read-write), chat panel usage
- [Vault Configuration](vault-configuration.md) — full `[mcp]` field reference and global config format
