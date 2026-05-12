const STORAGE_KEY = 'notesmith:recently-viewed';
const MAX_ENTRIES = 100;

interface ViewedEntry {
path: string;
title: string;
timestamp: number;
}

type VaultEntries = Record<string, ViewedEntry[]>;

function loadAll(): VaultEntries {
try {
const raw = localStorage.getItem(STORAGE_KEY);
if (!raw) return {};
return JSON.parse(raw) as VaultEntries;
} catch {
return {};
}
}

function saveAll(data: VaultEntries): void {
try {
localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
} catch {
// ignore storage errors
}
}

export function recordView(vault: string, path: string, title: string): void {
const all = loadAll();
const entries = all[vault] ?? [];
const filtered = entries.filter((entry) => entry.path !== path);
filtered.unshift({ path, title, timestamp: Date.now() });
all[vault] = filtered.slice(0, MAX_ENTRIES);
saveAll(all);
}

export function getRecentlyViewed(vault: string, limit: number): ViewedEntry[] {
const all = loadAll();
return (all[vault] ?? []).slice(0, limit);
}
