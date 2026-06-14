import { defineConfig, devices } from '@playwright/test';

/**
 * Headless e2e config for the agent chat panel. The flow in
 * `e2e/agent-chat.spec.ts` drives the real {@link AgentPanel} mounted at the
 * `/app/agent-harness` route, stubbing the Tauri IPC bridge and the transcript
 * HTTP endpoints from inside the test. The SvelteKit app uses base path `/app`.
 */
export default defineConfig({
	testDir: 'e2e',
	fullyParallel: true,
	forbidOnly: !!process.env.CI,
	retries: process.env.CI ? 2 : 0,
	reporter: process.env.CI ? 'list' : 'line',
	use: {
		baseURL: 'http://localhost:5173',
		trace: 'on-first-retry'
	},
	projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
	webServer: {
		command: 'npm run dev -- --port 5173',
		url: 'http://localhost:5173/app/agent-harness',
		reuseExistingServer: !process.env.CI,
		timeout: 120_000
	}
});
