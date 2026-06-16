import { test, expect, type Page } from '@playwright/test';

/**
 * Headless end-to-end flow for the MCP server management surface (ADR 0016, #211).
 *
 * Drives the real {@link McpServers} mounted at `/app/mcp-servers-harness`. The
 * Tauri IPC bridge (`window.__TAURI__`) is stubbed here exactly like
 * `agent-settings.spec.ts` so the Svelte component runs as it does in the
 * desktop shell. The flow asserts the built-in vault tools render as a static
 * "always on" entry, that the configured servers load, that toggling enabled
 * calls `mcp_servers_set`, and that adding a new server persists it.
 */

interface TestState {
	calls: Record<string, number>;
	lastSetConfig: unknown;
}

async function installFakeBridge(page: Page): Promise<void> {
	await page.addInitScript(() => {
		const w = window as unknown as { __testState: TestState };
		w.__testState = {
			calls: { mcp_servers_get: 0, mcp_servers_set: 0 },
			lastSetConfig: null
		};

		// A mutable in-memory store so get reflects the last set.
		let stored = {
			servers: [
				{
					id: 'filesystem',
					command: 'npx',
					args: ['-y', 'server-fs'],
					env: [],
					url: null,
					displayName: 'Files',
					enabled: true
				}
			]
		};

		async function invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
			const s = w.__testState;
			switch (cmd) {
				case 'mcp_servers_get':
					s.calls.mcp_servers_get += 1;
					return stored;
				case 'mcp_servers_set':
					s.calls.mcp_servers_set += 1;
					s.lastSetConfig = args.config;
					stored = args.config as typeof stored;
					return null;
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

function readState(page: Page): Promise<TestState> {
	return page.evaluate(() => (window as unknown as { __testState: TestState }).__testState);
}

test('renders MCP servers, toggles a server, and adds a new one', async ({ page }) => {
	await installFakeBridge(page);
	await page.goto('/app/mcp-servers-harness');

	// 1. Built-in vault tools render as a static, always-on entry.
	await expect(page.getByText('Built-in vault tools')).toBeVisible();
	await expect(page.getByTestId('builtin-status')).toContainText('always on');

	// 2. The configured external server loads from mcp_servers_get.
	await expect(page.getByText('Files')).toBeVisible();
	await expect(page.getByText('External MCP servers')).toBeVisible();

	// 3. Toggling "Enabled" persists via mcp_servers_set.
	const enabledToggle = page.getByTestId('mcp-server').getByRole('checkbox').first();
	await enabledToggle.uncheck();
	await expect.poll(async () => (await readState(page)).calls.mcp_servers_set).toBe(1);

	// 4. Add a new HTTP server → mcp_servers_set called again with the new entry.
	await page.getByPlaceholder('filesystem', { exact: true }).fill('remote-tools');
	await page.getByText('URL (HTTP)').click();
	await page.getByPlaceholder('https://tools.example.com/mcp').fill('https://tools.example.com/mcp');
	await page.getByRole('button', { name: 'Add server' }).click();

	await expect(page.getByText('remote-tools')).toBeVisible();

	const recorded = await readState(page);
	expect(recorded.calls.mcp_servers_set).toBe(2);
	expect(recorded.lastSetConfig).toMatchObject({
		servers: [
			{ id: 'filesystem', enabled: false },
			{ id: 'remote-tools', url: 'https://tools.example.com/mcp', enabled: true }
		]
	});
});
