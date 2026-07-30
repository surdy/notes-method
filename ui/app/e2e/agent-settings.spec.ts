import { test, expect, type Page } from '@playwright/test';

/**
 * Headless end-to-end flow for the AI Agent settings surface (ADR 0013, Phase 5).
 *
 * Drives the real {@link AgentSettings} mounted at `/app/agent-settings-harness`.
 * The Tauri IPC bridge (`window.__TAURI__`) is stubbed here exactly like
 * `agent-chat.spec.ts` so the Svelte component runs as it does in the desktop
 * shell. The flow asserts the agents list + break-glass toggle + debug toggle +
 * "Run diagnostics" button render, that clicking "Run diagnostics" renders a
 * trace, and that adding a custom agent calls `agent_config_set`.
 */

async function installFakeBridge(page: Page): Promise<void> {
	await page.addInitScript(() => {
		const w = window as unknown as {
			__testState: { calls: Record<string, number>; lastSetConfig: unknown };
		};
		w.__testState = {
			calls: { agent_list: 0, agent_config_get: 0, agent_config_set: 0, agent_diagnostics: 0 },
			lastSetConfig: null
		};

		async function invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
			const s = w.__testState;
			switch (cmd) {
				case 'agent_list':
					s.calls.agent_list += 1;
					return [
						{ id: 'copilot', name: 'GitHub Copilot', available: true },
						{ id: 'gemini', name: 'Gemini', available: false }
					];
				case 'agent_config_get':
					s.calls.agent_config_get += 1;
					return { debug: false, entries: [] };
				case 'agent_config_set':
					s.calls.agent_config_set += 1;
					s.lastSetConfig = args.config;
					return null;
				// AgentSettings loads the runtime diagnostics log on mount and again
				// after a run. Without this case the `default: null` below lands in
				// `diagLog`, and the `diagLog.length` read tears down the render.
				case 'agent_diagnostics_log':
					return [];
				case 'agent_diagnostics':
					s.calls.agent_diagnostics += 1;
					return {
						resolvedPath: ['/opt/homebrew/bin', '/usr/bin'],
						agents: [
							{
								id: 'copilot',
								displayName: 'GitHub Copilot',
								verdict: 'available',
								setupHint: 'Install the Copilot CLI',
								docsUrl: 'https://example.com/copilot',
								candidates: [
									{
										program: 'copilot',
										args: ['--acp'],
										resolvedProgram: '/opt/homebrew/bin/copilot',
										foundOnPath: true,
										searchedDirs: ['/opt/homebrew/bin'],
										probe: {
											command: '/opt/homebrew/bin/copilot --version',
											exitCode: 0,
											stdoutSnippet: 'copilot 1.2.3',
											timedOut: false
										}
									}
								]
							},
							{
								id: 'gemini',
								displayName: 'Gemini',
								verdict: 'not_found',
								setupHint: 'Install the Gemini CLI',
								docsUrl: 'https://example.com/gemini',
								candidates: [
									{
										program: 'gemini',
										args: ['--experimental-acp'],
										resolvedProgram: null,
										foundOnPath: false,
										searchedDirs: ['/opt/homebrew/bin', '/usr/bin'],
										probe: null
									}
								]
							}
						]
					};
				default:
					return null;
			}
		}

		async function listen(): Promise<() => void> {
			return () => {};
		}

		(window as unknown as { __TAURI__: unknown }).__TAURI__ = {
			core: { invoke },
			event: { listen }
		};
	});
}

test('renders the AI agent surface, runs diagnostics, and saves a custom agent', async ({
	page
}) => {
	await installFakeBridge(page);
	await page.goto('/app/agent-settings-harness');

	// 1. Available agents list populates from agent_list with availability badges.
	await expect(page.getByText('GitHub Copilot').first()).toBeVisible();
	await expect(page.getByText('✓ available')).toBeVisible();
	await expect(page.getByText('✗ not found')).toBeVisible();

	// Break-glass + debug toggles render.
	await expect(
		page.getByText('Allow filesystem & terminal access (break-glass)')
	).toBeVisible();
	await expect(page.getByText('Verbose discovery logging')).toBeVisible();

	// 2. Run diagnostics renders the trace.
	const runBtn = page.getByRole('button', { name: 'Run diagnostics' });
	await expect(runBtn).toBeVisible();
	await runBtn.click();

	const trace = page.getByTestId('diagnostics-trace');
	await expect(trace).toBeVisible();
	await expect(trace).toContainText('Resolved PATH');
	await expect(trace).toContainText('/opt/homebrew/bin');
	await expect(trace).toContainText('/opt/homebrew/bin/copilot --version');
	await expect(trace).toContainText('not found on PATH');

	// 4. Add a custom agent → agent_config_set is called with the new entry.
	await page.getByPlaceholder('my-agent').fill('my-agent');
	await page.getByPlaceholder('node').fill('node');
	await page.getByPlaceholder('index.js --acp').fill('index.js --acp');
	await page.getByRole('button', { name: 'Add custom agent' }).click();

	// The new entry card appears.
	await expect(page.getByText('my-agent').first()).toBeVisible();

	const recorded = await page.evaluate(
		() =>
			(
				window as unknown as {
					__testState: { calls: Record<string, number>; lastSetConfig: unknown };
				}
			).__testState
	);
	expect(recorded.calls.agent_config_set).toBe(1);
	expect(recorded.calls.agent_diagnostics).toBe(1);
	expect(recorded.lastSetConfig).toMatchObject({
		debug: false,
		entries: [{ id: 'my-agent', command: 'node', args: ['index.js', '--acp'], enabled: true }]
	});
});
