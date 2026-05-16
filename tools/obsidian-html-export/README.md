# Obsidian HTML Export Plugin

Minimal Obsidian plugin that renders each OFM formatting fixture through
Obsidian's `MarkdownRenderer.render()` and writes the normalized HTML output
to `golden-html/` in the vault root.

## Purpose

Generates ground-truth HTML snapshots for comparing Notesmith's rendering
against Obsidian's. Used as part of the OFM rendering test suite.

## Setup

1. Copy the `test-fixtures/obsidian-sandbox/` directory as a vault (or into
   an existing vault so the `Formatting/` folder is at vault root).

2. Copy this plugin into the vault's `.obsidian/plugins/notesmith-html-export/`:
   ```
   cp -r tools/obsidian-html-export/ <vault>/.obsidian/plugins/notesmith-html-export/
   ```

3. Open the vault in Obsidian and enable the plugin in Settings → Community Plugins.

## Usage

1. Open the command palette (`Cmd+P`)
2. Run **"Export golden HTML from fixtures"**
3. The plugin writes `golden-html/<fixture>.html` for each formatting fixture
4. Copy the `golden-html/` directory to `test-fixtures/obsidian-golden/` in the
   Notesmith repo for use in comparison tests

## Refreshing

Re-run the command after Obsidian updates to capture any rendering changes.
The golden HTML files should be committed to the repo and reviewed in PRs.
