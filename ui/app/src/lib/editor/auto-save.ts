export function createAutoSave(options: {
delay?: number;
save: (content: string) => Promise<{ hash: string }>;
onSaving?: () => void;
onSaved?: (hash: string) => void;
onError?: (error: unknown) => void;
}) {
const delay = options.delay ?? 1000;
let timer: ReturnType<typeof setTimeout> | null = null;
let saving = false;
let pendingContent: string | null = null;

async function runSave() {
if (saving || pendingContent === null) {
return;
}

const content = pendingContent;
pendingContent = null;
saving = true;
options.onSaving?.();
try {
const result = await options.save(content);
options.onSaved?.(result.hash);
} catch (error) {
pendingContent ??= content;
options.onError?.(error);
throw error;
} finally {
saving = false;
if (pendingContent !== null) {
void runSave();
}
}
}

function schedule(content: string) {
pendingContent = content;
if (timer) {
clearTimeout(timer);
}
timer = setTimeout(() => {
timer = null;
void runSave();
}, delay);
}

async function flush(content?: string) {
if (content !== undefined) {
pendingContent = content;
}
if (timer) {
clearTimeout(timer);
timer = null;
}
await runSave();
}

function cancel() {
if (timer) {
clearTimeout(timer);
timer = null;
}
pendingContent = null;
}

return { schedule, flush, cancel };
}
