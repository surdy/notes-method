export interface Hotkey {
key: string;
meta?: boolean;
shift?: boolean;
alt?: boolean;
action: () => void;
}

export function registerHotkeys(hotkeys: Hotkey[]): () => void {
function handler(event: KeyboardEvent) {
for (const hotkey of hotkeys) {
if (
event.key.toLowerCase() === hotkey.key.toLowerCase() &&
!!event.metaKey === !!hotkey.meta &&
!!event.shiftKey === !!hotkey.shift &&
!!event.altKey === !!hotkey.alt
) {
event.preventDefault();
hotkey.action();
return;
}
}
}

window.addEventListener('keydown', handler);
return () => window.removeEventListener('keydown', handler);
}
