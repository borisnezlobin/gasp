import {
  ACCENT_COLOURS,
  HIGHLIGHT_COLOURS,
  colourPair,
  type ColourPair,
  type LookChanges,
} from "./lookChanges";

export type SettingsFile = { file: string; lines: string };

type Entry = [key: string, value: string];
type Table = { name: string; entries: Entry[] };

const quoted = (text: string) => `"${text}"`;
const list = (items: string[]) => `[${items.map(quoted).join(", ")}]`;

function table(name: string, entries: (Entry | null)[]): Table {
  return { name, entries: entries.filter((entry): entry is Entry => entry !== null) };
}

function renderTable({ name, entries }: Table): string {
  const width = Math.max(...entries.map(([key]) => key.length));
  const rows = entries.map(([key, value]) => `${key.padEnd(width)} = ${value}`);
  return [`[${name}]`, ...rows].join("\n");
}

function renderFile(file: string, tables: Table[]): SettingsFile | null {
  const filled = tables.filter((each) => each.entries.length > 0);
  if (filled.length === 0) return null;
  return { file, lines: filled.map(renderTable).join("\n\n") };
}

function colourEntry(key: string, pair: ColourPair | null, mode: keyof ColourPair): Entry | null {
  return pair ? [key, quoted(pair[mode])] : null;
}

function themeFile(changes: LookChanges): SettingsFile | null {
  const accent = changes.accent ? colourPair(ACCENT_COLOURS, changes.accent) : null;
  const link = changes.link ? colourPair(ACCENT_COLOURS, changes.link) : null;
  const highlight = changes.highlight ? colourPair(HIGHLIGHT_COLOURS, changes.highlight) : null;
  const colours = (mode: keyof ColourPair) => [
    colourEntry("accent", accent, mode),
    colourEntry("link", link, mode),
    colourEntry("highlight", highlight, mode),
  ];
  return renderFile("theme.toml", [
    table("color", colours("light")),
    table("dark.color", colours("dark")),
    table("font", [changes.font ? ["text", quoted(changes.font)] : null]),
  ]);
}

function settingsFile(changes: LookChanges): SettingsFile | null {
  return renderFile("settings.toml", [
    table("appearance", [
      changes.fontSize !== undefined ? ["base-font-size", String(changes.fontSize)] : null,
      changes.appearance ? ["theme", quoted(changes.appearance)] : null,
    ]),
  ]);
}

function formattingTable(changes: LookChanges): Table {
  const bar = changes.toolbar;
  if (!bar) return table("toolbar.formatting", []);
  return table("toolbar.formatting", [
    ["place", quoted(bar.place)],
    ["surface", quoted(bar.surface)],
    ["items", list(bar.items)],
  ]);
}

function statusTable(changes: LookChanges): Table {
  return table("toolbar.status", [
    changes.statusBar === "hidden" ? ["enabled", "false"] : null,
    changes.statusWidgets ? ["items", list(["spacer", ...changes.statusWidgets])] : null,
  ]);
}

function toolbarsFile(changes: LookChanges): SettingsFile | null {
  return renderFile("toolbars.toml", [formattingTable(changes), statusTable(changes)]);
}

/** The lines the changes add to the vault's `.gasp` folder, derived from
    the validated changes alone, never from text a model wrote. */
export function settingsFiles(changes: LookChanges): SettingsFile[] {
  return [themeFile(changes), settingsFile(changes), toolbarsFile(changes)].filter(
    (file): file is SettingsFile => file !== null,
  );
}
