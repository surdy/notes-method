<script lang="ts">
	import { onMount } from 'svelte';
	import { breakGlassStore } from '$lib/agent/break-glass.svelte';
	import { createAgentClient } from '$lib/agent/agent-client';
	import { formatDiagnostics, formatTimestamp, verdictLabel } from '$lib/agent/diagnostics-format';
	import type {
		AgentEntryData,
		AgentInfo,
		AgentsConfigData,
		DiagEntry,
		DiagnosticsReport
	} from '$lib/agent/types';

	const client = createAgentClient();

	let agents = $state<AgentInfo[]>([]);
	let config = $state<AgentsConfigData>({ debug: false, entries: [] });
	let diagnostics = $state<DiagnosticsReport | null>(null);

	let diagnosticsRunning = $state(false);
	let saving = $state(false);
	let error = $state<string | null>(null);
	let copied = $state(false);

	// Runtime diagnostics log (recent errors + optional ACP wire trace, issue 192).
	let diagLog = $state<DiagEntry[]>([]);
	let diagVerbose = $state(false);
	let diagLogLoading = $state(false);
	let expandedEntries = $state<Set<number>>(new Set());

	// Newest first for display, retaining each entry's original index as a
	// stable key for the expand/collapse state.
	const orderedLog = $derived(
		diagLog.map((entry, index) => ({ entry, index })).reverse()
	);

	// Draft for the "Add custom agent" form.
	let draftId = $state('');
	let draftCommand = $state('');
	let draftArgs = $state('');
	let draftDisplayName = $state('');

	onMount(() => {
		breakGlassStore.load();
		void reload();
	});

	async function reload() {
		await Promise.all([reloadAgents(), reloadConfig(), loadDiagLog()]);
	}

	async function reloadAgents() {
		try {
			agents = await client.listAgents();
		} catch (e) {
			agents = [];
			error = errorText(e);
		}
	}

	async function reloadConfig() {
		try {
			config = await client.getAgentConfig();
		} catch (e) {
			config = { debug: false, entries: [] };
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

	/** Persist the whole `[agents]` config, then refresh availability. */
	async function saveConfig() {
		saving = true;
		error = null;
		try {
			await client.setAgentConfig(config);
			await reloadAgents();
		} catch (e) {
			error = errorText(e);
		} finally {
			saving = false;
		}
	}

	async function toggleDebug(value: boolean) {
		config.debug = value;
		await saveConfig();
	}

	function setArgs(entry: AgentEntryData, text: string) {
		entry.args = splitArgs(text);
	}

	function addEnvRow(entry: AgentEntryData) {
		entry.env = [...entry.env, ['', '']];
	}

	function removeEnvRow(entry: AgentEntryData, index: number) {
		entry.env = entry.env.filter((_, i) => i !== index);
	}

	async function removeEntry(index: number) {
		config.entries = config.entries.filter((_, i) => i !== index);
		await saveConfig();
	}

	async function addAgent() {
		const id = draftId.trim();
		if (id.length === 0) {
			error = 'A custom agent needs an id.';
			return;
		}
		if (config.entries.some((e) => e.id === id)) {
			error = `An agent with id "${id}" already exists.`;
			return;
		}
		error = null;
		const entry: AgentEntryData = {
			id,
			command: draftCommand.trim() || null,
			args: splitArgs(draftArgs),
			env: [],
			displayName: draftDisplayName.trim() || null,
			enabled: true
		};
		config.entries = [...config.entries, entry];
		await saveConfig();
		if (!error) {
			draftId = '';
			draftCommand = '';
			draftArgs = '';
			draftDisplayName = '';
		}
	}

	async function runDiagnostics() {
		diagnosticsRunning = true;
		error = null;
		copied = false;
		try {
			diagnostics = await client.agentDiagnostics();
		} catch (e) {
			error = errorText(e);
		} finally {
			diagnosticsRunning = false;
		}
	}

	async function copyDiagnostics() {
		if (!diagnostics) return;
		const text = formatDiagnostics(diagnostics);
		try {
			await navigator.clipboard.writeText(text);
			copied = true;
		} catch {
			copied = false;
		}
	}

	/** Find the diagnostics setup hint for an agent id, if a run produced one. */
	function hintFor(id: string): string {
		const found = diagnostics?.agents.find((a) => a.id === id);
		return found?.setupHint ?? '';
	}

	/** Load the runtime diagnostics log snapshot (recent errors + wire entries). */
	async function loadDiagLog() {
		diagLogLoading = true;
		try {
			diagLog = await client.diagnosticsLog();
		} catch (e) {
			error = errorText(e);
		} finally {
			diagLogLoading = false;
		}
	}

	/** Toggle verbose ACP wire capture, then refresh the snapshot. */
	async function toggleDiagVerbose(value: boolean) {
		diagVerbose = value;
		try {
			await client.setDiagnosticsVerbose(value);
		} catch (e) {
			error = errorText(e);
		}
	}

	/** Clear the runtime diagnostics log (errors and wire entries). */
	async function clearDiagLog() {
		try {
			await client.clearDiagnosticsLog();
			diagLog = [];
			expandedEntries = new Set();
		} catch (e) {
			error = errorText(e);
		}
	}

	/** Expand/collapse an entry's detail by its stable index. */
	function toggleEntry(index: number) {
		const next = new Set(expandedEntries);
		if (next.has(index)) next.delete(index);
		else next.add(index);
		expandedEntries = next;
	}
</script>

<div class="agent-settings">
	{#if error}
		<div class="error-banner" role="alert">{error}</div>
	{/if}

	<!-- 1. Available agents -->
	<section class="config-section">
		<h3 class="section-title">Available agents</h3>
		<p class="section-hint">
			Agents detected on your PATH. Install a CLI or add a custom agent below to enable more.
		</p>
		{#if agents.length === 0}
			<p class="empty">No agents configured.</p>
		{:else}
			<ul class="agent-list">
				{#each agents as agent (agent.id)}
					<li class="agent-row">
						<span class="agent-name">{agent.name}</span>
						{#if agent.available}
							<span class="badge badge-ok">✓ available</span>
						{:else}
							<span class="badge badge-missing">✗ not found</span>
							<span class="agent-reason">
								{hintFor(agent.id) || '(not found on PATH)'}
							</span>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<!-- 2. Run diagnostics -->
	<section class="config-section">
		<h3 class="section-title">Diagnostics</h3>
		<p class="section-hint">
			Probe each agent and show how its CLI was discovered — the resolved PATH, the
			directories searched, and any version probe. Useful for bug reports.
		</p>
		<div class="row-actions">
			<button
				type="button"
				class="btn"
				onclick={() => void runDiagnostics()}
				disabled={diagnosticsRunning}
			>
				{diagnosticsRunning ? 'Running…' : 'Run diagnostics'}
			</button>
			{#if diagnostics}
				<button type="button" class="btn" onclick={() => void copyDiagnostics()}>
					{copied ? 'Copied' : 'Copy'}
				</button>
			{/if}
		</div>

		{#if diagnostics}
			<div class="trace" data-testid="diagnostics-trace">
				<div class="trace-block">
					<div class="trace-heading">Resolved PATH</div>
					{#if diagnostics.resolvedPath.length === 0}
						<div class="trace-line muted">(empty)</div>
					{:else}
						{#each diagnostics.resolvedPath as dir}
							<div class="trace-line">{dir}</div>
						{/each}
					{/if}
				</div>
				{#each diagnostics.agents as agent (agent.id)}
					<div class="trace-block">
						<div class="trace-heading">
							{agent.displayName}
							<span class="verdict verdict-{agent.verdict}">{verdictLabel(agent.verdict)}</span>
						</div>
						{#if agent.detectedVersion}
							<div class="trace-line muted">version: {agent.detectedVersion}</div>
						{/if}
						{#if agent.versionWarning}
							<div class="version-warning" role="alert">⚠ {agent.versionWarning}</div>
						{/if}
						{#each agent.candidates as candidate}
							<div class="trace-line">
								<strong>{candidate.program}</strong>
								{candidate.args.join(' ')} —
								{#if candidate.foundOnPath}
									found → {candidate.resolvedProgram}
								{:else}
									not found on PATH
								{/if}
							</div>
							{#if !candidate.foundOnPath && candidate.searchedDirs.length > 0}
								<div class="trace-line muted">
									searched: {candidate.searchedDirs.join(', ')}
								</div>
							{/if}
							{#if candidate.probe}
								<div class="trace-line muted">
									probe: {candidate.probe.command}
									({candidate.probe.timedOut
										? 'timed out'
										: `exit ${candidate.probe.exitCode ?? 'none'}`})
								</div>
								{#if candidate.probe.stdoutSnippet}
									<div class="trace-line muted">stdout: {candidate.probe.stdoutSnippet}</div>
								{/if}
							{/if}
						{/each}
					</div>
				{/each}
			</div>
		{/if}
	</section>

	<!-- 3. Recent errors / ACP wire log (issue 192) -->
	<section class="config-section">
		<h3 class="section-title">Recent errors &amp; wire log</h3>
		<p class="section-hint">
			Errors from recent agent sessions are always recorded. Turn on the verbose wire log to
			also capture the ACP messages Notesmith mediates — outgoing prompts, streamed events, and
			permission / filesystem requests. Useful for debugging a misbehaving agent.
		</p>
		<label class="field field-toggle field-toggle-stack">
			<span class="field-label">Verbose ACP wire log</span>
			<input
				type="checkbox"
				checked={diagVerbose}
				onchange={(e) => void toggleDiagVerbose(e.currentTarget.checked)}
			/>
			<span class="field-description">
				Captures a "wire-ish" log at Notesmith's ACP boundary (not the raw JSON-RPC bytes,
				which the protocol library owns). Note content is truncated. Leave off for the quiet
				default; errors are recorded either way.
			</span>
		</label>
		<div class="row-actions">
			<button
				type="button"
				class="btn"
				onclick={() => void loadDiagLog()}
				disabled={diagLogLoading}
			>
				{diagLogLoading ? 'Refreshing…' : 'Refresh'}
			</button>
			<button
				type="button"
				class="btn btn-danger"
				onclick={() => void clearDiagLog()}
				disabled={diagLog.length === 0}
			>
				Clear
			</button>
		</div>

		{#if diagLog.length === 0}
			<p class="empty">No diagnostics recorded yet.</p>
		{:else}
			<ul class="log-list" data-testid="diagnostics-log">
				{#each orderedLog as { entry, index } (index)}
					<li class="log-entry">
						<button
							type="button"
							class="log-summary"
							onclick={() => toggleEntry(index)}
							disabled={!entry.detail}
							aria-expanded={expandedEntries.has(index)}
						>
							<span class="log-time">{formatTimestamp(entry.timestampMs)}</span>
							<span class="log-kind log-kind-{entry.kind}">{entry.kind}</span>
							{#if entry.agent}
								<span class="log-agent">{entry.agent}</span>
							{/if}
							<span class="log-text">{entry.summary}</span>
							{#if entry.detail}
								<span class="log-caret">{expandedEntries.has(index) ? '▾' : '▸'}</span>
							{/if}
						</button>
						{#if entry.detail && expandedEntries.has(index)}
							<pre class="log-detail">{entry.detail}</pre>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</section>

	<!-- 4. Debug toggle -->
	<section class="config-section">
		<label class="field field-toggle field-toggle-stack">
			<span class="field-label">Verbose discovery logging</span>
			<input
				type="checkbox"
				checked={config.debug}
				onchange={(e) => void toggleDebug(e.currentTarget.checked)}
			/>
			<span class="field-description">
				When on, agent discovery records detailed diagnostics for the trace above. Leave off
				for the quiet, zero-overhead default.
			</span>
		</label>
	</section>

	<!-- 5. Custom agents / overrides -->
	<section class="config-section">
		<h3 class="section-title">Custom agents &amp; overrides</h3>
		<p class="section-hint">
			Point a built-in agent at a custom binary, or add a brand-new ACP agent. These are saved
			to your global <code>config.toml</code> <code>[agents]</code> section.
		</p>

		{#if config.entries.length > 0}
			{#each config.entries as entry, i (entry.id)}
				<div class="entry-card">
					<div class="entry-head">
						<span class="entry-id">{entry.id}</span>
						<label class="entry-enabled">
							<input type="checkbox" bind:checked={entry.enabled} />
							<span>Enabled</span>
						</label>
					</div>
					<label class="field">
						<span class="field-label">Command</span>
						<input
							type="text"
							placeholder="path or PATH-resolved program"
							value={entry.command ?? ''}
							oninput={(e) => (entry.command = e.currentTarget.value || null)}
						/>
					</label>
					<label class="field">
						<span class="field-label">Args (whitespace-separated)</span>
						<input
							type="text"
							placeholder="--acp"
							value={entry.args.join(' ')}
							oninput={(e) => setArgs(entry, e.currentTarget.value)}
						/>
					</label>
					<label class="field">
						<span class="field-label">Display name</span>
						<input
							type="text"
							placeholder="optional"
							value={entry.displayName ?? ''}
							oninput={(e) => (entry.displayName = e.currentTarget.value || null)}
						/>
					</label>
					<div class="env-block">
						<span class="field-label">Environment</span>
						{#each entry.env as pair, ei}
							<div class="env-row">
								<input
									type="text"
									class="env-key"
									placeholder="KEY"
									value={pair[0]}
									oninput={(e) => (entry.env[ei][0] = e.currentTarget.value)}
								/>
								<input
									type="text"
									class="env-val"
									placeholder="value"
									value={pair[1]}
									oninput={(e) => (entry.env[ei][1] = e.currentTarget.value)}
								/>
								<button type="button" class="btn btn-ghost" onclick={() => removeEnvRow(entry, ei)}>
									Remove
								</button>
							</div>
						{/each}
						<button type="button" class="btn btn-ghost" onclick={() => addEnvRow(entry)}>
							Add variable
						</button>
					</div>
					<div class="row-actions">
						<button
							type="button"
							class="btn btn-primary"
							onclick={() => void saveConfig()}
							disabled={saving}
						>
							Save
						</button>
						<button type="button" class="btn btn-danger" onclick={() => void removeEntry(i)}>
							Remove
						</button>
					</div>
				</div>
			{/each}
		{/if}

		<div class="entry-card add-card">
			<h4 class="add-title">Add custom agent</h4>
			<label class="field">
				<span class="field-label">Id</span>
				<input type="text" placeholder="my-agent" bind:value={draftId} />
			</label>
			<label class="field">
				<span class="field-label">Command</span>
				<input type="text" placeholder="node" bind:value={draftCommand} />
			</label>
			<label class="field">
				<span class="field-label">Args (whitespace-separated)</span>
				<input type="text" placeholder="index.js --acp" bind:value={draftArgs} />
			</label>
			<label class="field">
				<span class="field-label">Display name</span>
				<input type="text" placeholder="optional" bind:value={draftDisplayName} />
			</label>
			<div class="row-actions">
				<button
					type="button"
					class="btn btn-primary"
					onclick={() => void addAgent()}
					disabled={saving}
				>
					Add custom agent
				</button>
			</div>
		</div>
	</section>

	<!-- 6. Break-glass -->
	<section class="config-section">
		<h3 class="section-title">Security</h3>
		<label class="field field-toggle field-toggle-stack">
			<span class="field-label">Allow filesystem &amp; terminal access (break-glass)</span>
			<input
				type="checkbox"
				checked={breakGlassStore.enabled}
				onchange={(e) => breakGlassStore.set(e.currentTarget.checked)}
			/>
			<span class="field-description">
				When off (default), agents can only reach your vault through Notesmith's vetted
				operations. When on, agents may additionally request raw filesystem and terminal
				access, scoped to the active vault. Every write is still permission-gated and is
				blocked entirely in read-only mode. Leave this off unless you trust the agent.
			</span>
		</label>
	</section>
</div>

<style>
	.agent-settings {
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

	.error-banner {
		padding: 8px 12px;
		margin-bottom: 16px;
		background: var(--danger-bg);
		color: var(--color-danger);
		border: 1px solid var(--danger-border);
		border-radius: 4px;
		font-size: 12px;
	}

	.empty {
		font-size: 12px;
		color: var(--text-muted);
	}

	.agent-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.agent-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 0;
		font-size: 13px;
		color: var(--text-default);
	}

	.agent-name {
		font-weight: 500;
	}

	.badge {
		font-size: 11px;
		padding: 1px 6px;
		border-radius: 10px;
	}

	.badge-ok {
		color: var(--color-success);
		background: var(--success-bg);
	}

	.badge-missing {
		color: var(--color-danger);
		background: var(--danger-bg);
	}

	.agent-reason {
		font-size: 11px;
		color: var(--text-muted);
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

	.trace {
		margin-top: 12px;
		padding: 10px 12px;
		background: var(--bg-secondary);
		border: 1px solid var(--border-default);
		border-radius: 4px;
		font-family: var(--font-mono);
		font-size: 11px;
		line-height: 1.5;
		color: var(--text-default);
		overflow-x: auto;
	}

	.trace-block {
		margin-bottom: 10px;
	}

	.trace-block:last-child {
		margin-bottom: 0;
	}

	.trace-heading {
		font-weight: 600;
		margin-bottom: 2px;
	}

	.trace-line {
		white-space: pre-wrap;
		word-break: break-all;
	}

	.trace-line.muted {
		color: var(--text-muted);
	}

	.verdict {
		font-size: 10px;
		padding: 0 5px;
		border-radius: 8px;
		margin-left: 6px;
	}

	.verdict-available {
		color: var(--color-success);
		background: var(--success-bg);
	}

	.verdict-not_found {
		color: var(--color-danger);
		background: var(--danger-bg);
	}

	.verdict-probe_failed {
		color: var(--color-warning);
		background: var(--warning-bg);
	}

	.version-warning {
		margin-top: 4px;
		padding: 4px 8px;
		color: var(--color-warning);
		background: var(--warning-bg);
		border: 1px solid var(--border-default);
		border-radius: 4px;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.log-list {
		list-style: none;
		margin: 12px 0 0;
		padding: 0;
		border: 1px solid var(--border-default);
		border-radius: 4px;
		background: var(--bg-secondary);
		overflow: hidden;
	}

	.log-entry {
		border-bottom: 1px solid var(--border-default);
	}

	.log-entry:last-child {
		border-bottom: none;
	}

	.log-summary {
		display: flex;
		align-items: baseline;
		gap: 8px;
		width: 100%;
		padding: 6px 10px;
		border: none;
		background: transparent;
		color: var(--text-default);
		font-family: var(--font-mono);
		font-size: 11px;
		text-align: left;
		cursor: pointer;
	}

	.log-summary:hover:not(:disabled) {
		background: var(--button-hover);
	}

	.log-summary:disabled {
		cursor: default;
	}

	.log-time {
		color: var(--text-muted);
		flex: 0 0 auto;
	}

	.log-kind {
		flex: 0 0 auto;
		padding: 0 5px;
		border-radius: 8px;
		font-size: 10px;
		text-transform: uppercase;
	}

	.log-kind-error {
		color: var(--color-danger);
		background: var(--danger-bg);
	}

	.log-kind-wire {
		color: var(--text-muted);
		background: var(--bg-surface);
	}

	.log-agent {
		flex: 0 0 auto;
		color: var(--text-muted);
	}

	.log-text {
		flex: 1 1 auto;
		word-break: break-word;
	}

	.log-caret {
		flex: 0 0 auto;
		color: var(--text-muted);
	}

	.log-detail {
		margin: 0;
		padding: 6px 10px 10px;
		color: var(--text-muted);
		font-family: var(--font-mono);
		font-size: 11px;
		white-space: pre-wrap;
		word-break: break-word;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 12px;
	}

	.field-toggle {
		flex-direction: row;
		align-items: center;
		gap: 10px;
	}

	.field-toggle input[type='checkbox'] {
		order: -1;
		width: 16px;
		height: 16px;
		accent-color: var(--accent-bg);
	}

	.field-toggle .field-label {
		order: 1;
	}

	.field-toggle-stack {
		align-items: flex-start;
	}

	.field-toggle-stack .field-description {
		order: 2;
		margin-left: 26px;
	}

	.field-label {
		font-size: 12px;
		color: var(--text-muted);
	}

	.field-description {
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.4;
		max-width: 460px;
	}

	.field input[type='text'] {
		padding: 5px 8px;
		font-size: 12px;
		border: 1px solid var(--border-input);
		border-radius: 4px;
		background: var(--bg-input);
		color: var(--text-default);
	}

	.entry-card {
		padding: 12px;
		margin-bottom: 12px;
		border: 1px solid var(--border-default);
		border-radius: 6px;
		background: var(--bg-surface);
	}

	.entry-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 10px;
	}

	.entry-id {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-default);
		font-family: var(--font-mono);
	}

	.entry-enabled {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}

	.entry-enabled input[type='checkbox'] {
		width: 14px;
		height: 14px;
		accent-color: var(--accent-bg);
	}

	.env-block {
		display: flex;
		flex-direction: column;
		gap: 6px;
		margin-bottom: 12px;
	}

	.env-row {
		display: flex;
		gap: 6px;
		align-items: center;
	}

	.env-key {
		flex: 0 0 35%;
	}

	.env-val {
		flex: 1;
	}

	.add-card {
		background: var(--bg-secondary);
	}

	.add-title {
		margin: 0 0 10px;
		font-size: 12px;
		font-weight: 600;
		color: var(--text-default);
	}

	code {
		font-family: var(--font-mono);
		font-size: 0.9em;
		color: var(--text-default);
	}
</style>
