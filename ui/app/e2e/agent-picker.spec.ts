import { test, expect, type Page } from '@playwright/test';

/**
 * Headless coverage for the agent picker's empty-state behaviour (ADR 0013,
 * Phase 6 / decision 6). Two failure modes the WebKit `<select>` rendering used
 * to make confusing:
 *   1. When every detected agent is unavailable, the picker must still render the
 *      selected agent's name (a disabled selected `<option>` renders blank), so
 *      the user can see what's selected and reach Settings.
 *   2. When zero agents are detected, the picker is replaced by an inline
 *      empty-state that explains the situation and links to Settings.
 *
 * The Tauri bridge + transcript endpoints are stubbed so the real AgentPanel is
 * exercised exactly as in the desktop shell.
 */

/** Install a fake Tauri bridge whose `agent_list` returns `agents`. */
async function installBridge(page: Page, agents: unknown[]): Promise<void> {
	await page.addInitScript((agentList) => {
		const listeners: Record<string, Array<(e: unknown) => void>> = {};
		async function invoke(cmd: string): Promise<unknown> {
			if (cmd === 'agent_list') return agentList;
			return null;
		}
		async function listen(name: string, handler: (e: unknown) => void): Promise<() => void> {
			(listeners[name] ??= []).push(handler);
			return () => {
				listeners[name] = (listeners[name] ?? []).filter((h) => h !== handler);
			};
		}
		(window as unknown as { __TAURI__: unknown }).__TAURI__ = {
			core: { invoke },
			event: { listen }
		};
	}, agents);
}

/** Stub the per-vault transcript list endpoint (empty). */
async function mockTranscripts(page: Page): Promise<void> {
	await page.route('**/api/v/**/agent/threads', async (route) => {
		await route.fulfill({ contentType: 'application/json', body: '[]' });
	});
}

test('keeps the selected agent visible when all agents are unavailable', async ({ page }) => {
	await installBridge(page, [
		{ id: 'copilot', name: 'Copilot', available: false },
		{ id: 'claude', name: 'Claude', available: false }
	]);
	await mockTranscripts(page);

	await page.goto('/app/agent-harness');

	const picker = page.getByLabel('Agent');
	await expect(picker).toBeVisible();
	// Defaults to the first agent even though it's unavailable…
	await expect(picker).toHaveValue('copilot');
	// …and its option is NOT disabled, so WebKit renders the name (not blank).
	const selected = picker.locator('option[value="copilot"]');
	await expect(selected).toHaveText(/Copilot \(not found\)/);
	await expect(selected).toBeEnabled();
});

test('shows an inline empty-state with a Settings link when no agents are found', async ({
	page
}) => {
	await installBridge(page, []);
	await mockTranscripts(page);

	await page.goto('/app/agent-harness');

	// No picker is rendered…
	await expect(page.getByLabel('Agent')).toHaveCount(0);
	// …instead an explanatory empty-state with a Settings link.
	await expect(page.getByText('No agent CLI found')).toBeVisible();
	await expect(page.getByRole('button', { name: 'Open AI Agent settings' })).toBeVisible();
});
