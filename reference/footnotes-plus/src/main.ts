import { debounce, Editor, MarkdownView, Notice, Plugin } from "obsidian";
import { applyRenumber } from "./apply";
import { parseFootnotes } from "./footnotes";
import { footnoteHighlighter } from "./highlight";
import { insertOrJumpFootnote } from "./insert";
import { blockingLabels, FootnoteProblem } from "./lint";
import { RenumberResult } from "./renumber";
import { DEFAULT_SETTINGS, FootnotesPlusSettings, FootnotesPlusSettingTab } from "./settings";

export default class FootnotesPlusPlugin extends Plugin {
  settings: FootnotesPlusSettings;
  private applying = false;
  private undoGuardText: string | null = null;
  private scheduleRenumber = debounce(() => this.runAutoRenumber(), 1200, false);

  async onload() {
    await this.loadSettings();

    this.registerEditorExtension([footnoteHighlighter]);

    this.addCommand({
      id: "insert-footnote",
      name: "Insert or jump to footnote",
      icon: "superscript",
      editorCallback: (editor: Editor) => {
        this.withoutAutoRenumber(() =>
          insertOrJumpFootnote(editor, {
            jumpToNewDefinition: this.settings.jumpToNewDefinition,
          })
        );
      },
    });

    this.addCommand({
      id: "renumber-footnotes",
      name: "Tidy / renumber footnotes",
      icon: "list-ordered",
      editorCallback: (editor: Editor) => {
        const { result, applied } = this.withoutAutoRenumber(() => applyRenumber(editor));
        new Notice(this.tidyMessage(result, applied));
      },
    });

    this.addCommand({
      id: "fix-inline-footnotes",
      name: "Convert inline footnote typos (^[n] → [^n])",
      icon: "wand",
      editorCallback: (editor: Editor) => this.fixInlineFootnotes(editor),
    });

    this.registerEvent(
      this.app.workspace.on("editor-change", () => {
        if (this.settings.autoRenumberOnEdit && !this.applying) this.scheduleRenumber();
      })
    );

    this.addSettingTab(new FootnotesPlusSettingTab(this.app, this));
  }

  private runAutoRenumber(): void {
    const view = this.app.workspace.getActiveViewOfType(MarkdownView);
    if (!view) return;
    const editor = view.editor;
    const before = editor.getValue();
    // If the document just returned to the pre-renumber state, the user undid
    // our change — leave it be instead of fighting the undo.
    if (before === this.undoGuardText) {
      this.undoGuardText = null;
      return;
    }
    const { applied } = this.withoutAutoRenumber(() => applyRenumber(editor, true));
    this.undoGuardText = applied ? before : null;
  }

  // Suppress the edit-watcher during our own edits so a renumber can't retrigger itself.
  private withoutAutoRenumber<T>(fn: () => T): T {
    this.applying = true;
    try {
      return fn();
    } finally {
      this.applying = false;
    }
  }

  private tidyMessage(result: RenumberResult, applied: boolean): string {
    const labels = blockingLabels(result.problems);
    if (labels.length > 0) {
      return `Couldn't renumber — these footnotes don't match up: ${labels.join(
        ", "
      )}. They're underlined in the editor.`;
    }
    let message = applied ? "Footnotes renumbered." : "Footnotes already in order.";
    const typos = result.problems.filter((p: FootnoteProblem) => p.kind === "inline-typo").length;
    if (typos > 0) {
      message += ` Also found ${typos} inline typo${typos === 1 ? "" : "s"} — run "Convert inline footnote typos".`;
    }
    return message;
  }

  private fixInlineFootnotes(editor: Editor): void {
    const typos = parseFootnotes(editor.getValue()).inlineTypos;
    if (typos.length === 0) {
      new Notice("No inline footnote typos found.");
      return;
    }
    this.withoutAutoRenumber(() => {
      editor.transaction({
        changes: typos.map((t) => ({
          from: editor.offsetToPos(t.start),
          to: editor.offsetToPos(t.end),
          text: `[^${t.label}]`,
        })),
      });
      applyRenumber(editor);
    });
    new Notice(`Fixed ${typos.length} inline footnote${typos.length === 1 ? "" : "s"}.`);
  }

  async loadSettings() {
    this.settings = Object.assign({}, DEFAULT_SETTINGS, await this.loadData());
  }

  async saveSettings() {
    await this.saveData(this.settings);
  }
}
