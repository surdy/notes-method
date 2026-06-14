import { test, expect, type Page } from '@playwright/test';

/**
 * Headless end-to-end flow for the agent chat panel (ADR 0012, Phase 8).
 *
 * Drives the real {@link AgentPanel} through the full streaming lifecycle:
 * prompt → streamed agent reply → tool call/result → write-permission prompt →
 * user grants permission → turn completes. The Tauri IPC bridge
 * (`window.__TAURI__`) and the transcript HTTP endpoints are stubbed here so the
 * Svelte component is exercised exactly as it runs in the desktop shell — this
 * is the user-visible state validation the repo's race/loading rules require,
 * not a curl-only API check.
 *
 * The fake bridge mirrors real Tauri semantics: `event.listen` handlers are
 * invoked with a `{ payload }` envelope (the bug the panel's `eventPayload`
 * unwrap fixes), and `agent_answer_permission` resolves the pending write before
 * the turn's `done` event fires.
 */

/** Install the fake Tauri bridge + record call counts before the app loads. */
async function installFakeBridge(page: Page): Promise<void> {
	await page.addInitScript(() => {
		const SID = 'sess-1';
		const listeners: Record<string, Array<(e: unknown) => void>> = {};
		const calls = { agent_start: 0, agent_prompt: 0, createThread: 0 };
		const w = window as unknown as {
			__testState: { calls: typeof calls; answered: unknown };
		};
		w.__testState = { calls, answered: null };

		function emit(name: string, payload: unknown) {
			for (const h of listeners[name] ?? []) h({ payload });
		}

		async function invoke(cmd: string, args: Record<string, unknown>): Promise<unknown> {
			switch (cmd) {
				case 'agent_list':
					return [
						{ id: 'copilot', name: 'Copilot', available: true },
						{ id: 'claude', name: 'Claude', available: false }
					];
				case 'agent_start':
					calls.agent_start += 1;
					return {
						sessionId: SID,
						models: {
							current: 'gpt-4o',
							options: [
								{ id: 'gpt-4o', name: 'GPT-4o' },
								{ id: 'o1', name: 'o1' }
							]
						}
					};
				case 'agent_prompt': {
					calls.agent_prompt += 1;
					const text = String(args.text ?? '');
					const ev = (event: unknown, delay: number) =>
						setTimeout(() => emit('notesmith://agent-event', { sessionId: SID, event }), delay);
					ev({ type: 'user_message', text }, 10);
					ev({ type: 'agent_message_delta', text: 'Here is ' }, 30);
					ev({ type: 'agent_message_delta', text: 'a summary of this week.' }, 50);
					ev({ type: 'tool_call', id: 't1', name: 'search_notes', args: { query: 'this week' } }, 70);
					ev({ type: 'tool_result', id: 't1', content: '3 notes found', is_error: false }, 90);
					setTimeout(
						() =>
							emit('notesmith://agent-permission', {
								sessionId: SID,
								requestId: 'perm-1',
								request: { tool: 'Write', kind: 'edit' }
							}),
						110
					);
					return null;
				}
				case 'agent_answer_permission':
					w.__testState.answered = args;
					setTimeout(
						() =>
							emit('notesmith://agent-event', {
								sessionId: SID,
								event: { type: 'done', result: null }
							}),
						10
					);
					return null;
				default:
					return null;
			}
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
	});
}

/** Stub the per-vault transcript REST endpoints and count thread creates. */
async function mockTranscripts(page: Page): Promise<void> {
	await page.route('**/api/v/**/agent/threads', async (route) => {
		if (route.request().method() === 'POST') {
			await page.evaluate(() => {
				const s = (window as unknown as { __testState: { calls: { createThread: number } } })
					.__testState;
				s.calls.createThread += 1;
			});
			const now = new Date().toISOString();
			await route.fulfill({
				contentType: 'application/json',
				body: JSON.stringify({
					id: 'thread-1',
					vault: 'harness',
					title: 'Summarise this week',
					agent: 'copilot',
					model: 'gpt-4o',
					created_at: now,
					updated_at: now
				})
			});
			return;
		}
		// GET listThreads — none yet.
		await route.fulfill({ contentType: 'application/json', body: '[]' });
	});

	// appendMessage (user + agent) and any other thread sub-resource.
	await page.route('**/api/v/**/agent/threads/**', async (route) => {
		const now = new Date().toISOString();
		await route.fulfill({
			contentType: 'application/json',
			body: JSON.stringify({
				id: 1,
				thread_id: 'thread-1',
				seq: 1,
				role: 'user',
				content: '',
				created_at: now
			})
		});
	});
}

test('streams an agent turn through a write-permission grant to completion', async ({ page }) => {
	await installFakeBridge(page);
	await mockTranscripts(page);

	await page.goto('/app/agent-harness');

	// Panel mounts and the agent picker is populated from agent_list.
	const agentPicker = page.getByLabel('Agent');
	await expect(agentPicker).toBeVisible();
	await expect(agentPicker).toHaveValue('copilot');

	// Compose and send a prompt.
	const composer = page.getByPlaceholder('Message the agent…');
	await composer.fill('Summarise this week');
	await page.getByRole('button', { name: 'Send' }).click();

	// User bubble appears (sourced from the streamed user_message event).
	await expect(page.locator('.message.user .text')).toHaveText('Summarise this week');

	// Streamed agent deltas accumulate into one bubble.
	await expect(page.locator('.message.agent .text')).toHaveText('Here is a summary of this week.');

	// Tool-call card renders with the tool name.
	await expect(page.locator('.tool .name')).toHaveText('search_notes');

	// Write-permission prompt appears mid-turn.
	const prompt = page.getByRole('alertdialog', { name: 'Agent permission request' });
	await expect(prompt).toBeVisible();
	await expect(prompt).toContainText('Write');
	await expect(prompt).toContainText('edit');

	// Grant once → prompt disappears and the decision is recorded by the bridge.
	await prompt.getByRole('button', { name: 'Allow once' }).click();
	await expect(prompt).toBeHidden();

	// The turn completes: Send is re-enabled once `done` clears the busy flag.
	await expect(page.getByRole('button', { name: 'Send' })).toBeDisabled();
	await composer.fill('again');
	await expect(page.getByRole('button', { name: 'Send' })).toBeEnabled();

	// Assert the recorded permission decision and bounded duplicate API/IPC calls.
	const recorded = await page.evaluate(
		() =>
			(
				window as unknown as {
					__testState: { calls: Record<string, number>; answered: unknown };
				}
			).__testState
	);
	expect(recorded.answered).toMatchObject({ requestId: 'perm-1', decision: 'allow_once' });
	expect(recorded.calls.createThread).toBe(1);
	expect(recorded.calls.agent_start).toBe(1);
	expect(recorded.calls.agent_prompt).toBe(1);
});
