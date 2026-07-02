import { useEffect, useMemo, useState } from 'react';
import type { DiffFile, DiffLine, GitHistoryPanelProps } from './types';
import './git-history-panel.css';

function formatWhen(timestampSecs: number): string {
	const d = new Date(timestampSecs * 1000);
	return d.toLocaleString(undefined, {
		month: 'short',
		day: 'numeric',
		hour: '2-digit',
		minute: '2-digit'
	});
}

function statusGlyph(status: DiffFile['status']): string {
	switch (status) {
		case 'added':
			return 'A';
		case 'deleted':
			return 'D';
		case 'renamed':
			return 'R';
		default:
			return 'M';
	}
}

function DiffLineRow({ line }: { line: DiffLine }) {
	const cls = `ghp-line ghp-line--${line.kind}`;
	const sign = line.kind === 'added' ? '+' : line.kind === 'removed' ? '-' : ' ';
	return (
		<div className={cls}>
			<span className="ghp-gutter">{line.oldLine ?? ''}</span>
			<span className="ghp-gutter">{line.newLine ?? ''}</span>
			<span className="ghp-sign">{line.kind === 'hunk' ? '' : sign}</span>
			<span className="ghp-code">{line.text}</span>
		</div>
	);
}

function DiffFileBlock({ file }: { file: DiffFile }) {
	return (
		<section className="ghp-file">
			<header className="ghp-file-head">
				<span className={`ghp-file-status ghp-file-status--${file.status}`}>
					{statusGlyph(file.status)}
				</span>
				<span className="ghp-file-path">{file.path}</span>
				<span className="ghp-file-stat ghp-file-stat--add">+{file.added}</span>
				<span className="ghp-file-stat ghp-file-stat--del">-{file.removed}</span>
			</header>
			<div className="ghp-file-body">
				{file.lines.map((line, i) => (
					<DiffLineRow key={i} line={line} />
				))}
			</div>
		</section>
	);
}

export function GitHistoryPanel({
	commits,
	diffForCommit,
	onSelectCommit,
	initialSelectedSha
}: GitHistoryPanelProps) {
	const [selectedSha, setSelectedSha] = useState(
		initialSelectedSha ?? commits[0]?.sha ?? ''
	);

	// Notify the host of the current selection so it can fetch the diff lazily.
	useEffect(() => {
		if (selectedSha) {
			onSelectCommit?.(selectedSha);
		}
	}, [selectedSha, onSelectCommit]);

	const diff = useMemo(
		() => (selectedSha ? diffForCommit(selectedSha) : undefined),
		[selectedSha, diffForCommit]
	);

	return (
		<div className="ghp-root">
			<ul className="ghp-list" role="listbox" aria-label="Commit history">
				{commits.map((c) => {
					const active = c.sha === selectedSha;
					return (
						<li key={c.sha}>
							<button
								type="button"
								role="option"
								aria-selected={active}
								className={`ghp-item${active ? ' ghp-item--active' : ''}`}
								onClick={() => setSelectedSha(c.sha)}
							>
								<span className="ghp-item-subject">{c.subject}</span>
								<span className="ghp-item-meta">
									<span className="ghp-sha">{c.shortSha}</span>
									<span className="ghp-item-author">{c.author}</span>
									<span className="ghp-item-when">{formatWhen(c.timestampSecs)}</span>
								</span>
								<span className="ghp-item-stats">
									<span className="ghp-item-stat ghp-item-stat--add">+{c.insertions}</span>
									<span className="ghp-item-stat ghp-item-stat--del">-{c.deletions}</span>
									<span className="ghp-item-files">
										{c.filesChanged} file{c.filesChanged === 1 ? '' : 's'}
									</span>
								</span>
							</button>
						</li>
					);
				})}
			</ul>

			<div className="ghp-diff">
				{diff ? (
					diff.files.length > 0 ? (
						diff.files.map((file) => <DiffFileBlock key={file.path} file={file} />)
					) : (
						<p className="ghp-empty">No file changes in this commit.</p>
					)
				) : (
					<p className="ghp-empty">Loading diff…</p>
				)}
			</div>
		</div>
	);
}
