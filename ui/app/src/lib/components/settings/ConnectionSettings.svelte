<script lang="ts">
	import { onMount } from 'svelte';
	import {
		createConnectionClient,
		describeTestResult,
		validateConnectionForm,
		LOCAL_ID,
		type ConnectionList,
		type ConnectionTestResult,
		type ServerView
	} from '$lib/connection/connection-client';

	const client = createConnectionClient();

	let available = $state(client.available());
	let list = $state<ConnectionList>({ active_id: LOCAL_ID, servers: [] });
	let error = $state<string | null>(null);
	let busy = $state(false);

	// Add/edit form state. `editingId === null` while adding; a server id while editing.
	let formOpen = $state(false);
	let editingId = $state<string | null>(null);
	let draftName = $state('');
	let draftUrl = $state('');
	let draftToken = $state('');
	let formError = $state<string | null>(null);

	// Inline test feedback, keyed to the open form.
	let testing = $state(false);
	let testResult = $state<ConnectionTestResult | null>(null);

	let editingServer = $derived(
		editingId ? (list.servers.find((s) => s.id === editingId) ?? null) : null
	);

	onMount(() => {
		if (!available) return;
		void reload();
		return client.onChanged((next) => {
			list = next;
		});
	});

	function errorText(e: unknown): string {
		return e instanceof Error ? e.message : String(e);
	}

	async function reload() {
		try {
			list = await client.list();
			error = null;
		} catch (e) {
			error = errorText(e);
		}
	}

	function openAdd() {
		editingId = null;
		draftName = '';
		draftUrl = '';
		draftToken = '';
		formError = null;
		testResult = null;
		formOpen = true;
	}

	function openEdit(server: ServerView) {
		editingId = server.id;
		draftName = server.name;
		draftUrl = server.url;
		draftToken = '';
		formError = null;
		testResult = null;
		formOpen = true;
	}

	function closeForm() {
		formOpen = false;
		editingId = null;
		formError = null;
		testResult = null;
	}

	async function runTest() {
		formError = validateConnectionForm({ name: draftName || 'probe', url: draftUrl });
		if (formError) return;
		testing = true;
		testResult = null;
		try {
			testResult = await client.test(draftUrl.trim(), draftToken.trim() || null);
		} catch (e) {
			testResult = { reachable: false, error: errorText(e) };
		} finally {
			testing = false;
		}
	}

	async function save() {
		formError = validateConnectionForm({ name: draftName, url: draftUrl });
		if (formError) return;
		busy = true;
		try {
			if (editingId) {
				// A blank token field leaves the stored credential untouched.
				await client.update(editingId, {
					name: draftName.trim(),
					url: draftUrl.trim(),
					token: draftToken.trim().length > 0 ? draftToken.trim() : null
				});
			} else {
				await client.add({
					name: draftName.trim(),
					url: draftUrl.trim(),
					token: draftToken.trim() || null
				});
			}
			await reload();
			closeForm();
		} catch (e) {
			formError = errorText(e);
		} finally {
			busy = false;
		}
	}

	async function remove(server: ServerView) {
		busy = true;
		try {
			await client.remove(server.id);
			await reload();
			if (editingId === server.id) closeForm();
		} catch (e) {
			error = errorText(e);
		} finally {
			busy = false;
		}
	}

	function isActive(id: string): boolean {
		return list.active_id === id;
	}
</script>

<div class="connection-settings">
	{#if !available}
		<p class="empty">
			Connections are managed in the Notesmith desktop app. This panel is only available there.
		</p>
	{:else}
		{#if error}
			<div class="error-banner" role="alert">{error}</div>
		{/if}

		<section class="config-section">
			<h3 class="section-title">Servers</h3>
			<p class="section-hint">
				Notesmith runs locally by default. Add a remote server to open a vault hosted on
				another machine in its own window; each window shows its connection in the status bar.
			</p>

			<ul class="server-list">
				<li class="server-row">
					<div class="server-main">
						<span class="server-name">This Mac</span>
						<span class="server-url">Local daemon</span>
					</div>
					{#if isActive(LOCAL_ID)}
						<span class="badge badge-active">Active</span>
					{/if}
				</li>

				{#each list.servers as server (server.id)}
					<li class="server-row">
						<div class="server-main">
							<span class="server-name">{server.name}</span>
							<span class="server-url">{server.url}</span>
						</div>
						{#if server.has_token}
							<span class="badge badge-token" title="An access token is stored">token</span>
						{/if}
						{#if isActive(server.id)}
							<span class="badge badge-active">Active</span>
						{/if}
						<button type="button" class="btn btn-ghost" onclick={() => openEdit(server)}>
							Edit
						</button>
					</li>
				{/each}
			</ul>

			{#if !formOpen}
				<div class="row-actions">
					<button type="button" class="btn btn-primary" onclick={openAdd} disabled={busy}>
						Add server
					</button>
				</div>
			{/if}
		</section>

		{#if formOpen}
			<section class="config-section form-card">
				<h4 class="form-title">{editingId ? 'Edit server' : 'Add server'}</h4>

				<label class="field">
					<span class="field-label">Name</span>
					<input type="text" placeholder="Home server" bind:value={draftName} />
				</label>

				<label class="field">
					<span class="field-label">Server URL</span>
					<input type="text" placeholder="https://notes.example.com" bind:value={draftUrl} />
				</label>

				<label class="field">
					<span class="field-label">Access token (optional)</span>
					<input
						type="password"
						placeholder={editingServer?.has_token
							? 'Leave blank to keep the saved token'
							: 'Paste a token to authenticate'}
						bind:value={draftToken}
					/>
					{#if editingServer?.has_token}
						<span class="field-description">A token is currently set. Clear the field and save to remove it.</span>
					{/if}
				</label>

				{#if formError}
					<div class="form-error" role="alert">{formError}</div>
				{/if}

				{#if testResult}
					<div
						class="test-result"
						class:test-ok={testResult.reachable}
						class:test-fail={!testResult.reachable}
					>
						{describeTestResult(testResult)}
					</div>
				{/if}

				<div class="row-actions">
					<button type="button" class="btn" onclick={() => void runTest()} disabled={testing || busy}>
						{testing ? 'Testing…' : 'Test'}
					</button>
					<button type="button" class="btn btn-primary" onclick={() => void save()} disabled={busy}>
						{busy ? 'Saving…' : 'Save'}
					</button>
					{#if editingServer}
						<button
							type="button"
							class="btn btn-danger"
							onclick={() => void remove(editingServer)}
							disabled={busy}
						>
							Remove
						</button>
					{/if}
					<button type="button" class="btn btn-ghost" onclick={closeForm} disabled={busy}>
						Cancel
					</button>
				</div>
			</section>
		{/if}
	{/if}
</div>

<style>
	.connection-settings {
		padding: 16px 24px;
		max-width: 640px;
	}

	.config-section {
		padding-bottom: 20px;
		margin-bottom: 20px;
		border-bottom: 1px solid var(--border-default);
	}

	.config-section:last-child {
		border-bottom: none;
		margin-bottom: 0;
	}

	.section-title {
		margin: 0 0 4px;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-default);
	}

	.section-hint {
		margin: 0 0 12px;
		font-size: 11px;
		line-height: 1.4;
		color: var(--text-muted);
	}

	.empty {
		font-size: 13px;
		color: var(--text-muted);
	}

	.error-banner {
		padding: 8px 12px;
		margin-bottom: 16px;
		background: var(--danger-bg);
		color: var(--color-danger);
		border: 1px solid var(--danger-border);
		border-radius: 4px;
		font-size: 12px;
	}

	.server-list {
		list-style: none;
		margin: 0 0 12px;
		padding: 0;
	}

	.server-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 0;
		border-bottom: 1px solid var(--border-subtle);
	}

	.server-row:last-child {
		border-bottom: none;
	}

	.server-main {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.server-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-default);
	}

	.server-url {
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.badge {
		font-size: 11px;
		padding: 1px 8px;
		border-radius: 10px;
		flex-shrink: 0;
	}

	.badge-active {
		color: var(--color-success);
		background: var(--success-bg);
	}

	.badge-token {
		color: var(--text-muted);
		background: var(--bg-secondary);
		font-family: var(--font-mono);
	}

	.row-actions {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}

	.btn {
		padding: 5px 12px;
		font-size: 12px;
		border: 1px solid var(--border-default);
		border-radius: 4px;
		background: var(--button-bg);
		color: var(--button-text);
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
		background: transparent;
	}

	.btn-ghost {
		background: transparent;
		color: var(--text-muted);
	}

	.form-card {
		padding: 16px;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--bg-surface);
	}

	.form-title {
		margin: 0 0 12px;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-default);
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 12px;
	}

	.field-label {
		font-size: 12px;
		color: var(--text-muted);
	}

	.field-description {
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.4;
	}

	.field input {
		padding: 5px 8px;
		font-size: 12px;
		border: 1px solid var(--border-input);
		border-radius: 4px;
		background: var(--bg-input);
		color: var(--text-default);
	}

	.form-error {
		padding: 6px 10px;
		margin-bottom: 12px;
		background: var(--danger-bg);
		color: var(--color-danger);
		border-radius: 4px;
		font-size: 12px;
	}

	.test-result {
		padding: 6px 10px;
		margin-bottom: 12px;
		border-radius: 4px;
		font-size: 12px;
	}

	.test-ok {
		color: var(--color-success);
		background: var(--success-bg);
	}

	.test-fail {
		color: var(--color-danger);
		background: var(--danger-bg);
	}
</style>
