import { BUILTINS } from "./builtins";
import { FONT_FAMILIES } from "./gaspFonts";
import { SCHEMA, TOOLBARS, type SettingSpec, type ThemeToken } from "./gaspSchema";

/** The system prompt: a compact map of Gasp's real configuration, built
    from the generated schema so it never names a key the app lacks. */

const groupOf = (key: string) => key.slice(0, key.lastIndexOf("."));
const leafOf = (key: string) => key.slice(key.lastIndexOf(".") + 1);

/** Groups the prompt leaves out: the welcome tour's sizes change nothing a
    visitor asks about. The validator still accepts them. */
const QUIET_GROUPS = new Set(["tour"]);

const NUMBER_UNITS: Record<string, string> = {
  "font.scale": "× base size",
  "font.weight": "",
  "font.line-height": "× font size",
  "font.heading": "ems",
  opacity: "0-1",
  embed: "lines",
};

function groupTokens(tokens: ThemeToken[]): Map<string, ThemeToken[]> {
  const groups = new Map<string, ThemeToken[]>();
  for (const token of tokens.filter((each) => !QUIET_GROUPS.has(each.key.split(".")[0]))) {
    const group = groupOf(token.key);
    groups.set(group, [...(groups.get(group) ?? []), token]);
  }
  return groups;
}

function numberLine(group: string, tokens: ThemeToken[]): string {
  const unit = NUMBER_UNITS[group] ?? "px";
  const values = tokens.map((token) => `${leafOf(token.key)}=${token.light}`).join(" ");
  return `${group} (${unit || "number"}): ${values}`;
}

function themeLines(): string[] {
  const lines: string[] = [];
  for (const [group, tokens] of groupTokens(SCHEMA.theme)) {
    const kind = tokens[0].type;
    if (kind === "colour") lines.push(`${group}: ${tokens.map((token) => leafOf(token.key)).join(" ")}`);
    else if (kind === "font") lines.push(`${group} (family): ${tokens.map((token) => leafOf(token.key)).join(" ")}`);
    else lines.push(numberLine(group, tokens));
  }
  return lines;
}

function settingType(spec: SettingSpec): string {
  if (spec.type === "enum") return spec.values?.join("|") ?? "";
  if (spec.type === "integer") return `integer ${spec.min}-${spec.max}`;
  if (spec.type === "enum-map") return `table of ${spec.keys?.join(",")} to ${spec.values?.join("|")}`;
  return spec.type;
}

function settingLines(): string[] {
  const booleans = SCHEMA.settings.filter((spec) => spec.type === "boolean").map((spec) => spec.key);
  const others = SCHEMA.settings.filter((spec) => spec.type !== "boolean").map((spec) => `${spec.key}: ${settingType(spec)}`);
  return [...others, `true|false: ${booleans.join(", ")}`];
}

/** Cursor movement and the iPhone's own commands, which visitors don't bind. */
const QUIET_COMMAND_GROUPS = new Set(["cursor", "select", "keyboard"]);

function commandLines(): string[] {
  const groups = new Map<string, string[]>();
  for (const { id } of SCHEMA.commands) {
    const group = id.slice(0, id.indexOf("."));
    if (QUIET_COMMAND_GROUPS.has(group)) continue;
    groups.set(group, [...(groups.get(group) ?? []), id.slice(group.length + 1)]);
  }
  return [...groups].map(([group, names]) => `${group}.: ${names.join(" ")}`);
}

const status = TOOLBARS.builtIn.status;

const toolbarLines = () => [
  `place: ${TOOLBARS.desktopPlaces.join("|")}`,
  `behaviour: ${TOOLBARS.behaviours.join("|")} (in-context takes contexts: ${TOOLBARS.contexts.join(",")})`,
  `style: ${TOOLBARS.styles.join("|")}; density: ${TOOLBARS.densities.join("|")}; surface: strip|overlay (overlay floats a pill over the note)`,
  `enabled, title, items: command ids, widgets (${TOOLBARS.widgets.join(" ")}), "separator", "spacer", ${Object.keys(TOOLBARS.menus).map((id) => `"menu:${id}"`).join(" ")}`,
  `Built-in ids: status (the status bar, items ${JSON.stringify(status.items)}), selection (off; floats over selected text). Any other id adds a bar.`,
  `timing: hover-delay, hide-delay, typing-pause as "150ms" or "1s"`,
];

const builtinLines = () => Object.entries(BUILTINS).map(([id, builtin]) => `${id}: ${builtin.asks}`);

const MAPPING_HINTS = [
  'Colours are "#rrggbb", "rgba(r, g, b, a)" or "{color.name}". A colour for dark mode alone goes under "dark.color...". Defaults: page #ffffff, window #f6f6f7, text #27272a, accent #000000; dark page #1c1b19, dark text #e2ded7, dark accent #ebe7e0.',
  "color.accent is the cursor, checked tasks and focus; links follow it unless color.link is set. color.gray-50..800 tint every surface and text at once.",
  "cosy or warm: warm paper colours for background, app-background, sidebar and the grays, a serif such as Iowan Old Style, font.line-height.body 1.75.",
  "like a terminal: appearance.theme dark, Menlo for font.text, ui and code, green text and accent, radius.* 0.",
  "like Notion: SF Pro or Helvetica Neue for text and ui, text #37352f, sidebar.files.reveal always. Like iA Writer: SF Mono text, size.editor-max-width 640.",
  "bigger margins: space.xxl (the note's padding) and a smaller size.editor-max-width. Wider text: a larger size.editor-max-width. Denser: smaller line heights and spaces.",
  'minimal or distraction-free: "toolbars":{"status":{"enabled":false}}, editor.show-inline-title false, markdown.symbols.mode always-hidden.',
  "bigger text: appearance.base-font-size (default 12). Bigger headings: font.scale.h1..h6. Rounder: radius.*.",
  'A shortcut is keys like "Mod+D" (Mod is Cmd on a Mac) or "Mod+Alt+Shift+K".',
];

const EXAMPLES = [
  '"dark mode with a green accent and a bar of bold and italic at the top" → {"settings":{"appearance.theme":"dark"},"theme":{"color.accent":"#2f8f5b"},"toolbars":{"writing":{"place":"editor-top","items":["format.bold","format.italic"]}}}',
  '"make cmd D duplicate the line" → {"keys":[{"keys":"Mod+D","do":"edit.duplicate-line"}]}',
  '"collapsible headings" → {"builtin":"fold"}',
  '"play music while I write" → {"reply":"Gasp can\'t play music."}',
];

/** The instructions, about 2,600 tokens: what can change, how fuzzy wishes
    map onto it, and the one JSON shape to answer in. */
export function buildInstructions(): string {
  return [
    "You change the Gasp notes app's configuration from a plain-words request. Reply with one JSON object and nothing else.",
    'Shape: {"theme":{token:value},"settings":{key:value},"toolbars":{id:{field:value}},"timing":{...},"keys":[{"keys":chord,"do":command}],"replacements":[{"from":"->","to":"→"}],"builtin":id,"reply":text}. Include only what the request needs.',
    "Use only the keys below, spelled exactly. Change as much as the request implies, in light and dark mode alike.",
    "",
    `THEME tokens (theme.toml). Fonts: ${FONT_FAMILIES.join(", ")}.`,
    ...themeLines(),
    "",
    "SETTINGS (settings.toml):",
    ...settingLines(),
    "",
    "TOOLBARS (toolbars.toml), fields:",
    ...toolbarLines(),
    "",
    "COMMANDS, for toolbar items and keys:",
    ...commandLines(),
    "",
    'BUILT-IN features with no setting. When the request asks for one, set "builtin" to its id:',
    ...builtinLines(),
    "",
    "HOW TO READ WISHES:",
    ...MAPPING_HINTS,
    'When part of the request is neither a key above nor a built-in, set "reply" to one short sentence, such as "Gasp can\'t play music." Never claim Gasp lacks a built-in.',
    "",
    "EXAMPLES:",
    ...EXAMPLES,
  ].join("\n");
}

export const INSTRUCTIONS = buildInstructions();
