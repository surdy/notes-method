import type { VaultConfigData } from '$lib/api';

export type FieldErrorFn = (key: string) => string | null;
export type FieldWarningFn = (key: string) => string | null;
export type SectionIsDirtyFn = (section: string) => boolean;
export type SaveSectionFn = (section: string) => Promise<void>;
export type RevertFn = (section: string) => void;
export type MarkDirtyFn = (section: string) => void;
export type SaveImmediateFn = (section: string) => Promise<void>;

export interface SectionProps {
cfg: VaultConfigData;
fieldError: FieldErrorFn;
fieldWarning: FieldWarningFn;
sectionIsDirty: SectionIsDirtyFn;
saveSection: SaveSectionFn;
revert: RevertFn;
markDirty: MarkDirtyFn;
saveImmediate: SaveImmediateFn;
}

export function textField(
markDirty: MarkDirtyFn,
section: string,
value: string | null | undefined,
setter: (v: string) => void
) {
return {
value: value ?? '',
oninput(e: Event) {
setter((e.target as HTMLInputElement).value);
markDirty(section);
}
};
}

export function toggleField(
saveImmediate: SaveImmediateFn,
section: string,
value: boolean,
setter: (v: boolean) => void
) {
return {
checked: value,
onchange(e: Event) {
setter((e.target as HTMLInputElement).checked);
void saveImmediate(section);
}
};
}

export function selectField(
saveImmediate: SaveImmediateFn,
section: string,
value: string,
setter: (v: string) => void
) {
return {
value,
onchange(e: Event) {
setter((e.target as HTMLSelectElement).value);
void saveImmediate(section);
}
};
}
