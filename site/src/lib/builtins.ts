import { defaultShortcut, macChord } from "./keyChords";

/** What the demo window does to show a built-in feature. */
export type BuiltinDemo = "fold" | "palette" | "switcher" | "search" | "link-preview" | "outline" | "math" | "table" | "sync";

/** Something Gasp already does with no setting to change. `asks` is the
    request it answers, for the model; `answer` is what the visitor reads,
    where `{key}` becomes the command's default shortcut. */
export type Builtin = { asks: string; command?: string; answer: string; demo?: BuiltinDemo };

export const BUILTINS = {
  fold: {
    asks: "collapsible, folding or hiding headings and sections",
    command: "fold.toggle",
    answer: "Headings already fold in Gasp: click the arrow beside one, or press {key}.",
    demo: "fold",
  },
  palette: {
    asks: "a command palette, running any command by name",
    command: "palette.open",
    answer: "Press {key} for the command palette, which runs every command by name.",
    demo: "palette",
  },
  switcher: {
    asks: "quick open, jumping to a note by name",
    command: "switcher.open",
    answer: "Press {key} to jump to any note by typing part of its name.",
    demo: "switcher",
  },
  search: {
    asks: "searching every note, full-text search",
    command: "search.open",
    answer: "Press {key} to search every note in the vault as you type.",
    demo: "search",
  },
  math: {
    asks: "LaTeX, equations or math",
    command: "format.math-inline",
    answer: "Gasp draws LaTeX math in place as you type, and {key} turns a selection into math.",
    demo: "math",
  },
  tables: {
    asks: "tables or a spreadsheet grid",
    command: "table.insert",
    answer: "Tables already edit as a grid: Tab moves between cells, and Insert table in the palette adds one.",
    demo: "table",
  },
  "link-preview": {
    asks: "previewing links on hover, wikilinks, following links",
    command: "link.follow",
    answer: "Hover a link to preview the note it points to, and {key} follows it.",
    demo: "link-preview",
  },
  outline: {
    asks: "an outline or table of contents",
    command: "sidebar.outline",
    answer: "Press {key} to show the note's outline beside it.",
    demo: "outline",
  },
  backlinks: {
    asks: "backlinks, notes that link here",
    command: "sidebar.backlinks",
    answer: "Press {key} to list every note that links to this one.",
  },
  daily: { asks: "daily notes or a journal", command: "daily.open", answer: "Press {key} to open today's daily note." },
  footnotes: {
    asks: "footnotes",
    command: "footnote.insert-or-jump",
    answer: "Press {key} to add a numbered footnote, or to jump between one and its text.",
  },
  templates: { asks: "templates", command: "template.insert", answer: "Press {key} to start a note from a template." },
  export: { asks: "exporting to PDF or HTML, printing", command: "app.export", answer: "Press {key} to export the note as HTML or PDF." },
  split: { asks: "split view, two notes side by side", command: "pane.split-right", answer: "Press {key} to split the window and show two notes side by side." },
  history: {
    asks: "version history, recovering an older version",
    command: "note.recover",
    answer: "Gasp keeps snapshots as you write, and Recover a previous version in the palette brings one back.",
  },
  "look-up": { asks: "a dictionary, looking up a word", command: "edit.look-up", answer: "Press {key} to look up the word at the cursor in the dictionary." },
  emoji: { asks: "emoji", answer: "Type a colon and a letter, such as :smi, and Gasp offers emoji to pick from." },
  snippets: {
    asks: "text expansion, snippets, math snippets",
    answer: "Snippets already expand as you type, such as mk for inline math, and your agent can add more to .gasp/snippets.txt.",
  },
  agents: {
    asks: "letting AI agents such as Claude or ChatGPT read or edit notes, MCP",
    answer: "Run gasp mcp, and agents like Claude can read and edit your notes and every setting.",
  },
  sync: {
    asks: "syncing between devices, backup, iCloud or GitHub",
    command: "sync.now",
    answer: "Gasp syncs through iCloud or GitHub on its own, and {key} syncs right away.",
    demo: "sync",
  },
  obsidian: {
    asks: "Obsidian vaults or Obsidian settings",
    command: "vault.import-obsidian",
    answer: "Gasp opens an Obsidian vault as it is, and Import settings from Obsidian in the palette brings its settings over.",
  },
  shortcuts: { asks: "a list of keyboard shortcuts", command: "help.shortcuts", answer: "Show keyboard shortcuts in the palette lists every shortcut, and holding ⌘ shows them too." },
} satisfies Record<string, Builtin>;

export type BuiltinId = keyof typeof BUILTINS;

export const BUILTIN_IDS = Object.keys(BUILTINS) as BuiltinId[];

export function isBuiltinId(value: unknown): value is BuiltinId {
  return typeof value === "string" && Object.hasOwn(BUILTINS, value);
}

/** The answer a visitor reads, with the command's real default shortcut. */
export function builtinAnswer(id: BuiltinId): string {
  const builtin: Builtin = BUILTINS[id];
  const chord = builtin.command ? defaultShortcut(builtin.command) : undefined;
  return builtin.answer.replace("{key}", chord ? macChord(chord) : "the command palette");
}

export function builtinDemo(id: BuiltinId): BuiltinDemo | undefined {
  return (BUILTINS[id] as Builtin).demo;
}
