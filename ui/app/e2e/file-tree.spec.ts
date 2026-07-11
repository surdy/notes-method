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
