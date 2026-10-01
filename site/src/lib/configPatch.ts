import { REFERENCE, darkTwin, formatColour, parseColour } from "./colours";
import { fontFamily } from "./gaspFonts";
import {
  COMMANDS,
  DARK_PREFIX,
  SCHEMA,
  SETTINGS,
  THEME_TOKENS,
  TOOLBARS,
  type SettingSpec,
  type SettingValue,
  type ThemeToken,
  type ToolbarSpec,
} from "./gaspSchema";
import { normalizeChord, reservedOnMac } from "./keyChords";
import { changesInDark, colourRole, type TokenMap, type TokenValue } from "./themeTokens";

/** One shortcut a request adds to `rules.toml`. */
export type KeyBinding = { keys: string; command: string };

/** A replacement a request adds to `replacements.toml`. */
export type AddedReplacement = { from: string; to: string };

/** The changes to a vault's `.gasp/` files, validated against Gasp's real
    schema. The server writes the files from this alone. */
export type ConfigPatch = {
  theme?: TokenMap;
  settings?: Record<string, SettingValue>;
  toolbars?: Record<string, ToolbarSpec>;
  timing?: Record<string, string>;
  keys?: KeyBinding[];
  replacements?: AddedReplacement[];
};

/** What validation set aside, so the reply can say why. */
export type Refusal = { kind: "reserved-chord"; chord: string };

const MOST_TOOLBARS = 4;
const MOST_ITEMS = 16;
const MOST_BINDINGS = 6;
const MOST_REPLACEMENTS = 6;
const TOOLBAR_ID = /^[a-z0-9-]{1,24}$/;
const SAFE_TEXT = /^[\p{L}\p{N} ._\-/{}:,()]*$/u;
const DURATION = /^\d{1,4}(ms|s)$/;

type Plain = Record<string, unknown>;

export function isRecord(value: unknown): value is Plain {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

/** `{"color": {"accent": "#000"}}` and `{"color.accent": "#000"}` alike, as dotted keys. */
function dotted(value: unknown, prefix = ""): [string, unknown][] {
  if (!isRecord(value)) return prefix ? [[prefix, value]] : [];
  return Object.entries(value).flatMap(([key, child]) => dotted(child, prefix ? `${prefix}.${key}` : key));
}

function clamp(value: number, min = -Infinity, max = Infinity): number {
  return Math.min(max, Math.max(min, value));
}

function numberIn(value: unknown, bounds: { min?: number; max?: number; step?: number }): number | undefined {
  const number = typeof value === "string" ? Number(value) : value;
  if (typeof number !== "number" || !Number.isFinite(number)) return undefined;
  const step = bounds.step ?? 0.01;
  return Number((Math.round(clamp(number, bounds.min, bounds.max) / step) * step).toFixed(2));
}

function referenceOfType(value: string, type: ThemeToken["type"]): string | undefined {
  const name = value.trim().match(REFERENCE)?.[1];
  return name && THEME_TOKENS.get(name)?.type === type ? `{${name}}` : undefined;
}

function colourValue(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const colour = parseColour(value);
  return colour ? formatColour(colour) : referenceOfType(value, "colour");
}

const TOKEN_READERS: Record<ThemeToken["type"], (value: unknown, token: ThemeToken) => TokenValue | undefined> = {
  colour: colourValue,
  font: fontFamily,
  number: (value, token) =>
    typeof value === "string" && REFERENCE.test(value.trim()) ? referenceOfType(value, "number") : numberIn(value, token),
};

function themeEntry(key: string, value: unknown): [string, TokenValue] | null {
  const token = THEME_TOKENS.get(key.startsWith(DARK_PREFIX) ? key.slice(DARK_PREFIX.length) : key);
  if (!token) return null;
  const read = TOKEN_READERS[token.type](value, token);
  return read === undefined ? null : [key, read];
}

/** A light colour with no dark partner, where Gasp's dark palette would
    otherwise hide it, gets one, so the change shows in both modes. */
function withDarkTwins(theme: TokenMap): TokenMap {
  const twins: TokenMap = {};
  for (const [key, value] of Object.entries(theme)) {
    const role = colourRole(key);
    const colour = typeof value === "string" ? parseColour(value) : null;
    const needsTwin = role && colour && !(`${DARK_PREFIX}${key}` in theme) && changesInDark(key);
    if (needsTwin) twins[`${DARK_PREFIX}${key}`] = formatColour(darkTwin(colour, role));
  }
  return { ...theme, ...twins };
}

function parseTheme(value: unknown): TokenMap | undefined {
  const entries = dotted(value)
    .map(([key, child]) => themeEntry(key, child))
    .filter((entry): entry is [string, TokenValue] => entry !== null);
  return entries.length ? withDarkTwins(Object.fromEntries(entries)) : undefined;
}

function oneOf(values: readonly string[] | undefined, value: unknown): string | undefined {
  return values?.find((option) => option === value);
}

function textValue(value: unknown, maxLength = 60): string | undefined {
  if (typeof value !== "string") return undefined;
  const text = value.trim();
  return text.length <= maxLength && SAFE_TEXT.test(text) ? text : undefined;
}

const SETTING_READERS: Record<SettingSpec["type"], (value: unknown, spec: SettingSpec) => SettingValue | undefined> = {
  boolean: (value) => (typeof value === "boolean" ? value : undefined),
  integer: (value, spec) => numberIn(value, { ...spec, step: 1 }),
  enum: (value, spec) => oneOf(spec.values, value),
  text: (value, spec) => textValue(value, spec.maxLength),
  "enum-map": () => undefined,
};

const ENUM_MAP_KEYS = SCHEMA.settings.filter((spec) => spec.type === "enum-map");

/** `markdown.symbols.overrides.link-url`, one entry of a table of choices. */
function enumMapEntry(key: string, value: unknown): [string, SettingValue] | null {
  const map = ENUM_MAP_KEYS.find((spec) => key.startsWith(`${spec.key}.`));
  if (!map) return null;
  const name = oneOf(map.keys, key.slice(map.key.length + 1));
  const choice = oneOf(map.values, value);
  return name && choice ? [key, choice] : null;
}

function settingEntry(key: string, value: unknown): [string, SettingValue] | null {
  const spec = SETTINGS.get(key);
  if (!spec) return enumMapEntry(key, value);
  const read = SETTING_READERS[spec.type](value, spec);
  return read === undefined ? null : [key, read];
}

function parseSettings(value: unknown): Record<string, SettingValue> | undefined {
  const entries = dotted(value)
    .map(([key, child]) => settingEntry(key, child))
    .filter((entry): entry is [string, SettingValue] => entry !== null);
  return entries.length ? Object.fromEntries(entries) : undefined;
}

const MENU_ITEMS = Object.keys(TOOLBARS.menus).map((id) => `menu:${id}`);
const NON_COMMAND_ITEMS = new Set([...TOOLBARS.widgets, "separator", "spacer", ...MENU_ITEMS]);

export function isToolbarItem(item: unknown): item is string {
  return typeof item === "string" && (COMMANDS.has(item) || NON_COMMAND_ITEMS.has(item));
}

function itemList(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return value.filter(isToolbarItem).slice(0, MOST_ITEMS);
}

function contextList(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  return [...new Set(value.map((each) => oneOf(TOOLBARS.contexts, each)).filter((each): each is string => !!each))];
}

const TOOLBAR_FIELDS: Record<keyof ToolbarSpec, (value: unknown) => ToolbarSpec[keyof ToolbarSpec]> = {
  title: (value) => textValue(value, 30),
  enabled: (value) => (typeof value === "boolean" ? value : undefined),
  place: (value) => oneOf(TOOLBARS.places, value),
  behaviour: (value) => oneOf(TOOLBARS.behaviours, value),
  contexts: contextList,
  style: (value) => oneOf(TOOLBARS.styles, value),
  density: (value) => oneOf(TOOLBARS.densities, value),
  surface: (value) => oneOf(TOOLBARS.surfaces, value),
  items: itemList,
};

function toolbarSpec(value: unknown): ToolbarSpec | null {
  if (!isRecord(value)) return null;
  const spec: Record<string, unknown> = {};
  for (const [field, read] of Object.entries(TOOLBAR_FIELDS)) {
    const parsed = read(value[field]);
    if (parsed !== undefined) spec[field] = parsed;
  }
  return Object.keys(spec).length ? (spec as ToolbarSpec) : null;
}

function parseToolbars(value: unknown): Record<string, ToolbarSpec> | undefined {
  if (!isRecord(value)) return undefined;
  const toolbars = Object.entries(value)
    .filter(([id]) => TOOLBAR_ID.test(id))
    .map(([id, spec]) => [id, toolbarSpec(spec)] as const)
    .filter((entry): entry is readonly [string, ToolbarSpec] => entry[1] !== null)
    .slice(0, MOST_TOOLBARS);
  return toolbars.length ? Object.fromEntries(toolbars) : undefined;
}

function parseTiming(value: unknown): Record<string, string> | undefined {
  const entries = dotted(value).filter(
    ([key, child]) => Object.hasOwn(TOOLBARS.timing, key) && typeof child === "string" && DURATION.test(child),
  ) as [string, string][];
  return entries.length ? Object.fromEntries(entries) : undefined;
}

function binding(value: unknown): KeyBinding | null {
  if (!isRecord(value)) return null;
  const keys = normalizeChord(value.keys);
  const command = value.do ?? value.command;
  if (!keys || typeof command !== "string" || !COMMANDS.has(command)) return null;
  return { keys, command };
}

function parseKeys(value: unknown, refusals: Refusal[]): KeyBinding[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const bindings = value.map(binding).filter((each): each is KeyBinding => each !== null);
  for (const { keys } of bindings.filter(({ keys }) => reservedOnMac(keys))) refusals.push({ kind: "reserved-chord", chord: keys });
  const allowed = bindings.filter(({ keys }) => !reservedOnMac(keys));
  const unique = [...new Map(allowed.map((each) => [each.keys, each])).values()].slice(0, MOST_BINDINGS);
  return unique.length ? unique : undefined;
}

const CONTROL = /[\u0000-\u001f\u007f]/;

function replacementText(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const length = [...value].length;
  return length >= 1 && length <= 12 && !CONTROL.test(value) && value.trim() === value ? value : undefined;
}

function replacement(value: unknown): AddedReplacement | null {
  if (!isRecord(value)) return null;
  const from = replacementText(value.from);
  const to = replacementText(value.to);
  return from && to && from !== to ? { from, to } : null;
}

function parseReplacements(value: unknown): AddedReplacement[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const added = value.map(replacement).filter((each): each is AddedReplacement => each !== null);
  const unique = [...new Map(added.map((each) => [each.from, each])).values()].slice(0, MOST_REPLACEMENTS);
  return unique.length ? unique : undefined;
}

/** Keeps only what Gasp's schema allows: unknown keys, wrong types and
    unsafe text are dropped, and numbers are clamped to their bounds. */
export function parsePatch(value: unknown): { patch: ConfigPatch; refusals: Refusal[] } {
  const refusals: Refusal[] = [];
  if (!isRecord(value)) return { patch: {}, refusals };
  const sections: ConfigPatch = {
    theme: parseTheme(value.theme),
    settings: parseSettings(value.settings),
    toolbars: parseToolbars(value.toolbars),
    timing: parseTiming(value.timing),
    keys: parseKeys(value.keys, refusals),
    replacements: parseReplacements(value.replacements),
  };
  const patch = Object.fromEntries(Object.entries(sections).filter(([, section]) => section !== undefined)) as ConfigPatch;
  return { patch, refusals };
}

export function hasChanges(patch: ConfigPatch): boolean {
  return Object.keys(patch).length > 0;
}

function mergeToolbars(current: ConfigPatch["toolbars"], next: ConfigPatch["toolbars"]): ConfigPatch["toolbars"] {
  if (!next) return current;
  const merged = { ...current };
  for (const [id, spec] of Object.entries(next)) merged[id] = { ...merged[id], ...spec };
  return merged;
}

function mergeByKey<T>(current: T[] | undefined, next: T[] | undefined, key: (item: T) => string): T[] | undefined {
  if (!next) return current;
  return [...new Map([...(current ?? []), ...next].map((item) => [key(item), item])).values()];
}

function mergeRecords<T>(current: Record<string, T> | undefined, next: Record<string, T> | undefined) {
  return next ? { ...current, ...next } : current;
}

/** Later changes replace earlier ones key by key, so requests build up. */
export function mergePatches(current: ConfigPatch, next: ConfigPatch): ConfigPatch {
  const merged: ConfigPatch = {
    theme: mergeRecords(current.theme, next.theme),
    settings: mergeRecords(current.settings, next.settings),
    toolbars: mergeToolbars(current.toolbars, next.toolbars),
    timing: mergeRecords(current.timing, next.timing),
    keys: mergeByKey(current.keys, next.keys, (binding) => binding.keys),
    replacements: mergeByKey(current.replacements, next.replacements, (each) => each.from),
  };
  return Object.fromEntries(Object.entries(merged).filter(([, section]) => section !== undefined)) as ConfigPatch;
}
