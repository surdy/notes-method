import { expect, test, type Page } from '@playwright/test';

type CreatedNote = {
	title: string;
	content: string;
	folder?: string;
};

async function stubSidebarApi(page: Page) {
	let notes = [
		{
			path: 'Projects/Quill Rail.md',
			title: 'Quill Rail',
			tags: [],
			frontmatter: null
		},
		{
			path: 'Projects/Projects.md',
			title: 'Projects',
			tags: [],
			frontmatter: null
		}
	];
	const created: CreatedNote[] = [];
	let reindexes = 0;

	await page.route('**/api/v/harness/sidebar-config', async (route) => {
		await route.fulfill({ json: { views: [] } });
	});
	await page.route('**/api/app/vaults/harness/reindex', async (route) => {
		reindexes += 1;
		await route.fulfill({ json: { notes: notes.length } });
	});
	await page.route('**/api/v/harness/notes', async (route) => {
		if (route.request().method() === 'GET') {
			await route.fulfill({ json: notes });
			return;
		}

		const body = route.request().postDataJSON() as CreatedNote;
		created.push(body);
		const filename = `${body.title}.md`;
		const path = body.folder ? `${body.folder}/${filename}` : filename;
		notes = [...notes, { path, title: body.title, tags: [], frontmatter: null }];
		await route.fulfill({ json: { path, hash: 'created-hash' } });
	});

	return {
		created,
		get reindexes() {
			return reindexes;
		}
	};
}

test('files toolbar combines search with new note, new folder, and refresh actions', async ({
	page
}) => {
	await stubSidebarApi(page);
	await page.goto('/app/sidebar-files-harness');

	await expect(page.getByRole('textbox', { name: 'Search notes' })).toBeVisible();
	for (const action of ['New note', 'New folder', 'Refresh']) {
		await expect(page.getByRole('button', { name: action })).toBeVisible();
	}
});

test('files toolbar creates notes and folder notes through the shared input palette', async ({
	page
}) => {
	const api = await stubSidebarApi(page);
	await page.goto('/app/sidebar-files-harness');

	await page.getByRole('button', { name: 'New note' }).click();
	let paletteInput = page.getByRole('dialog').getByRole('textbox');
	await paletteInput.fill('Roadmap');
	await paletteInput.press('Enter');
	paletteInput = page.getByRole('dialog').getByRole('textbox');
	await expect(paletteInput).toHaveValue('Inbox');
	await paletteInput.press('Enter');

	await expect.poll(() => api.created).toContainEqual({
		title: 'Roadmap',
		content: '',
		folder: 'Inbox'
	});

	await page.getByRole('button', { name: 'New folder' }).click();
	paletteInput = page.getByRole('dialog').getByRole('textbox');
	await paletteInput.fill('Research');
	await paletteInput.press('Enter');

	await expect.poll(() => api.created).toContainEqual({
		title: 'Research',
		content: '# Research\n',
		folder: 'Research'
	});
});

test('files toolbar refreshes the vault index', async ({ page }) => {
	const api = await stubSidebarApi(page);
	await page.goto('/app/sidebar-files-harness');

	await page.getByRole('button', { name: 'Refresh' }).click();
	await expect.poll(() => api.reindexes).toBe(1);
});

test('new folder refreshes stale notes and opens an existing folder note', async ({ page }) => {
	const api = await stubSidebarApi(page);
	await page.goto('/app/sidebar-files-harness');

	await page.getByRole('button', { name: 'New folder' }).click();
	const paletteInput = page.getByRole('dialog').getByRole('textbox');
	await paletteInput.fill('Projects');
	await paletteInput.press('Enter');

	await expect(page.getByRole('button', { name: 'Projects', exact: true })).toHaveClass(/selected/);
	expect(api.created).toHaveLength(0);
});
