import {
  loadConfig,
  saveConfig,
  fetchVaults,
  clipUrl,
  ensureHostPermission,
} from "./config.js";

const vaultSelect = document.getElementById("vault");
const tagsInput = document.getElementById("tags");
const urlEl = document.getElementById("url");
const clipBtn = document.getElementById("clip");
const statusEl = document.getElementById("status");

let currentUrl = "";

function setStatus(message, kind) {
  statusEl.textContent = message;
  statusEl.className = "status" + (kind ? ` ${kind}` : "");
}

async function currentTab() {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  return tab;
}

function parseTags(raw) {
  return raw
    .split(",")
    .map((t) => t.trim())
    .filter((t) => t.length > 0);
}

async function init() {
  const config = await loadConfig();

  const tab = await currentTab();
  currentUrl = tab?.url || "";
  urlEl.textContent = currentUrl;

  if (!/^https?:/i.test(currentUrl)) {
    setStatus("This page can't be clipped (not an http/https URL).", "error");
    return;
  }

  const granted = await ensureHostPermission(config.baseUrl);
  if (!granted) {
    setStatus("Permission to reach the daemon was denied. Open Settings.", "error");
    return;
  }

  try {
    const vaults = await fetchVaults(config.baseUrl);
    if (vaults.length === 0) {
      setStatus("No vaults found on the daemon.", "error");
      return;
    }
    vaultSelect.innerHTML = "";
    for (const vault of vaults) {
      const opt = document.createElement("option");
      opt.value = vault.name;
      opt.textContent = vault.name;
      if (vault.name === config.defaultVault || vault.is_default) {
        opt.selected = true;
      }
      vaultSelect.appendChild(opt);
    }
    clipBtn.disabled = false;
    setStatus("", null);
  } catch (err) {
    setStatus(
      `Could not reach the daemon at ${config.baseUrl}. Check Settings.`,
      "error",
    );
  }
}

clipBtn.addEventListener("click", async () => {
  const config = await loadConfig();
  const vault = vaultSelect.value;
  if (!vault) return;

  clipBtn.disabled = true;
  setStatus("Clipping…", null);

  try {
    const result = await clipUrl(
      config.baseUrl,
      vault,
      currentUrl,
      parseTags(tagsInput.value),
    );
    // Remember the last vault used as the default.
    await saveConfig({ baseUrl: config.baseUrl, defaultVault: vault });
    if (result.duplicate) {
      setStatus(`Already clipped: ${result.path}`, "ok");
    } else {
      setStatus(`Saved: ${result.path}`, "ok");
    }
  } catch (err) {
    setStatus(err.message || "Clip failed.", "error");
    clipBtn.disabled = false;
  }
});

document.getElementById("open-options").addEventListener("click", (e) => {
  e.preventDefault();
  chrome.runtime.openOptionsPage();
});

init();
