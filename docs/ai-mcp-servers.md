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

> **GitHub Copilot rejects stdio servers.** Copilot's ACP mode refuses any stdio MCP server handed to it by an ACP client — it logs `Rejecting non-http/sse MCP server "<id>" from client` and never starts the process (verified on Copilot CLI `1.0.83-1`). So a stdio entry you add here reaches **Claude, Codex, Gemini, and OpenCode** sessions but is silently absent from **Copilot** sessions. A stdio-only tool such as Microsoft's `workiq mcp` is therefore usable from a Claude or Codex session but not from a Copilot one; for Copilot, use an HTTP entry (below), or the tool's own Copilot plugin if it ships one. (This is specific to servers supplied *by* an ACP client — Copilot does support stdio MCP servers configured in its own config.) Notesmith's built-in vault tools are unaffected: they are always offered over HTTP when the agent supports it.

### HTTP(S) server

An HTTP server is a remote or local process reachable over the network. Set **Transport** to **URL (HTTP)** and enter the full endpoint URL (e.g. `https://tools.example.com/mcp`).

The **agent process — not Notesmith — opens the connection**: the entry is handed to the spawned agent in its ACP session setup, and the agent then talks to the URL using the Streamable HTTP MCP protocol. Authentication is therefore enforced by the remote server against the agent's requests.

For an auth-protected server, add **request headers** to the entry (see [Managing existing servers](#managing-existing-servers)). Each header is a name/value pair the agent sends with every request — typically `Authorization` with a bearer credential. Header values support `$VAR` / `${VAR}` expansion when the session starts, so the value can be `Bearer $MY_TOKEN` and the secret stays in your environment instead of on disk.

---

## Managing existing servers

Each saved server is shown as a card with its display name (or id) and a transport badge (**stdio** or **http**).

- **Enabled checkbox** — uncheck to disable the server without removing it. Disabled servers are skipped when sessions start.
- **Command / Args / URL / Display name** — edit any field directly in the card and click **Save**.
- **Environment** (stdio servers only) — click **Add variable** to add a `KEY` / `value` row; click **Remove** next to a row to delete it. Environment values support the same `~` / `$VAR` expansion as **Command** and **Args**.
- **Headers** (HTTP servers only) — click **Add header** to add a header / value row sent with every request the agent makes to the server; click **Remove** next to a row to delete the header. Values support `$VAR` / `${VAR}` expansion. **Stored values are never displayed**: a saved header shows a blank value field with a "value stored" hint, and saving with the value left blank keeps the stored value — only typing a new value replaces it.
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

[mcp.servers.headers]                  # optional request headers (HTTP only)
Authorization = "Bearer $REMOTE_TOOLS_TOKEN"
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

At the start of each agent session the entry is handed to the agent, which connects to the URL using the Streamable HTTP MCP protocol.

### 3. Add an auth-protected remote server (e.g. Microsoft Work IQ)

A remote MCP server behind bearer authentication — such as Microsoft Work IQ's Entra-protected endpoint — needs an `Authorization` header on every request. Keep the token itself out of `config.toml` by referencing an environment variable:

1. Add the server with **Transport**: **URL (HTTP)** and the endpoint URL.
2. In the server card, click **Add header** and fill in:
   - **Header**: `Authorization`
   - **Value**: `Bearer $WORKIQ_TOKEN`
3. Click **Save**, and export `WORKIQ_TOKEN` in the environment Notesmith (or `notesmith ai`) runs in.

The resulting `config.toml` entry:

```toml
[[mcp.servers]]
id = "workiq"
url = "https://workiq.example.com/mcp"
display_name = "Work IQ"
enabled = true

[mcp.servers.headers]
Authorization = "Bearer $WORKIQ_TOKEN"
```

`$WORKIQ_TOKEN` is expanded from the environment when the session starts, and the agent sends the resolved header with every request. Headers are fixed for the lifetime of a session — if the token expires (Entra access tokens last about an hour), start a new session after refreshing it.

> **Headless runs too:** external servers configured here are attached to `notesmith ai` sessions as well as desktop chat, so scheduled jobs can reach the same auth-protected servers.

---

## Related docs

- [MCP Adapter](mcp.md) — daemon MCP endpoint, tools, resources, and the stdio bridge
- [AI Chat](ai-chat.md) — scope toggle (read-only vs read-write), chat panel usage
- [Vault Configuration](vault-configuration.md) — full `[mcp]` field reference and global config format
