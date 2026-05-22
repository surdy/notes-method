const obsidian = require("obsidian");

// Fixture files to render (relative to vault root)
const FIXTURES = [
  "Formatting/Blockquote.md",
  "Formatting/Callout.md",
  "Formatting/Code block.md",
  "Formatting/Comment.md",
  "Formatting/Diagram.md",
  "Formatting/Embeds.md",
  "Formatting/Emphasis.md",
  "Formatting/Footnote.md",
  "Formatting/Heading.md",
  "Formatting/Highlighting.md",
  "Formatting/Horizontal divider.md",
  "Formatting/Images.md",
  "Formatting/Inline code.md",
  "Formatting/Internal link.md",
  "Formatting/Links.md",
  "Formatting/Lists.md",
  "Formatting/Math.md",
  "Formatting/Strikethrough.md",
  "Formatting/Table.md",
  "Formatting/Task.md",
];

class HtmlExportPlugin extends obsidian.Plugin {
  async onload() {
    this.addCommand({
      id: "export-golden-html",
      name: "Export golden HTML from fixtures",
      callback: () => this.exportAll(),
    });
  }

  async exportAll() {
    const outputDir = "golden-html";

    // Ensure output directory exists
    if (!(await this.app.vault.adapter.exists(outputDir))) {
      await this.app.vault.adapter.mkdir(outputDir);
    }

    let exported = 0;
    let errors = [];

    for (const fixturePath of FIXTURES) {
      try {
        const file = this.app.vault.getAbstractFileByPath(fixturePath);
        if (!file || !(file instanceof obsidian.TFile)) {
          errors.push(`Not found: ${fixturePath}`);
          continue;
        }

        const content = await this.app.vault.read(file);

        // Render using Obsidian's MarkdownRenderer
        const container = document.createElement("div");
        await obsidian.MarkdownRenderer.render(
          this.app,
          content,
          container,
          fixturePath,
          new obsidian.Component()
        );

        // Normalize: remove Obsidian-specific wrapper classes
        const html = normalizeHtml(container.innerHTML);

        // Write to output
        const baseName = fixturePath
          .replace("Formatting/", "")
          .replace(".md", ".html");
        await this.app.vault.adapter.write(`${outputDir}/${baseName}`, html);
        exported++;
      } catch (e) {
        errors.push(`${fixturePath}: ${e.message}`);
      }
    }

    const msg =
      `Exported ${exported}/${FIXTURES.length} fixtures to ${outputDir}/` +
      (errors.length ? `\n\nErrors:\n${errors.join("\n")}` : "");

    new obsidian.Notice(msg, 10000);
    console.log(msg);
  }
}

function normalizeHtml(html) {
  // Strip Obsidian-internal attributes and classes that aren't part of the
  // rendering spec (data-heading, contenteditable, spellcheck, etc.)
  return html
    .replace(/ contenteditable="[^"]*"/g, "")
    .replace(/ spellcheck="[^"]*"/g, "")
    .replace(/ dir="[^"]*"/g, "")
    .replace(/ tabindex="[^"]*"/g, "")
    .replace(/\s+class=""/g, "")
    .replace(/\n\s*\n/g, "\n")
    .trim();
}

module.exports = HtmlExportPlugin;
