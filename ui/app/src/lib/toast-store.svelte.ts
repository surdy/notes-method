export type ToastType = 'success' | 'error' | 'warning';

export type Toast = {
	id: string;
	message: string;
	type: ToastType;
};

class ToastStore {
	toasts = $state<Toast[]>([]);

	add(message: string, type: ToastType = 'success') {
		const id = crypto.randomUUID();
		this.toasts.push({ id, message, type });
		setTimeout(() => this.dismiss(id), 4000);
	}

	dismiss(id: string) {
		this.toasts = this.toasts.filter((toast) => toast.id !== id);
	}
}

export const toastStore = new ToastStore();
