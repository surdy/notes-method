/**
 * Shared holder for the live chat {@link ChatStore} (issue #195). The chat store
 * is created and owned by `AgentPanel.svelte` (rebuilt per vault), but inline
 * editor commands — invoked from the command palette or the editor's right-click
 * menu — need to reach it without the panel being expanded. The panel publishes
 * its current store here on init and clears it on dispose, so the editor side can
 * read `activeSession.current` regardless of panel visibility.
 */

import type { ChatStore } from './chat-store.svelte.ts';

export class ActiveSessionStore {
	store = $state<ChatStore | null>(null);

	set(store: ChatStore | null): void {
		this.store = store;
	}

	get current(): ChatStore | null {
		return this.store;
	}
}

export const activeSession = new ActiveSessionStore();
