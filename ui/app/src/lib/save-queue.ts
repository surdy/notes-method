import { get, writable, type Readable } from 'svelte/store';

import { ApiError } from './api/core.ts';
import {
	getNote,
	listNotes,
	putNote,
	type NoteDetail,
	type NoteSummary,
	type WriteNoteResponse
} from './api/notes.ts';
import * as offlineQueue from './offline-queue.ts';
import type { QueuedSaveRecord } from './offline-queue.ts';

export type SaveState = 'idle' | 'saving' | 'saved' | 'failed-retrying' | 'failed';

export interface QueuedSave {
	vault: string;
	path: string;
	content: string;
	timestamp: number;
	retryCount: number;
}

export type ConflictDecision = 'overwrite' | 'keep-server';

type TimerHandle = ReturnType<typeof setTimeout> | number;
type ScheduledCallback = () => void | Promise<void>;

interface SaveQueueAdapter {
	enqueue(save: QueuedSave): Promise<number>;
	dequeue(id: number): Promise<void>;
	getAll(): Promise<QueuedSaveRecord[]>;
	getCount(): Promise<number>;
	clear(): Promise<void>;
	get(id: number): Promise<QueuedSaveRecord | undefined>;
	update(id: number, changes: Partial<QueuedSave>): Promise<void>;
	removeByPath(vault: string, path: string): Promise<void>;
}

interface SaveSummaryMap extends Map<string, NoteSummary> {}

export interface SaveQueueDependencies {
	putNote: (
		vault: string,
		path: string,
		content: string,
		expectedHash?: string | null
	) => Promise<WriteNoteResponse>;
	getNote: (vault: string, path: string) => Promise<NoteDetail>;
	listNotes: (vault: string) => Promise<NoteSummary[]>;
	queue: SaveQueueAdapter;
	confirmConflict: (save: QueuedSaveRecord, currentNote: NoteDetail) => Promise<ConflictDecision>;
	schedule: (callback: ScheduledCallback, delay: number) => TimerHandle;
	cancel: (handle: TimerHandle) => void;
	autoInit?: boolean;
}

export interface SaveOptions {
	expectedHash?: string | null;
	fallbackHash?: string | null;
}

const MAX_RETRIES = 3;
const SAVE_CLEAR_MS = 2_000;
const BASE_RETRY_MS = 1_000;

function defaultConfirmConflict(save: QueuedSaveRecord): Promise<ConflictDecision> {
	if (typeof window === 'undefined' || typeof window.confirm !== 'function') {
		return Promise.resolve('overwrite');
	}

	const overwrite = window.confirm(
		`This note was modified while offline:\n\n${save.path}\n\nPress OK to overwrite the server version, or Cancel to keep the server version.`
	);
	return Promise.resolve(overwrite ? 'overwrite' : 'keep-server');
}

function isRetryable(error: unknown): boolean {
	if (error instanceof ApiError) {
		if (error.status === 408 || error.status === 429) {
			return true;
		}
		return error.status >= 500;
	}
	return true;
}

function isConflict(error: unknown): boolean {
	return error instanceof ApiError && error.status === 409;
}

function toMillis(value?: string): number | null {
	if (!value) {
		return null;
	}
	const millis = Date.parse(value);
	return Number.isNaN(millis) ? null : millis;
}

export class SaveQueue {
	readonly saveState = writable<SaveState>('idle');
	readonly queuedCount = writable(0);

	private readonly dependencies: SaveQueueDependencies;
	private readonly retryTimers = new Map<number, TimerHandle>();
	private clearStateTimer: TimerHandle | null = null;
	private flushing = false;

	constructor(dependencies?: Partial<SaveQueueDependencies>) {
		this.dependencies = {
			putNote,
			getNote,
			listNotes,
			queue: offlineQueue,
			confirmConflict: defaultConfirmConflict,
			schedule: (callback, delay) => setTimeout(callback, delay),
			cancel: (handle) => clearTimeout(handle as ReturnType<typeof setTimeout>),
			autoInit: true,
			...dependencies
		};

		if (this.dependencies.autoInit) {
			void this.refreshQueuedCount();
		}
	}

	async refreshQueuedCount(): Promise<number> {
		const count = await this.dependencies.queue.getCount();
		this.queuedCount.set(count);
		return count;
	}

	async save(
		vault: string,
		path: string,
		content: string,
		options: SaveOptions = {}
	): Promise<WriteNoteResponse> {
		this.setState('saving');

		try {
			const result = await this.dependencies.putNote(vault, path, content, options.expectedHash);
			await this.cancelRetryTimerForPath(vault, path);
			await this.dependencies.queue.removeByPath(vault, path);
			await this.refreshQueuedCount();
			this.setState('saved');
			return result;
		} catch (error) {
			if (isConflict(error)) {
				this.setState('idle');
				throw error;
			}
			if (!isRetryable(error)) {
				this.setState('failed');
				throw error;
			}

			const id = await this.dependencies.queue.enqueue({
				vault,
				path,
				content,
				timestamp: Date.now(),
				retryCount: 0
			});
			await this.refreshQueuedCount();
			this.setState('failed-retrying');
			this.scheduleRetry(id, 0);

			return {
				path,
				hash: options.fallbackHash ?? options.expectedHash ?? ''
			};
		}
	}

	async retryAll(): Promise<void> {
		const saves = await this.dependencies.queue.getAll();
		await Promise.all(saves.map((save) => this.resetRetry(save.id)));
		await this.flushQueuedRecords();
	}

	async flushOnReconnect(): Promise<void> {
		const saves = await this.dependencies.queue.getAll();
		await Promise.all(saves.map((save) => this.resetRetry(save.id)));
		await this.flushQueuedRecords();
	}

	private async resetRetry(id: number): Promise<void> {
		this.cancelRetryTimer(id);
		await this.dependencies.queue.update(id, { retryCount: 0 });
	}

	private setState(nextState: SaveState) {
		if (this.clearStateTimer) {
			this.dependencies.cancel(this.clearStateTimer);
			this.clearStateTimer = null;
		}

		this.saveState.set(nextState);
		if (nextState === 'saved') {
			this.clearStateTimer = this.dependencies.schedule(() => {
				this.clearStateTimer = null;
				if (get(this.saveState) === 'saved') {
					this.saveState.set('idle');
				}
			}, SAVE_CLEAR_MS);
		}
	}

	private scheduleRetry(id: number, retryCount: number) {
		this.cancelRetryTimer(id);
		const delay = BASE_RETRY_MS * 2 ** retryCount;
		const handle = this.dependencies.schedule(() => {
			this.retryTimers.delete(id);
			return this.retryQueuedSave(id);
		}, delay);
		this.retryTimers.set(id, handle);
	}

	private cancelRetryTimer(id: number) {
		const handle = this.retryTimers.get(id);
		if (!handle) {
			return;
		}
		this.dependencies.cancel(handle);
		this.retryTimers.delete(id);
	}

	private async cancelRetryTimerForPath(vault: string, path: string) {
		const records = await this.dependencies.queue.getAll();
		for (const record of records) {
			if (record.vault === vault && record.path === path) {
				this.cancelRetryTimer(record.id);
			}
		}
	}

	private async retryQueuedSave(id: number): Promise<void> {
		const save = await this.dependencies.queue.get(id);
		if (!save) {
			await this.refreshQueuedCount();
			return;
		}

		this.setState('failed-retrying');
		try {
			const result = await this.dependencies.putNote(save.vault, save.path, save.content);
			await this.dependencies.queue.dequeue(id);
			await this.refreshQueuedCount();
			this.setState('saved');
			void result;
		} catch (error) {
			if (!isRetryable(error)) {
				this.setState('failed');
				return;
			}

			const nextRetryCount = save.retryCount + 1;
			await this.dependencies.queue.update(id, { retryCount: nextRetryCount });
			if (nextRetryCount >= MAX_RETRIES) {
				this.setState('failed');
				return;
			}

			this.scheduleRetry(id, nextRetryCount);
		}
	}

	private async flushQueuedRecords(): Promise<void> {
		if (this.flushing) {
			return;
		}
		this.flushing = true;
		this.setState('saving');

		try {
			const saves = await this.dependencies.queue.getAll();
			if (saves.length === 0) {
				this.setState('idle');
				return;
			}

			const summariesByVault = await this.loadSummaries(saves);
			let flushedAny = false;
			for (const save of saves) {
				const result = await this.flushQueuedSave(save, summariesByVault.get(save.vault) ?? new Map());
				if (result === 'stopped') {
					return;
				}
				if (result === 'saved') {
					flushedAny = true;
				}
			}

			const remaining = await this.refreshQueuedCount();
			if (remaining > 0) {
				this.setState('failed-retrying');
				return;
			}
			this.setState(flushedAny ? 'saved' : 'idle');
		} finally {
			this.flushing = false;
		}
	}

	private async loadSummaries(saves: QueuedSaveRecord[]): Promise<Map<string, SaveSummaryMap>> {
		const summariesByVault = new Map<string, SaveSummaryMap>();
		for (const vault of [...new Set(saves.map((save) => save.vault))]) {
			const summaries = await this.dependencies.listNotes(vault);
			summariesByVault.set(
				vault,
				new Map(summaries.map((summary) => [summary.path, summary]))
			);
		}
		return summariesByVault;
	}

	private async flushQueuedSave(
		save: QueuedSaveRecord,
		summaries: SaveSummaryMap
	): Promise<'saved' | 'skipped' | 'stopped'> {
		this.cancelRetryTimer(save.id);
		const currentNote = await this.dependencies.getNote(save.vault, save.path);
		const summary = summaries.get(save.path);
		const serverUpdatedAt = toMillis(summary?.updated_at ?? summary?.created_at);

		if (serverUpdatedAt !== null && serverUpdatedAt > save.timestamp) {
			const decision = await this.dependencies.confirmConflict(save, currentNote);
			if (decision === 'keep-server') {
				await this.dependencies.queue.dequeue(save.id);
				await this.refreshQueuedCount();
				this.setState('idle');
				return 'skipped';
			}
		}

		try {
			await this.dependencies.putNote(save.vault, save.path, save.content, currentNote.hash);
			await this.dependencies.queue.dequeue(save.id);
			await this.refreshQueuedCount();
			return 'saved';
		} catch (error) {
			if (!isRetryable(error)) {
				this.setState('failed');
				return 'stopped';
			}
			await this.dependencies.queue.update(save.id, { retryCount: 0 });
			this.setState('failed-retrying');
			this.scheduleRetry(save.id, 0);
			return 'stopped';
		}
	}
}

export const saveQueue = new SaveQueue();
export const saveState: Readable<SaveState> = saveQueue.saveState;
export const queuedCount: Readable<number> = saveQueue.queuedCount;
