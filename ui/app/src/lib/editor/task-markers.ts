import type { TaskMutationStatus } from '$lib/api';

export const TASK_MARKER_PATTERN = ' xX/\\-bwhBWH';

export function taskMarkerToStatus(marker: string): TaskMutationStatus | null {
	switch (marker) {
		case ' ':
			return 'todo';
		case '/':
			return 'in_progress';
		case 'b':
		case 'B':
			return 'blocked';
		case 'w':
		case 'W':
			return 'waiting';
		case 'h':
		case 'H':
			return 'on_hold';
		case 'x':
		case 'X':
			return 'done';
		case '-':
			return 'cancelled';
		default:
			return null;
	}
}

export function nextTaskStatus(status: TaskMutationStatus): TaskMutationStatus {
	return status === 'done' ? 'todo' : 'done';
}

export function taskStatusClass(status: TaskMutationStatus): string {
	return `status-${status.replaceAll('_', '-')}`;
}
