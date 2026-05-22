export type Heading = {
	level: number;
	text: string;
	from: number;
};

class HeadingStore {
	headings = $state<Heading[]>([]);
	activeIndex = $state(-1);

	update(headings: Heading[]) {
		this.headings = headings;
	}

	setActive(index: number) {
		this.activeIndex = index;
	}

	clear() {
		this.headings = [];
		this.activeIndex = -1;
	}
}

export const headingStore = new HeadingStore();
