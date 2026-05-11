import { RangeSetBuilder, StateEffect, StateField, type Extension } from '@codemirror/state';
import {
	Decoration,
	type DecorationSet,
	EditorView,
	ViewPlugin,
	type ViewUpdate,
	WidgetType
} from '@codemirror/view';

import { executeSql, type SqlQueryResult } from '../api.ts';
import { findSqlBlocks, type SqlBlock } from './sql-blocks-helpers.ts';

interface SqlBlockQueryState {
	result: SqlQueryResult | null;
	error: string | null;
	loading: boolean;
	token: number;
}

const setSqlBlockDecorations = StateEffect.define<DecorationSet>();
export const refreshSqlBlockResults = StateEffect.define<number>();

const sqlBlockDecorationsField = StateField.define<DecorationSet>({
	create() {
		return Decoration.none;
	},
	update(decorations, transaction) {
		for (const effect of transaction.effects) {
			if (effect.is(setSqlBlockDecorations)) {
				return effect.value;
			}
		}
		return transaction.docChanged ? decorations.map(transaction.changes) : decorations;
	},
	provide: (field) => EditorView.decorations.from(field)
});

export function createSqlBlockPlugin(vault: () => string): Extension {
	return [
		sqlBlockDecorationsField,
		ViewPlugin.fromClass(
		class {
			private blocks: SqlBlock[] = [];
			private cache = new Map<string, SqlBlockQueryState>();
			private debounceHandle: number | null = null;
			private decorationHandle: number | null = null;
			private requestToken = 0;
			private destroyed = false;
			private readonly view: EditorView;

			constructor(view: EditorView) {
				this.view = view;
				this.blocks = findSqlBlocks(view.state.doc.toString());
				this.scheduleDecorationUpdate();
				this.scheduleQueries(true, 0);
			}

			update(update: ViewUpdate) {
				const refreshRequested = update.transactions.some((transaction) =>
					transaction.effects.some((effect) => effect.is(refreshSqlBlockResults))
				);
				if (update.docChanged) {
					this.blocks = findSqlBlocks(update.state.doc.toString());
					this.scheduleDecorationUpdate();
					this.scheduleQueries(false, 500);
					return;
				}

				if (refreshRequested) {
					this.markActiveBlocksLoading();
					this.scheduleDecorationUpdate();
					this.scheduleQueries(true, 0);
				}
			}

			destroy() {
				this.destroyed = true;
				if (this.debounceHandle !== null) {
					window.clearTimeout(this.debounceHandle);
				}
				if (this.decorationHandle !== null) {
					window.clearTimeout(this.decorationHandle);
				}
			}

			private scheduleQueries(force: boolean, delay: number) {
				if (this.debounceHandle !== null) {
					window.clearTimeout(this.debounceHandle);
				}

				this.debounceHandle = window.setTimeout(() => {
					void this.runQueries(force);
				}, delay);
			}

			private async runQueries(force: boolean) {
				this.debounceHandle = null;
				const currentVault = vault();

				const activeSql = [...new Set(this.blocks.map((block) => block.sql).filter((sql) => sql.length > 0))];
				await Promise.all(
					activeSql.map(async (sql) => {
						const cached = this.cache.get(sql);
						if (!force && cached && (cached.loading || cached.result || cached.error)) {
							return;
						}

						const token = ++this.requestToken;
						this.cache.set(sql, {
							result: null,
							error: null,
							loading: true,
							token
						});
						this.invalidateDecorations();

						try {
							const result = await executeSql(currentVault, sql);
							const current = this.cache.get(sql);
							if (!current || current.token !== token) {
								return;
							}

							this.cache.set(sql, {
								result,
								error: null,
								loading: false,
								token
							});
						} catch (error) {
							const current = this.cache.get(sql);
							if (!current || current.token !== token) {
								return;
							}

							this.cache.set(sql, {
								result: null,
								error: error instanceof Error ? error.message : 'Query failed',
								loading: false,
								token
							});
						}

						this.invalidateDecorations();
					})
				);
			}

			private markActiveBlocksLoading() {
				for (const sql of new Set(this.blocks.map((block) => block.sql).filter((value) => value.length > 0))) {
					const previous = this.cache.get(sql);
					this.cache.set(sql, {
						result: null,
						error: null,
						loading: true,
						token: previous?.token ?? 0
					});
				}
			}

			private invalidateDecorations() {
				this.scheduleDecorationUpdate();
			}

			private scheduleDecorationUpdate() {
				if (this.destroyed) {
					return;
				}
				if (this.decorationHandle !== null) {
					return;
				}

				this.decorationHandle = window.setTimeout(() => {
					this.decorationHandle = null;
					if (this.destroyed) {
						return;
					}
					this.view.dispatch({
						effects: setSqlBlockDecorations.of(this.buildDecorations())
					});
				}, 0);
			}

			private buildDecorations(): DecorationSet {
				if (this.blocks.length === 0) {
					return Decoration.none;
				}

				const builder = new RangeSetBuilder<Decoration>();

				for (const block of this.blocks) {
					const queryState = this.cache.get(block.sql);
					builder.add(
						block.blockEnd,
						block.blockEnd,
						Decoration.widget({
							widget: new SqlResultWidget(
								queryState?.result ?? null,
								queryState?.error ?? null,
								queryState?.loading ?? true
							),
							block: true,
							side: 1
						})
					);
				}

				return builder.finish();
			}
		}
		)
	];
}

class SqlResultWidget extends WidgetType {
	private readonly result: SqlQueryResult | null;
	private readonly error: string | null;
	private readonly loading: boolean;

	constructor(
		result: SqlQueryResult | null,
		error: string | null,
		loading: boolean
	) {
		super();
		this.result = result;
		this.error = error;
		this.loading = loading;
	}

	eq(other: SqlResultWidget): boolean {
		return (
			this.result === other.result && this.error === other.error && this.loading === other.loading
		);
	}

	toDOM(): HTMLElement {
		const container = document.createElement('div');
		container.className = 'cm-sql-result';
		container.contentEditable = 'false';

		if (this.loading) {
			container.textContent = 'Executing query...';
			container.className += ' cm-sql-loading';
			return container;
		}

		if (this.error) {
			container.textContent = `SQL Error: ${this.error}`;
			container.className += ' cm-sql-error';
			return container;
		}

		if (!this.result || this.result.rows.length === 0) {
			container.textContent = 'No results';
			container.className += ' cm-sql-empty';
			return container;
		}

		const table = document.createElement('table');
		table.className = 'cm-sql-table';

		const thead = document.createElement('thead');
		const headerRow = document.createElement('tr');
		for (const column of this.result.columns) {
			const cell = document.createElement('th');
			cell.textContent = column;
			headerRow.appendChild(cell);
		}
		thead.appendChild(headerRow);
		table.appendChild(thead);

		const tbody = document.createElement('tbody');
		for (const row of this.result.rows) {
			const tr = document.createElement('tr');
			for (const column of this.result.columns) {
				const td = document.createElement('td');
				const value = row[column];
				td.textContent = value == null ? '' : String(value);
				tr.appendChild(td);
			}
			tbody.appendChild(tr);
		}
		table.appendChild(tbody);
		container.appendChild(table);

		return container;
	}
}
