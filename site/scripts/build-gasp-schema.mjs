// Builds src/lib/gaspConfigSchema.json and src/lib/gaspIcons.json from
// Gasp's real defaults and source, so the "Change anything" demo accepts
// exactly the keys the app reads. Run with `npm run schema`.

import { readdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { flatten, parseToml } from "./toml-lite.mjs";
import { commandsIn, enumsIn, lookupNames, reservedChords, structsIn } from "./rust-source.mjs";

const SITE = join(dirname(fileURLToPath(import.meta.url)), "..");
const REPO = join(SITE, "..");
const read = (path) => readFileSync(join(REPO, path), "utf8");
const DEFAULTS = "crates/config/defaults";

/** Where the desktop app and the iPhone bridge read theme tokens. */
const TOKEN_READERS = ["apps/desktop/src", "crates/ffi/src", "crates/highlight/src"];

/** Token groups the app has no reader for, with the reason the demo leaves them out. */
const UNREAD_GROUPS = {
  shadow: "declared, but the app draws its shadows from colour tokens and fixed blurs",
  curve: "declared, but the app's animations use fixed curves",
  duration: "declared, but the app's animations use fixed times",
  animation: "declared, but the app's animations use fixed times and curves",
  theme: "says which mode the file describes, which the demo doesn't change",
};

const ALWAYS_LEFT_OUT = new Set(["theme"]);

const COLOUR = { type: "colour" };
const NUMBER_RULES = [
  [/^opacity\./, { type: "number", min: 0, max: 1 }],
  [/^font\.weight\./, { type: "number", min: 100, max: 900, step: 100 }],
  [/^font\.scale\./, { type: "number", min: 0.5, max: 4 }],
  [/^font\.(line-height\.|heading\.line-height)/, { type: "number", min: 1, max: 3 }],
  [/^font\.heading\.space-above/, { type: "number", min: 0, max: 3 }],
  [/^table\.min-(column-width|row-height)/, { type: "number", min: 1, max: 20 }],
  [/^embed\.max-lines/, { type: "number", min: 1, max: 200, step: 1 }],
  [/^radius\.pill/, { type: "number", min: 0, max: 999 }],
  [/^size\.editor-max-width/, { type: "number", min: 320, max: 2000 }],
  [/^tour\.(breach-rise|swim|step-enter|press)/, { type: "number", min: 0, max: 5000 }],
];

function pixelRule(value) {
  const base = typeof value === "number" ? value : 8;
  return { type: "number", min: 0, max: Math.max(64, Math.ceil(base * 4)) };
}

function tokenKind(key, value) {
  if (key.startsWith("color.")) return COLOUR;
  if (/^font\.(text|ui|code)$/.test(key)) return { type: "font" };
  const rule = NUMBER_RULES.find(([pattern]) => pattern.test(key));
  return rule ? rule[1] : pixelRule(value);
}

function rustFiles(directory) {
  return readdirSync(join(REPO, directory), { withFileTypes: true, recursive: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .map((entry) => readFileSync(join(entry.parentPath, entry.name), "utf8"));
}

/** Token names the app's source spells out, and the prefixes it builds names from. */
function tokensTheAppReads() {
  const source = TOKEN_READERS.flatMap(rustFiles).join("\n");
  const names = new Set([...source.matchAll(/"([a-z]+(?:\.[a-z0-9-]+)+)"/g)].map((match) => match[1]));
  const prefixes = [...source.matchAll(/format!\("([a-z]+\.[a-z0-9.-]*)\{/g)].map((match) => match[1]);
  return { names, prefixes };
}

const referencesIn = (value) =>
  typeof value === "string" ? [...value.matchAll(/\{([a-z0-9.-]+)\}/g)].map((match) => match[1]) : [];

/** Tokens that change something on screen: the ones read, and every token those refer to. */
function effectiveTokens(light, dark) {
  const { names, prefixes } = tokensTheAppReads();
  const isRead = (key) => names.has(key) || prefixes.some((prefix) => key.startsWith(prefix));
  const effective = new Set(Object.keys(light).filter(isRead));
  const queue = [...effective];
  while (queue.length > 0) {
    const key = queue.pop();
    for (const reference of [...referencesIn(light[key]), ...referencesIn(dark[key])]) {
      if (effective.has(reference) || !(reference in light)) continue;
      effective.add(reference);
      queue.push(reference);
    }
  }
  return effective;
}

function themeSchema() {
  const all = Object.fromEntries(flatten(parseToml(read(`${DEFAULTS}/theme.toml`))));
  const light = Object.fromEntries(Object.entries(all).filter(([key]) => !key.startsWith("dark.")));
  const dark = Object.fromEntries(
    Object.entries(all)
      .filter(([key]) => key.startsWith("dark."))
      .map(([key, value]) => [key.slice("dark.".length), value]),
  );
  const effective = effectiveTokens(light, dark);
  const tokens = [];
  const leftOut = [];
  for (const [key, value] of Object.entries(light)) {
    const group = key.split(".")[0];
    if (!effective.has(key) || ALWAYS_LEFT_OUT.has(group)) {
      leftOut.push({ key, reason: UNREAD_GROUPS[group] ?? "declared, but nothing in the app reads it" });
      continue;
    }
    const token = { key, ...tokenKind(key, value), light: value };
    if (key in dark) token.dark = dark[key];
    tokens.push(token);
  }
  return { tokens, leftOut };
}

const SETTING_BOUNDS = {
  "appearance.base-font-size": [8, 32],
  "prose.sentence-length.short-below": [1, 60],
  "prose.sentence-length.long-above": [1, 120],
  "recovery.interval-minutes": [1, 120],
  "recovery.keep-days": [1, 365],
  "sync.interval-minutes": [1, 1440],
};

/** Free-text settings the demo may write, all short names and patterns. */
const TEXT_SETTINGS = new Set([
  "files.attachments-folder",
  "daily-notes.folder",
  "daily-notes.format",
  "daily-notes.template",
  "templates.folder",
  "templates.date-format",
  "templates.time-format",
]);

function scalarSetting(key, type, enums) {
  if (type === "bool") return { type: "boolean" };
  if (/^u(8|16|32|64)$/.test(type)) {
    const [min, max] = SETTING_BOUNDS[key] ?? [0, 1000];
    return { type: "integer", min, max };
  }
  if (type === "String") return TEXT_SETTINGS.has(key) ? { type: "text", maxLength: 60 } : null;
  if (enums[type]) return { type: "enum", values: enums[type] };
  return null;
}

function mapSetting(type, enums) {
  const match = type.match(/^BTreeMap<(\w+), (\w+)>$/);
  if (!match || !enums[match[1]] || !enums[match[2]]) return null;
  return { type: "enum-map", keys: enums[match[1]], values: enums[match[2]] };
}

function walkSettings(structName, prefix, context, out) {
  for (const field of context.structs[structName] ?? []) {
    const key = prefix ? `${prefix}.${field.name}` : field.name;
    if (context.structs[field.type]) {
      walkSettings(field.type, key, context, out);
      continue;
    }
    const kind = scalarSetting(key, field.type, context.enums) ?? mapSetting(field.type, context.enums);
    if (!kind) {
      context.leftOut.push({ key, reason: `a ${field.type} the demo doesn't write` });
      continue;
    }
    out.push({ key, ...kind, default: context.defaults[key] ?? null, note: field.doc });
  }
}

function settingsSchema() {
  const source = read("crates/config/src/settings.rs");
  const defaults = Object.fromEntries(flatten(parseToml(read(`${DEFAULTS}/settings.toml`))));
  const context = { structs: structsIn(source), enums: enumsIn(source), defaults, leftOut: [] };
  const settings = [];
  walkSettings("Settings", "", context, settings);
  return { settings, leftOut: context.leftOut };
}

function toolbarsSchema() {
  const source = read("crates/config/src/toolbars.rs");
  const enums = enumsIn(source);
  const defaults = parseToml(read(`${DEFAULTS}/toolbars.toml`));
  return {
    places: enums.Place,
    desktopPlaces: enums.Place.filter((place) => !["keyboard", "browser-bar"].includes(place)),
    behaviours: enums.Behaviour,
    contexts: enums.ToolbarContext,
    styles: enums.ButtonStyle,
    densities: enums.Density,
    surfaces: enums.Surface,
    widgets: [...source.matchAll(/\(\s*Widget::\w+,\s*"([^"]+)"/g)].map((match) => match[1]),
    timing: defaults.timing,
    builtIn: defaults.toolbar,
    menus: defaults.menu,
  };
}

function keyRules() {
  const rules = parseToml(read(`${DEFAULTS}/rules.toml`)).rule;
  return rules
    .filter((rule) => rule.on === "key")
    .map(({ id, keys, do: command, platform }) => (platform ? { id, keys, command, platform } : { id, keys, command }));
}

function keysSchema() {
  const keys = read("crates/config/src/keys.rs");
  return {
    modifiers: lookupNames(keys, "MODIFIER_NAMES"),
    named: lookupNames(keys, "NAMED_KEYS"),
    reserved: { macos: reservedChords(read("crates/config/src/keymap.rs"), "Macos") },
    rules: keyRules(),
  };
}

function replacementsSchema() {
  return parseToml(read("crates/snippets/defaults/replacements.toml")).replacement;
}

const PHOSPHOR = join(SITE, "node_modules/@phosphor-icons/react/dist/defs");
const pascal = (name) => name.replace(/(^|-)([a-z0-9])/g, (_, __, letter) => letter.toUpperCase());

function regularBlock(source) {
  const start = source.indexOf('"regular"');
  const end = source.indexOf('"thin"', start);
  return source.slice(start, end < 0 ? undefined : end);
}

const APP_ICONS = join(REPO, "apps/desktop/assets/icons");
const camel = (name) => name.replace(/-([a-z])/g, (_, letter) => letter.toUpperCase());

/** The shapes of the SVG the desktop app bundles for this icon, as [tag, attributes] pairs. */
function appIconShapes(name) {
  const file = join(APP_ICONS, `${name}.svg`);
  if (!existsSync(file)) return null;
  const svg = readFileSync(file, "utf8");
  return [...svg.matchAll(/<(path|circle|rect|line|polyline|polygon)\s([^>]*?)\/?>/g)].map(([, tag, body]) => [
    tag,
    Object.fromEntries([...body.matchAll(/([\w-]+)="([^"]*)"/g)].map(([, key, value]) => [camel(key), value])),
  ]);
}

/** The regular weight's shapes as [tag, attributes] pairs, read from Phosphor's own icon module. */
function phosphorShapes(name) {
  const file = join(PHOSPHOR, `${pascal(name)}.es.js`);
  if (!existsSync(file)) return null;
  const block = regularBlock(readFileSync(file, "utf8"));
  return [...block.matchAll(/createElement\(\s*"(\w+)",\s*\{([^}]*)\}/g)].map(([, tag, body]) => [
    tag,
    Object.fromEntries([...body.matchAll(/(\w+):\s*"([^"]*)"/g)].map(([, key, value]) => [key, value])),
  ]);
}

function iconsFor(names) {
  const icons = {};
  const missing = [];
  for (const name of [...new Set(names)].sort()) {
    const shapes = appIconShapes(name) ?? phosphorShapes(name);
    if (shapes?.length) icons[name] = shapes;
    else missing.push(name);
  }
  return { icons, missing };
}

const WIDGET_ICONS = ["text-aa", "text-t", "book-open", "clock", "cursor-text", "cloud-check"];
const CHROME_ICONS = ["lightning", "sidebar-simple", "sidebar-simple-right", "plus", "caret-down", "caret-right", "x", "arrow-left", "arrow-right", "book-open", "dots-three", "file-text", "folder-simple", "check", "pencil-simple", "lightbulb", "dots-three-vertical", "keyboard", "magnifying-glass"];

function build() {
  const theme = themeSchema();
  const settings = settingsSchema();
  const commands = commandsIn(read("crates/config/src/commands.rs"));
  const toolbars = toolbarsSchema();
  const menuIcons = Object.values(toolbars.menus ?? {}).map((menu) => menu.icon);
  const { icons, missing } = iconsFor([...commands.map((command) => command.icon), ...WIDGET_ICONS, ...CHROME_ICONS, ...menuIcons]);
  const schema = {
    source: "Generated by scripts/build-gasp-schema.mjs from crates/config and crates/snippets. Don't edit by hand.",
    theme: theme.tokens,
    settings: settings.settings,
    toolbars,
    commands,
    keys: keysSchema(),
    replacements: replacementsSchema(),
    leftOut: [...theme.leftOut.map((each) => ({ file: "theme.toml", ...each })), ...settings.leftOut.map((each) => ({ file: "settings.toml", ...each })), { file: "layout.toml", key: "*", reason: "declared, but the desktop app doesn't build its window from it yet" }],
  };
  writeFileSync(join(SITE, "src/lib/gaspConfigSchema.json"), `${JSON.stringify(schema, null, 1)}\n`);
  writeFileSync(join(SITE, "src/lib/gaspIcons.json"), `${JSON.stringify(icons)}\n`);
  const counts = `${schema.theme.length} theme tokens, ${schema.settings.length} settings, ${commands.length} commands, ${schema.keys.rules.length} key rules, ${Object.keys(icons).length} icons`;
  console.log(`Wrote the schema: ${counts}.`);
  if (missing.length) console.warn(`No Phosphor icon for: ${missing.join(", ")}`);
}

build();
