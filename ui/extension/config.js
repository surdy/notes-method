// Shared config helpers for the Notesmith Web Clipper.
//
// The daemon base URL and default vault are stored in chrome.storage.sync so
// they persist across the popup and options pages.

export const DEFAULT_BASE_URL = "http://127.0.0.1:27183";

export async function loadConfig() {
  const { baseUrl, defaultVault } = await chrome.storage.sync.get([
    "baseUrl",
    "defaultVault",
  ]);
  return {
    baseUrl: (baseUrl || DEFAULT_BASE_URL).replace(/\/+$/, ""),
    defaultVault: defaultVault || "",
  };
}

export async function saveConfig({ baseUrl, defaultVault }) {
  await chrome.storage.sync.set({
    baseUrl: (baseUrl || DEFAULT_BASE_URL).replace(/\/+$/, ""),
    defaultVault: defaultVault || "",
  });
}

// Request host permission for the daemon origin so cross-origin fetches from the
// extension are allowed. Returns true when granted.
export async function ensureHostPermission(baseUrl) {
  let origin;
  try {
    origin = new URL(baseUrl).origin + "/*";
  } catch {
    return false;
  }
  const has = await chrome.permissions.contains({ origins: [origin] });
  if (has) return true;
  return chrome.permissions.request({ origins: [origin] });
}

export async function fetchVaults(baseUrl) {
  const res = await fetch(`${baseUrl}/api/app/vaults`);
  if (!res.ok) {
    throw new Error(`vault list failed (${res.status})`);
  }
  const vaults = await res.json();
  return Array.isArray(vaults) ? vaults : [];
}

export async function clipUrl(baseUrl, vault, url, tags) {
  const res = await fetch(
    `${baseUrl}/api/v/${encodeURIComponent(vault)}/clip`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ url, tags }),
    },
  );
  const body = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(body.error || `clip failed (${res.status})`);
  }
  return body;
}
