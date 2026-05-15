import type { QueuedSave } from './save-queue.ts';

export interface QueuedSaveRecord extends QueuedSave {
	id: number;
}

const DB_NAME = 'notesmith-save-queue';
const STORE_NAME = 'pending-saves';
const PATH_INDEX = 'path';

let openDatabasePromise: Promise<IDBDatabase> | null = null;
let memoryId = 0;
const memoryQueue: QueuedSaveRecord[] = [];

function canUseIndexedDb(): boolean {
	return typeof indexedDB !== 'undefined';
}

function toRecord(id: number, save: QueuedSave): QueuedSaveRecord {
	return { id, ...save };
}

function requestToPromise<T>(request: IDBRequest<T>): Promise<T> {
	return new Promise((resolve, reject) => {
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error ?? new Error('IndexedDB request failed'));
	});
}

function transactionToPromise(transaction: IDBTransaction): Promise<void> {
	return new Promise((resolve, reject) => {
		transaction.oncomplete = () => resolve();
		transaction.onerror = () =>
			reject(transaction.error ?? new Error('IndexedDB transaction failed'));
		transaction.onabort = () => reject(transaction.error ?? new Error('IndexedDB transaction aborted'));
	});
}

async function withDatabase<T>(
	mode: IDBTransactionMode,
	handler: (store: IDBObjectStore, transaction: IDBTransaction) => Promise<T> | T
): Promise<T> {
	const db = await openDatabase();
	const transaction = db.transaction(STORE_NAME, mode);
	const store = transaction.objectStore(STORE_NAME);
	const result = await handler(store, transaction);
	await transactionToPromise(transaction);
	return result;
}

function openDatabase(): Promise<IDBDatabase> {
	if (!canUseIndexedDb()) {
		throw new Error('IndexedDB unavailable');
	}
	if (openDatabasePromise) {
		return openDatabasePromise;
	}
	openDatabasePromise = new Promise((resolve, reject) => {
		const request = indexedDB.open(DB_NAME, 1);
		request.onupgradeneeded = () => {
			const db = request.result;
			const store = db.createObjectStore(STORE_NAME, {
				keyPath: 'id',
				autoIncrement: true
			});
			store.createIndex(PATH_INDEX, 'path', { unique: false });
		};
		request.onsuccess = () => resolve(request.result);
		request.onerror = () => reject(request.error ?? new Error('Failed to open IndexedDB'));
	});
	return openDatabasePromise;
}

function findMemoryRecord(vault: string, path: string): QueuedSaveRecord | undefined {
	return memoryQueue.find((record) => record.vault === vault && record.path === path);
}

export async function enqueue(save: QueuedSave): Promise<number> {
	if (!canUseIndexedDb()) {
		const existing = findMemoryRecord(save.vault, save.path);
		if (existing) {
			Object.assign(existing, save, { retryCount: 0 });
			return existing.id;
		}
		const id = ++memoryId;
		memoryQueue.push(toRecord(id, save));
		return id;
	}

	return withDatabase('readwrite', async (store) => {
		const existing = ((await requestToPromise(
			store.index(PATH_INDEX).getAll(save.path)
		)) as QueuedSaveRecord[]).find((record) => record.vault === save.vault);

		if (existing) {
			const nextRecord = {
				...existing,
				...save,
				retryCount: 0
			};
			await requestToPromise(store.put(nextRecord));
			return existing.id;
		}

		const id = await requestToPromise(store.add(save));
		return typeof id === 'number' ? id : Number(id);
	});
}

export async function dequeue(id: number): Promise<void> {
	if (!canUseIndexedDb()) {
		const index = memoryQueue.findIndex((record) => record.id === id);
		if (index >= 0) {
			memoryQueue.splice(index, 1);
		}
		return;
	}

	await withDatabase('readwrite', async (store) => {
		await requestToPromise(store.delete(id));
	});
}

export async function getAll(): Promise<QueuedSaveRecord[]> {
	if (!canUseIndexedDb()) {
		return [...memoryQueue].sort((left, right) => left.timestamp - right.timestamp);
	}

	const records = await withDatabase('readonly', async (store) => {
		return requestToPromise(store.getAll()) as Promise<QueuedSaveRecord[]>;
	});
	return [...records].sort((left, right) => left.timestamp - right.timestamp);
}

export async function getCount(): Promise<number> {
	if (!canUseIndexedDb()) {
		return memoryQueue.length;
	}

	return withDatabase('readonly', async (store) => requestToPromise(store.count()));
}

export async function clear(): Promise<void> {
	if (!canUseIndexedDb()) {
		memoryQueue.splice(0, memoryQueue.length);
		return;
	}

	await withDatabase('readwrite', async (store) => {
		await requestToPromise(store.clear());
	});
}

export async function get(id: number): Promise<QueuedSaveRecord | undefined> {
	if (!canUseIndexedDb()) {
		return memoryQueue.find((record) => record.id === id);
	}

	const record = await withDatabase('readonly', async (store) => {
		return requestToPromise(store.get(id)) as Promise<QueuedSaveRecord | undefined>;
	});
	return record;
}

export async function update(id: number, changes: Partial<QueuedSave>): Promise<void> {
	if (!canUseIndexedDb()) {
		const record = memoryQueue.find((entry) => entry.id === id);
		if (record) {
			Object.assign(record, changes);
		}
		return;
	}

	await withDatabase('readwrite', async (store) => {
		const current = (await requestToPromise(store.get(id))) as QueuedSaveRecord | undefined;
		if (!current) {
			return;
		}
		await requestToPromise(
			store.put({
				...current,
				...changes
			})
		);
	});
}

export async function removeByPath(vault: string, path: string): Promise<void> {
	if (!canUseIndexedDb()) {
		const index = memoryQueue.findIndex((record) => record.vault === vault && record.path === path);
		if (index >= 0) {
			memoryQueue.splice(index, 1);
		}
		return;
	}

	await withDatabase('readwrite', async (store) => {
		const matches = ((await requestToPromise(
			store.index(PATH_INDEX).getAll(path)
		)) as QueuedSaveRecord[]).filter((record) => record.vault === vault);
		await Promise.all(matches.map((record) => requestToPromise(store.delete(record.id))));
	});
}
