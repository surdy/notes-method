/**
 * Shared focus-request signal for the sidebar file search box. The ⌘⇧F hotkey
 * (wired via the app shell) bumps `focusNonce`; SidebarViews watches it and
 * focuses / selects the search input.
 */
class SidebarSearchStore {
	focusNonce = $state(0);

	requestFocus() {
		this.focusNonce += 1;
	}
}

export const sidebarSearchStore = new SidebarSearchStore();
