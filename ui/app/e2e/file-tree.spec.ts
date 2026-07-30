import { expect, test } from '@playwright/test';

test('Quill Rail keeps the file tree typographic while honoring explicit icons', async ({
	page
}) => {
	await page.goto('/app/file-tree-harness');

	const plainNote = page.getByRole('button', { name: 'Quill Rail' });
	const customIconNote = page.getByRole('button', { name: '🔬 Custom Icon' });

	await expect(plainNote.locator('.note-icon')).toHaveCount(0);
	await expect(customIconNote.locator('.note-icon')).toHaveText('🔬');
	await expect(page.getByRole('button', { name: 'Projects' }).locator('.note-icon')).toHaveCount(0);
	const plainTitle = await plainNote.locator('.note-title').boundingBox();
	const customIconTitle = await customIconNote.locator('.note-title').boundingBox();
	expect(plainTitle?.x).toBe(customIconTitle?.x);

	await plainNote.click();
	await expect(plainNote).toHaveClass(/selected/);
	await expect(plainNote).toHaveCSS('font-weight', '600');
	await expect(plainNote).not.toHaveCSS('box-shadow', 'none');
});

test('a folder name lines up with a sibling note title at the same depth', async ({ page }) => {
	await page.goto('/app/file-tree-harness');

	// "Research" (a folder) and "Quill Rail" (a note) are both direct
	// children of "Projects", so their titles should share the same left
	// edge rather than the folder looking nested one level deeper.
	const folderName = page.getByRole('button', { name: 'Research' }).locator('.folder-name');
	const noteTitle = page.getByRole('button', { name: 'Quill Rail' }).locator('.note-title');

	const folderBox = await folderName.boundingBox();
	const noteBox = await noteTitle.boundingBox();
	expect(folderBox?.x).toBe(noteBox?.x);
});

test('folder disclosure matches the subtle Quill Rail mockup scale', async ({ page }) => {
	await page.goto('/app/file-tree-harness');

	const disclosure = page.getByRole('button', { name: 'Projects' }).locator('.disclosure');
	const borders = await disclosure.evaluate((element) => {
		const style = getComputedStyle(element, '::before');
		return {
			left: style.borderLeftWidth,
			top: style.borderTopWidth,
			bottom: style.borderBottomWidth
		};
	});

	expect(borders).toEqual({ left: '5px', top: '4px', bottom: '4px' });
});

test('rows hang outliner markers off the indent spine without folder icons', async ({ page }) => {
	await page.goto('/app/file-tree-harness');

	const projects = page.locator('.folder').filter({ hasText: 'Projects' }).first();
	const research = page.locator('.folder').filter({ hasText: 'Research' }).first();

	// Connector stubs are gone. Folders stay unmarked — the chevron alone
	// distinguishes them, so nothing sits in front of the folder name.
	await expect(page.locator('.branch-connector')).toHaveCount(0);
	await expect(page.locator('.folder-marker')).toHaveCount(0);
	await expect(projects.locator('.tree-marker')).toHaveCount(0);
	await expect(research.locator('.tree-marker')).toHaveCount(0);
	await expect(page.locator('.folder-icon')).toHaveCount(0);

	// Notes get a filled dot, unless an explicit icon already marks the row.
	const quillRail = page.getByRole('button', { name: 'Quill Rail' });
	await expect(quillRail.locator('.note-marker')).toHaveCount(1);
	await expect(
		page.getByRole('button', { name: '🔬 Custom Icon' }).locator('.note-marker')
	).toHaveCount(0);

	await page.getByRole('button', { name: 'Research' }).click();
	await expect(page.getByRole('button', { name: 'Structure' }).locator('.note-marker')).toHaveCount(
		1
	);

	// The dot sits on the innermost indent guide of its own row, so notes
	// hang off the spine their parent folder draws.
	const dot = await quillRail.locator('.note-marker').boundingBox();
	const guide = await quillRail.locator('.indent-guide').last().boundingBox();
	expect(dot).not.toBeNull();
	expect(guide).not.toBeNull();
	expect(dot!.x + dot!.width / 2).toBeCloseTo(guide!.x + guide!.width / 2, 1);

	await expect(quillRail.locator('.note-marker')).toHaveCSS('width', '5px');
});

test('the open note takes the accent on its marker', async ({ page }) => {
	await page.goto('/app/file-tree-harness');

	const quillRail = page.getByRole('button', { name: 'Quill Rail' });
	const marker = quillRail.locator('.note-marker');
	const resting = await marker.evaluate((el) => getComputedStyle(el).backgroundColor);

	await quillRail.click();
	await expect(quillRail).toHaveClass(/selected/);
	const selected = await marker.evaluate((el) => getComputedStyle(el).backgroundColor);

	expect(selected).not.toBe(resting);
});
