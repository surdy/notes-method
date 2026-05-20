/**
 * Returns true if the given link URL should be treated as "external" — i.e. it
 * leaves the current vault (web, mail, custom URL schemes, protocol-relative).
 *
 * Anything else (relative paths, fragment anchors, vault-internal markdown
 * links) is treated as internal.
 */
export function isExternalLinkUrl(url: string): boolean {
	if (!url) return false;
	if (url.startsWith('//')) return true;
	return /^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(url);
}
