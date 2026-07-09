import { loadConfig, saveConfig, ensureHostPermission } from "./config.js";

const baseUrlInput = document.getElementById("baseUrl");
const defaultVaultInput = document.getElementById("defaultVault");
const statusEl = document.getElementById("status");

async function init() {
  const config = await loadConfig();
  baseUrlInput.value = config.baseUrl;
  defaultVaultInput.value = config.defaultVault;
}

document.getElementById("save").addEventListener("click", async () => {
  const baseUrl = baseUrlInput.value.trim();
  const defaultVault = defaultVaultInput.value.trim();

  try {
    // Validate the URL early.
    new URL(baseUrl);
  } catch {
    statusEl.textContent = "Enter a valid URL.";
    statusEl.style.color = "#c0392b";
    return;
  }

  await saveConfig({ baseUrl, defaultVault });
  // Proactively request host permission so the popup can fetch straight away.
  await ensureHostPermission(baseUrl);

  statusEl.style.color = "#27ae60";
  statusEl.textContent = "Saved.";
  setTimeout(() => (statusEl.textContent = ""), 1500);
});

init();
