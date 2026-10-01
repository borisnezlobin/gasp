import { SCHEMA, type KeyRule } from "./gaspSchema";

/** Key chords as `rules.toml` writes them, such as `Mod+Shift+D`, read the
    way `crates/config/src/keys.rs` reads them. */

const MODIFIER_ORDER = ["Mod", "Ctrl", "Alt", "Shift", "Cmd"] as const;
type Modifier = (typeof MODIFIER_ORDER)[number];

/** Each name keys.rs accepts, and the symbols people type, to one modifier.
    Cmd becomes Mod, which is Cmd on a Mac and Ctrl elsewhere, so a binding
    asked for on a Mac works on every machine the vault syncs to. */
const MODIFIER_NAMES: Record<string, Modifier> = {
  mod: "Mod",
  cmd: "Mod",
  command: "Mod",
  "⌘": "Mod",
  ctrl: "Ctrl",
  control: "Ctrl",
  "⌃": "Ctrl",
  alt: "Alt",
  option: "Alt",
  opt: "Alt",
  "⌥": "Alt",
  shift: "Shift",
  "⇧": "Shift",
  win: "Cmd",
  super: "Cmd",
  meta: "Cmd",
};

const NAMED_KEYS = new Map(SCHEMA.keys.named.map((name) => [name.toLowerCase(), name]));
NAMED_KEYS.set("return", "Enter");
NAMED_KEYS.set("esc", "Escape");

function keyName(text: string): string | undefined {
  if ([...text].length === 1) return text.toUpperCase();
  const named = NAMED_KEYS.get(text.toLowerCase());
  if (named) return named;
  const functionKey = text.match(/^f([1-9]|1\d|2[0-4])$/i);
  return functionKey ? `F${functionKey[1]}` : undefined;
}

/** Splits `⌘⇧D` into `⌘+⇧+D`, so typed symbols read like names. */
const spreadSymbols = (text: string) => text.replace(/([⌘⌃⌥⇧])(?!\+)/g, "$1+");

function splitChord(text: string): { modifiers: string[]; key: string } | null {
  const spread = spreadSymbols(text.trim());
  if (spread.endsWith("++")) return { modifiers: spread.slice(0, -2).split("+").filter(Boolean), key: "+" };
  const parts = spread.split("+").map((part) => part.trim());
  const key = parts.pop();
  return key ? { modifiers: parts.filter(Boolean), key } : null;
}

/** The chord in its one written form, or null when keys.rs wouldn't read it. */
export function normalizeChord(text: unknown): string | null {
  if (typeof text !== "string" || text.length > 40) return null;
  const parts = splitChord(text);
  if (!parts) return null;
  const modifiers = parts.modifiers.map((name) => MODIFIER_NAMES[name.toLowerCase()]);
  const key = keyName(parts.key);
  if (!key || modifiers.some((modifier) => !modifier)) return null;
  const ordered = MODIFIER_ORDER.filter((modifier) => modifiers.includes(modifier));
  if (ordered.length !== new Set(modifiers).size || ordered.length !== modifiers.length) return null;
  return [...ordered, key].join("+");
}

const MAC_SYMBOLS: Record<string, string> = { Mod: "⌘", Ctrl: "⌃", Alt: "⌥", Shift: "⇧", Cmd: "⌘" };
const MAC_ORDER = ["Ctrl", "Alt", "Shift", "Mod", "Cmd"];
const KEY_SYMBOLS: Record<string, string> = {
  Enter: "↩",
  Backspace: "⌫",
  Delete: "⌦",
  Escape: "⎋",
  Tab: "⇥",
  Left: "←",
  Right: "→",
  Up: "↑",
  Down: "↓",
  Space: "Space",
};

/** A chord as a Mac menu shows it, such as ⇧⌘D. */
export function macChord(chord: string): string {
  const parts = chord === "+" ? ["+"] : chord.endsWith("++") ? [...chord.slice(0, -2).split("+"), "+"] : chord.split("+");
  const key = parts.pop() ?? "";
  const modifiers = MAC_ORDER.filter((modifier) => parts.includes(modifier)).map((modifier) => MAC_SYMBOLS[modifier]);
  return [...modifiers, KEY_SYMBOLS[key] ?? key].join("");
}

const asMacOs = (chord: string) => chord.replace(/^Mod\+/, "Cmd+").replace("+Mod+", "+Cmd+");
const RESERVED_ON_MAC = new Set(SCHEMA.keys.reserved.macos.map((chord) => normalizeChord(chord.replaceAll("Cmd", "Meta"))));

/** Whether macOS keeps the chord for itself, as keymap.rs's table says. */
export function reservedOnMac(chord: string): boolean {
  const asMeta = normalizeChord(asMacOs(chord).replaceAll("Cmd", "Meta"));
  return RESERVED_ON_MAC.has(asMeta);
}

const DESKTOP_MAC_PLATFORMS = new Set([undefined, "macos", "apple", "desktop"]);

function onMac(rule: KeyRule): boolean {
  return DESKTOP_MAC_PLATFORMS.has(rule.platform);
}

/** The built-in rules a chord already triggers, which a new binding replaces. */
export function rulesBoundTo(chord: string): KeyRule[] {
  return SCHEMA.keys.rules.filter((rule) => normalizeChord(rule.keys) === chord);
}

/** The chord that runs a command on a Mac by default, if any. */
export function defaultShortcut(command: string): string | undefined {
  const rule = SCHEMA.keys.rules.find((each) => each.command === command && onMac(each));
  return rule ? normalizeChord(rule.keys) ?? undefined : undefined;
}
