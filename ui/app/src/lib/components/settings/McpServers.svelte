<script lang="ts">
	import { onMount } from 'svelte';
	import { createAgentClient } from '$lib/agent/agent-client';
	import type { McpConfigData, McpServerData } from '$lib/agent/types';

	const client = createAgentClient();

	let config = $state<McpConfigData>({ servers: [] });
	let saving = $state(false);
	let error = $state<string | null>(null);

	// Draft for the "Add MCP server" form.
	let draftId = $state('');
	let draftTransport = $state<'command' | 'url'>('command');
	let draftCommand = $state('');
	let draftArgs = $state('');
	let draftUrl = $state('');
	let draftDisplayName = $state('');

	onMount(() => {
		void reload();
	});

	async function reload() {
		try {
			config = await client.getMcpServers();
		} catch (e) {
			config = { servers: [] };
			error = errorText(e);
		}
	}

	function errorText(e: unknown): string {
		return e instanceof Error ? e.message : String(e);
	}

	/** Split a whitespace-separated args string into an args array. */
	function splitArgs(text: string): string[] {
		return text.trim().length === 0 ? [] : text.trim().split(/\s+/);
	}

	/** Persist the whole `[mcp]` section, then reload to reflect what was saved. */
	async function saveConfig() {
		saving = true;
		error = null;
		try {
			await client.setMcpServers(config);
			await reload();
		} catch (e) {
			error = errorText(e);
		} finally {
			saving = false;
		}
	}

	function setArgs(server: McpServerData, text: string) {
		server.args = splitArgs(text);
	}

	function addEnvRow(server: McpServerData) {
		server.env = [...server.env, ['', '']];
	}

	function removeEnvRow(server: McpServerData, index: number) {
		server.env = server.env.filter((_, i) => i !== index);
	}

	async function toggleEnabled(server: McpServerData, value: boolean) {
		server.enabled = value;
		await saveConfig();
	}

	async function removeServer(index: number) {
		config.servers = config.servers.filter((_, i) => i !== index);
		await saveConfig();
	}

	/** Whether a server uses the stdio (command) transport. */
	function isStdio(server: McpServerData): boolean {
		return Boolean(server.command && server.command.trim().length > 0);
	}

	async function addServer() {
		const id = draftId.trim();
		if (id.length === 0) {
			error = 'An MCP server needs an id.';
			return;
		}
		if (config.servers.some((s) => s.id === id)) {
			error = `A server with id "${id}" already exists.`;
			return;
		}
		const command = draftTransport === 'command' ? draftCommand.trim() : '';
		const url = draftTransport === 'url' ? draftUrl.trim() : '';
		if (command.length === 0 && url.length === 0) {
			error = 'Provide a command (stdio) or a URL (HTTP) for the server.';
			return;
		}
		error = null;
		const server: McpServerData = {
			id,
			command: command || null,
			args: draftTransport === 'command' ? splitArgs(draftArgs) : [],
			env: [],
			url: url || null,
			displayName: draftDisplayName.trim() || null,
			enabled: true
		};
		config.servers = [...config.servers, server];
		await saveConfig();
		if (!error) {
			draftId = '';
			draftCommand = '';
			draftArgs = '';
			draftUrl = '';
			draftDisplayName = '';
			draftTransport = 'command';
		}
	}
</script>

<div class="mcp-settings">
	{#if error}
		<div class="error-banner" role="alert">{error}</div>
	{/if}

	<!-- 1. Built-in vault tools (always present, non-removable) -->
	<section class="config-section">
		<h3 class="section-title">Built-in vault tools</h3>
		<p class="section-hint">
			Every chat session exposes the active vault's notes to the agent over the daemon's MCP
			endpoint. These tools are always available and cannot be removed; read-only vs read-write
			is controlled by the chat panel's scope toggle.
		</p>
		<ul class="server-list">
			<li class="server-row">
				<span class="server-name">Notesmith vault</span>
				<span class="badge badge-ok" data-testid="builtin-status">✓ always on</span>
			</li>
		</ul>
	</section>

	<!-- 2. External MCP servers -->
	<section class="config-section">
		<h3 class="section-title">External MCP servers</h3>
		<p class="section-hint">
			Add MCP servers the agent can use alongside the vault tools. Saved to your global
			<code>config.toml</code> <code>[mcp]</code> section and shared across vaults.
		</p>

		{#if config.servers.length > 0}
			{#each config.servers as server, i (server.id)}
				<div class="entry-card" data-testid="mcp-server">
					<div class="entry-head">
						<span class="entry-id">{server.displayName || server.id}</span>
						<span class="badge badge-kind">{isStdio(server) ? 'stdio' : 'http'}</span>
						<label class="entry-enabled">
							<input
								type="checkbox"
								checked={server.enabled}
								onchange={(e) => void toggleEnabled(server, e.currentTarget.checked)}
							/>
							<span>Enabled</span>
						</label>
					</div>
					{#if isStdio(server)}
						<label class="field">
							<span class="field-label">Command</span>
							<input
								type="text"
								placeholder="path or PATH-resolved program"
								value={server.command ?? ''}
								oninput={(e) => (server.command = e.currentTarget.value || null)}
							/>
						</label>
						<label class="field">
							<span class="field-label">Args (whitespace-separated)</span>
							<input
								type="text"
								placeholder="-y @modelcontextprotocol/server-filesystem"
								value={server.args.join(' ')}
								oninput={(e) => setArgs(server, e.currentTarget.value)}
							/>
						</label>
					{:else}
						<label class="field">
							<span class="field-label">URL</span>
							<input
								type="text"
								placeholder="https://tools.example.com/mcp"
								value={server.url ?? ''}
								oninput={(e) => (server.url = e.currentTarget.value || null)}
							/>
						</label>
					{/if}
					<label class="field">
						<span class="field-label">Display name</span>
						<input
							type="text"
							placeholder="optional"
							value={server.displayName ?? ''}
							oninput={(e) => (server.displayName = e.currentTarget.value || null)}
						/>
					</label>
					{#if isStdio(server)}
						<div class="env-block">
							<span class="field-label">Environment</span>
							{#each server.env as pair, ei}
								<div class="env-row">
									<input
										type="text"
										class="env-key"
										placeholder="KEY"
										value={pair[0]}
										oninput={(e) => (server.env[ei][0] = e.currentTarget.value)}
									/>
									<input
										type="text"
										class="env-val"
										placeholder="value"
										value={pair[1]}
										oninput={(e) => (server.env[ei][1] = e.currentTarget.value)}
									/>
									<button
										type="button"
										class="btn btn-ghost"
										onclick={() => removeEnvRow(server, ei)}
									>
										Remove
									</button>
								</div>
							{/each}
							<button type="button" class="btn btn-ghost" onclick={() => addEnvRow(server)}>
								Add variable
							</button>
						</div>
					{/if}
					<div class="row-actions">
						<button
							type="button"
							class="btn btn-primary"
							onclick={() => void saveConfig()}
							disabled={saving}
						>
							Save
						</button>
						<button type="button" class="btn btn-danger" onclick={() => void removeServer(i)}>
							Remove
						</button>
					</div>
				</div>
			{/each}
		{:else}
			<p class="empty">No external MCP servers configured.</p>
		{/if}

		<div class="entry-card add-card">
			<h4 class="add-title">Add MCP server</h4>
			<label class="field">
				<span class="field-label">Id</span>
				<input type="text" placeholder="filesystem" bind:value={draftId} />
			</label>
			<div class="field">
				<span class="field-label">Transport</span>
				<div class="transport-toggle">
					<label class="radio">
						<input type="radio" value="command" bind:group={draftTransport} />
						<span>Command (stdio)</span>
					</label>
					<label class="radio">
						<input type="radio" value="url" bind:group={draftTransport} />
						<span>URL (HTTP)</span>
					</label>
				</div>
			</div>
			{#if draftTransport === 'command'}
				<label class="field">
					<span class="field-label">Command</span>
					<input type="text" placeholder="npx" bind:value={draftCommand} />
				</label>
				<label class="field">
					<span class="field-label">Args (whitespace-separated)</span>
					<input
						type="text"
						placeholder="-y @modelcontextprotocol/server-filesystem ~/notes"
						bind:value={draftArgs}
					/>
				</label>
			{:else}
				<label class="field">
					<span class="field-label">URL</span>
					<input type="text" placeholder="https://tools.example.com/mcp" bind:value={draftUrl} />
				</label>
			{/if}
			<label class="field">
				<span class="field-label">Display name</span>
				<input type="text" placeholder="optional" bind:value={draftDisplayName} />
			</label>
			<div class="row-actions">
				<button
					type="button"
					class="btn btn-primary"
					onclick={() => void addServer()}
					disabled={saving}
				>
					Add server
				</button>
			</div>
		</div>
	</section>
</div>

<style>
	.mcp-settings {
		display: flex;
		flex-direction: column;
	}

	.config-section {
		padding: 1.25rem 0;
		border-bottom: 1px solid var(--border-default);
	}

	.config-section:last-child {
		border-bottom: none;
	}

	.section-title {
		margin: 0 0 0.35rem;
		font-size: 0.95rem;
		font-weight: 600;
		color: var(--text-default);
	}

	.section-hint {
		margin: 0 0 0.85rem;
		font-size: 0.82rem;
		color: var(--text-muted);
	}

	.error-banner {
		padding: 0.6rem 0.8rem;
		background: var(--danger-bg);
		color: var(--color-danger);
		border: 1px solid var(--danger-border);
		border-radius: 6px;
		font-size: 0.85rem;
	}

	.empty {
		font-size: 0.85rem;
		color: var(--text-muted);
	}

	.server-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.server-row {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding: 0.4rem 0;
		color: var(--text-default);
	}

	.server-name {
		font-weight: 500;
	}

	.badge {
		font-size: 0.72rem;
		padding: 0.1rem 0.45rem;
		border-radius: 4px;
	}

	.badge-ok {
		color: var(--color-success);
		background: var(--success-bg);
	}

	.badge-kind {
		color: var(--text-muted);
		background: var(--bg-secondary);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.row-actions {
		display: flex;
		gap: 0.5rem;
		margin-top: 0.75rem;
	}

	.btn {
		padding: 0.4rem 0.85rem;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--button-bg);
		color: var(--button-text);
		font-size: 0.82rem;
		cursor: pointer;
	}

	.btn:hover:not(:disabled) {
		background: var(--button-hover);
	}

	.btn:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.btn-primary {
		background: var(--accent-bg);
		color: var(--accent-text);
		border-color: var(--accent-bg);
	}

	.btn-danger {
		color: var(--color-danger);
		border-color: var(--danger-border);
	}

	.btn-ghost {
		background: transparent;
		color: var(--text-muted);
	}

	.entry-card {
		border: 1px solid var(--border-default);
		border-radius: 8px;
		padding: 0.9rem;
		margin-bottom: 0.85rem;
		background: var(--bg-secondary);
	}

	.add-card {
		margin-bottom: 0;
	}

	.add-title {
		margin: 0 0 0.6rem;
		font-size: 0.88rem;
		font-weight: 600;
		color: var(--text-default);
	}

	.entry-head {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		margin-bottom: 0.6rem;
	}

	.entry-id {
		font-weight: 600;
		color: var(--text-default);
	}

	.entry-enabled {
		display: flex;
		align-items: center;
		gap: 0.3rem;
		margin-left: auto;
		font-size: 0.8rem;
		color: var(--text-muted);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		margin-bottom: 0.6rem;
	}

	.field-label {
		font-size: 0.78rem;
		font-weight: 500;
		color: var(--text-muted);
	}

	.field input[type='text'] {
		padding: 0.4rem 0.55rem;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--input-bg);
		color: var(--text-default);
		font-size: 0.85rem;
	}

	.transport-toggle {
		display: flex;
		gap: 1rem;
	}

	.radio {
		display: flex;
		align-items: center;
		gap: 0.3rem;
		font-size: 0.82rem;
		color: var(--text-default);
	}

	.env-block {
		margin-bottom: 0.6rem;
	}

	.env-row {
		display: flex;
		gap: 0.4rem;
		margin: 0.35rem 0;
	}

	.env-key,
	.env-val {
		flex: 1;
		padding: 0.35rem 0.5rem;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--input-bg);
		color: var(--text-default);
		font-size: 0.82rem;
	}
</style>
