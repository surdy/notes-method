/**
 * The allow tiers offered by the permission prompt (issue #189), as pure data
 * so the prompt's options are unit-testable without mounting the component.
 *
 * - `allow_once` — allow this single call, remember nothing.
 * - `allow_session` — allow + suppress re-prompts for this tool this session only.
 * - `allow_always` — allow + persist the grant so future sessions never re-prompt.
 *
 * "Deny" is rendered separately (it is not an *allow* tier).
 */

import type { PermissionDecision } from './types.ts';

export interface AllowOption {
	decision: Extract<PermissionDecision, 'allow_once' | 'allow_session' | 'allow_always'>;
	label: string;
}

export const ALLOW_OPTIONS: readonly AllowOption[] = [
	{ decision: 'allow_once', label: 'Allow once' },
	{ decision: 'allow_session', label: 'Allow this session' },
	{ decision: 'allow_always', label: 'Always allow' }
];
