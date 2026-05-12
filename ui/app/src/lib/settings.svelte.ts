import {
	getCapabilities,
	getVaultConfig,
	putVaultConfig,
	type Capabilities,
	type ConfigConflictError,
	type VaultConfigData,
	ApiError
} from './api';

export type SettingsStatus = 'idle' | 'loading' | 'saving' | 'error';

class SettingsStore {
	open = $state(false);

	capabilities = $state<Capabilities | null>(null);

	serverConfig = $state<VaultConfigData | null>(null);
	draftConfig = $state<VaultConfigData | null>(null);
	etag = $state<string>('');
	configPath = $state<string>('');
	warnings = $state<Record<string, string>>({});

	status = $state<SettingsStatus>('idle');
	error = $state<string | null>(null);
	fieldErrors = $state<Record<string, string>>({});
	conflict = $state<ConfigConflictError | null>(null);

	dirtySections = $state<Set<string>>(new Set());

	get isDirty(): boolean {
		return this.dirtySections.size > 0;
	}

	toggle() {
		if (this.open) {
			this.close();
		} else {
			this.open = true;
		}
	}

	close() {
		if (this.isDirty) {
			const discard = window.confirm('You have unsaved settings changes. Discard them?');
			if (!discard) return;
		}
		this.open = false;
		this.conflict = null;
		this.fieldErrors = {};
		this.error = null;
	}

	forceClose() {
		this.open = false;
		this.conflict = null;
		this.fieldErrors = {};
		this.error = null;
		this.dirtySections = new Set();
	}

	async loadCapabilities() {
		try {
			this.capabilities = await getCapabilities();
		} catch (e) {
			console.error('Failed to load capabilities', e);
		}
	}

	async loadConfig(vault: string) {
		this.status = 'loading';
		this.error = null;
		this.fieldErrors = {};
		this.conflict = null;

		try {
			const result = await getVaultConfig(vault);
			this.serverConfig = result.config;
			this.draftConfig = structuredClone(result.config);
			this.etag = result.etag;
			this.configPath = result.path;
			this.warnings = result.warnings;
			this.dirtySections = new Set();
			this.status = 'idle';
		} catch (e) {
			this.error = e instanceof Error ? e.message : 'Failed to load config';
			this.status = 'error';
		}
	}

	async saveConfig(vault: string) {
		if (!this.draftConfig) return;

		this.status = 'saving';
		this.error = null;
		this.fieldErrors = {};
		this.conflict = null;

		try {
			const result = await putVaultConfig(vault, this.draftConfig, this.etag);
			this.serverConfig = result.config;
			this.draftConfig = structuredClone(result.config);
			this.etag = result.etag;
			this.warnings = result.warnings;
			this.dirtySections = new Set();
			this.status = 'idle';
			return true;
		} catch (e) {
			if (e instanceof ApiError && e.status === 409) {
				const conflictData = (e as ApiError & { conflict: ConfigConflictError }).conflict;
				this.conflict = conflictData;
				this.status = 'idle';
				return false;
			}
			if (e instanceof ApiError && e.status === 422) {
				const validationData = (e as ApiError & { validation: { errors: Record<string, string> } }).validation;
				this.fieldErrors = validationData.errors;
				this.status = 'idle';
				return false;
			}
			this.error = e instanceof Error ? e.message : 'Failed to save config';
			this.status = 'error';
			return false;
		}
	}

	acceptServerVersion() {
		if (this.conflict) {
			this.serverConfig = this.conflict.config;
			this.draftConfig = structuredClone(this.conflict.config);
			this.etag = this.conflict.hash;
			this.warnings = this.conflict.warnings;
			this.conflict = null;
			this.dirtySections = new Set();
		}
	}

	async overwriteConflict(vault: string) {
		if (this.conflict && this.draftConfig) {
			this.etag = this.conflict.hash;
			this.conflict = null;
			return this.saveConfig(vault);
		}
		return false;
	}

	revertSection(section: string) {
		if (!this.serverConfig || !this.draftConfig) return;
		const key = section as keyof VaultConfigData;
		if (key in this.serverConfig) {
			this.draftConfig = {
				...this.draftConfig,
				[key]: structuredClone(this.serverConfig[key])
			} as VaultConfigData;
		}
		const updated = new Set(this.dirtySections);
		updated.delete(section);
		this.dirtySections = updated;
	}

	markDirty(section: string) {
		if (!this.dirtySections.has(section)) {
			const updated = new Set(this.dirtySections);
			updated.add(section);
			this.dirtySections = updated;
		}
	}

	markClean(section: string) {
		if (this.dirtySections.has(section)) {
			const updated = new Set(this.dirtySections);
			updated.delete(section);
			this.dirtySections = updated;
		}
	}

	async handleExternalConfigChange(vault: string) {
		if (this.open && this.status !== 'saving') {
			await this.loadConfig(vault);
		}
	}
}

export const settingsStore = new SettingsStore();
