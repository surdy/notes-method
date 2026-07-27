export type InputStep =
	| {
			mode: 'text';
			label: string;
			placeholder?: string;
			required?: boolean;
			defaultValue?: string;
	  }
	| {
			mode: 'list';
			label: string;
			items: { id: string; label: string; description?: string }[];
			placeholder?: string;
			/**
			 * Accept a typed value that matches no item. Needed for field pickers:
			 * suggestions come from values already in the vault, but the first
			 * meeting with a new customer has to be able to name one.
			 */
			allowCustom?: boolean;
	  };

export type InputRequest = {
	steps: InputStep[];
	onComplete: (values: (string | null)[]) => void | Promise<void>;
	onCancel?: () => void;
};

class InputPaletteStore {
	request = $state<InputRequest | null>(null);
	currentStep = $state(0);
	values = $state<(string | null)[]>([]);

	open(request: InputRequest) {
		this.request = request;
		this.currentStep = 0;
		this.values = Array(request.steps.length).fill(null);
	}

	async submitStep(value: string) {
		const request = this.request;
		const step = request?.steps[this.currentStep];
		if (!request || !step) return;

		const trimmed = value.trim();
		if (step.mode === 'text') {
			if (step.required && !trimmed) {
				return;
			}

			this.values[this.currentStep] = trimmed ? value : null;
		} else {
			if (!trimmed) {
				return;
			}

			this.values[this.currentStep] = value;
		}

		if (this.currentStep < request.steps.length - 1) {
			this.currentStep += 1;
			return;
		}

		const completedValues = [...this.values];
		this.request = null;
		this.currentStep = 0;
		this.values = [];

		await request.onComplete(completedValues);
	}

	cancel() {
		const request = this.request;
		this.request = null;
		this.currentStep = 0;
		this.values = [];
		request?.onCancel?.();
	}
}

export const inputPalette = new InputPaletteStore();
