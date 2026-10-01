import type { ConfigPatch } from "./configPatch";
import { SCHEMA, type Replacement, type ToolbarSpec } from "./gaspSchema";
import { rulesBoundTo } from "./keyChords";

export type SettingsFile = { file: string; lines: string };

type Scalar = string | number | boolean;
type Entry = [key: string, value: string];

/** TOML's basic strings take JSON's escapes. */
const quoted = (text: string) => JSON.stringify(text);
const list = (items: string[]) => `[${items.map(quoted).join(", ")}]`;
const tomlValue = (value: Scalar | string[]) => (Array.isArray(value) ? list(value) : typeof value === "string" ? quoted(value) : String(value));

function renderEntries(entries: Entry[]): string[] {
  const width = Math.max(...entries.map(([key]) => key.length));
  return entries.map(([key, value]) => `${key.padEnd(width)} = ${value}`);
}

function renderTable(name: string, entries: Entry[]): string {
  return [`[${name}]`, ...renderEntries(entries)].join("\n");
}

/** Dotted keys grouped into the tables they belong in, such as
    `color.code.comment` into `[color.code]`, in first-seen order. */
function tablesOf(values: Record<string, Scalar>): Map<string, Entry[]> {
  const tables = new Map<string, Entry[]>();
  for (const [key, value] of Object.entries(values)) {
    const split = key.lastIndexOf(".");
    const table = key.slice(0, split);
    tables.set(table, [...(tables.get(table) ?? []), [key.slice(split + 1), tomlValue(value)]]);
  }
  return tables;
}

function renderDotted(values: Record<string, Scalar> | undefined): string | null {
  if (!values || Object.keys(values).length === 0) return null;
  return [...tablesOf(values)].map(([name, entries]) => renderTable(name, entries)).join("\n\n");
}

const isDarkKey = (key: string) => key.startsWith("dark.");

/** Light tokens first, then their dark partners, as Gasp's own file orders them. */
function themeText(theme: ConfigPatch["theme"]): string | null {
  if (!theme) return null;
  const ordered = Object.fromEntries([
    ...Object.entries(theme).filter(([key]) => !isDarkKey(key)),
    ...Object.entries(theme).filter(([key]) => isDarkKey(key)),
  ]);
  return renderDotted(ordered);
}

const TOOLBAR_FIELD_ORDER: (keyof ToolbarSpec)[] = ["title", "enabled", "place", "behaviour", "contexts", "style", "density", "surface", "items"];

function toolbarTable(id: string, spec: ToolbarSpec): string {
  const entries: Entry[] = TOOLBAR_FIELD_ORDER.filter((field) => spec[field] !== undefined).map((field) => [
    field,
    tomlValue(spec[field] as Scalar | string[]),
  ]);
  return renderTable(`toolbar.${id}`, entries);
}

function toolbarsText(patch: ConfigPatch): string | null {
  const parts = [
    patch.timing ? renderTable("timing", Object.entries(patch.timing).map(([key, value]) => [key, quoted(value)])) : null,
    ...Object.entries(patch.toolbars ?? {}).map(([id, spec]) => toolbarTable(id, spec)),
  ].filter((part): part is string => part !== null);
  return parts.length ? parts.join("\n\n") : null;
}

/** One id per chord, so binding a chord again replaces the earlier rule. */
const customRuleId = (chord: string) => `key.custom.${chord.toLowerCase().replaceAll("+", "-")}`;

function ruleBlock(entries: Entry[]): string {
  return ["[[rule]]", ...renderEntries(entries)].join("\n");
}

/** Each binding as a rule, after removing the built-in rules its chord
    already ran, so the new one isn't shadowed. */
function rulesText(keys: ConfigPatch["keys"]): string | null {
  if (!keys?.length) return null;
  const removed = [...new Set(keys.flatMap(({ keys: chord }) => rulesBoundTo(chord).map((rule) => rule.id)))];
  const blocks = [
    ...removed.map((id) => ruleBlock([["id", quoted(id)], ["delete", "true"]])),
    ...keys.map(({ keys: chord, command }) =>
      ruleBlock([
        ["id", quoted(customRuleId(chord))],
        ["on", quoted("key")],
        ["keys", quoted(chord)],
        ["do", quoted(command)],
      ]),
    ),
  ];
  return blocks.join("\n\n");
}

/** A vault's replacements.toml replaces the built-in table, so it starts
    from a copy of Gasp's own and adds the new ones. */
function replacementsText(added: ConfigPatch["replacements"]): string | null {
  if (!added?.length) return null;
  const replacing = new Map(added.map((each) => [each.from, each.to]));
  const builtIn: Replacement[] = SCHEMA.replacements.map((each) => ({ ...each, to: replacing.get(each.from) ?? each.to }));
  const fresh = added.filter((each) => !SCHEMA.replacements.some((builtInOne) => builtInOne.from === each.from));
  const rows = [...builtIn, ...fresh].map((each: Replacement) => {
    const entries: Entry[] = [["from", quoted(each.from)], ["to", quoted(each.to)]];
    if (each.enabled === false) entries.push(["enabled", "false"]);
    return ["[[replacement]]", ...renderEntries(entries)].join("\n");
  });
  return rows.join("\n\n");
}

/** The files a patch writes in the vault's `.gasp` folder, derived from the
    validated patch alone, never from text a model wrote. */
export function settingsFiles(patch: ConfigPatch): SettingsFile[] {
  const files: [string, string | null][] = [
    ["theme.toml", themeText(patch.theme)],
    ["settings.toml", renderDotted(patch.settings)],
    ["toolbars.toml", toolbarsText(patch)],
    ["rules.toml", rulesText(patch.keys)],
    ["replacements.toml", replacementsText(patch.replacements)],
  ];
  return files.filter((entry): entry is [string, string] => entry[1] !== null).map(([file, lines]) => ({ file, lines }));
}
