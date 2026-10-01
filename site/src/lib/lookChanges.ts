/** The changes the "Change anything" demo can make to Gasp's look, each
    mapped to a real key in `crates/config/defaults/`. Shared by the route
    that validates a model's answer and the window that shows it. */

export type ColourPair = { light: string; dark: string };

/** Named colours a request can use. Each has a value for light mode and a
    lighter one for dark mode, written to `[color]` and `[dark.color]`. */
export const ACCENT_COLOURS: Record<string, ColourPair> = {
  green: { light: "#1f7a4d", dark: "#5cc393" },
  blue: { light: "#0868d3", dark: "#5ea4f2" },
  teal: { light: "#007876", dark: "#44cfcb" },
  purple: { light: "#744dee", dark: "#a88ef8" },
  pink: { light: "#c2185b", dark: "#f48fb1" },
  red: { light: "#d1162d", dark: "#f1707d" },
  orange: { light: "#aa5400", dark: "#f0a054" },
  yellow: { light: "#8a6d00", dark: "#e8c547" },
  brown: { light: "#7a4f2a", dark: "#c99a6e" },
  gray: { light: "#52525b", dark: "#aaa59d" },
  black: { light: "#000000", dark: "#ebe7e0" },
};

export const HIGHLIGHT_COLOURS: Record<string, ColourPair> = {
  yellow: { light: "#fff59d", dark: "#4f4418" },
  green: { light: "#c8f2d4", dark: "#1f4a2f" },
  blue: { light: "#cfe6ff", dark: "#1d3550" },
  pink: { light: "#fcd5e5", dark: "#4d2236" },
  orange: { light: "#ffe0b8", dark: "#4d3318" },
  purple: { light: "#e4d9ff", dark: "#352a55" },
  gray: { light: "#e4e4e7", dark: "#3d3a35" },
};

/** Families a Mac has, with what the page falls back to elsewhere. */
export const FONTS: Record<string, string> = {
  Charter: 'Charter, "Bitstream Charter", var(--font-charis), Georgia, serif',
  "Helvetica Neue": '"Helvetica Neue", Helvetica, Arial, sans-serif',
  "Avenir Next": '"Avenir Next", Avenir, "Segoe UI", sans-serif',
  Georgia: "Georgia, serif",
  "New York": '"New York", ui-serif, Georgia, serif',
  "Iowan Old Style": '"Iowan Old Style", Palatino, "Palatino Linotype", serif',
  Menlo: "Menlo, ui-monospace, monospace",
};

export const APPEARANCES = ["light", "dark", "match-system"] as const;
export type Appearance = (typeof APPEARANCES)[number];

export const TOOLBAR_PLACES = ["editor-top", "editor-bottom"] as const;
export type ToolbarPlace = (typeof TOOLBAR_PLACES)[number];

export const TOOLBAR_SURFACES = ["overlay", "strip"] as const;
export type ToolbarSurface = (typeof TOOLBAR_SURFACES)[number];

export const TOOLBAR_ITEMS = [
  "format.bold",
  "format.italic",
  "format.underline",
  "format.strikethrough",
  "format.highlight",
  "format.code",
  "format.link",
  "format.bullet-list",
  "format.numbered-list",
  "edit.toggle-task",
  "separator",
] as const;
export type ToolbarItem = (typeof TOOLBAR_ITEMS)[number];

export const STATUS_WIDGETS = [
  "word-count",
  "character-count",
  "reading-time",
  "edit-time",
  "cursor-position",
  "sync",
] as const;
export type StatusWidget = (typeof STATUS_WIDGETS)[number];

export const FONT_SIZE = { min: 10, max: 24, default: 12 };
const MAX_TOOLBAR_ITEMS = 8;
export const MAX_REPLY_LENGTH = 120;

export type Toolbar = { place: ToolbarPlace; surface: ToolbarSurface; items: ToolbarItem[] };

/** A colour is a name from its palette or a `#rrggbb` hex. */
export type Colour = string;

export type LookChanges = {
  accent?: Colour;
  link?: Colour;
  highlight?: Colour;
  font?: string;
  fontSize?: number;
  appearance?: Appearance;
  toolbar?: Toolbar | null;
  statusWidgets?: StatusWidget[];
  statusBar?: "shown" | "hidden";
};

export const DEFAULT_TOOLBAR_ITEMS: ToolbarItem[] = ["format.bold", "format.italic", "format.highlight", "format.link"];

const HEX = /^#[0-9a-f]{6}$/;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function oneOf<T extends string>(allowed: readonly T[], value: unknown): T | undefined {
  return allowed.find((option) => option === value);
}

function colourIn(palette: Record<string, ColourPair>, value: unknown): Colour | undefined {
  if (typeof value !== "string") return undefined;
  const colour = value.trim().toLowerCase().replace("grey", "gray");
  if (Object.hasOwn(palette, colour) || HEX.test(colour)) return colour;
  return undefined;
}

export function colourPair(palette: Record<string, ColourPair>, colour: Colour): ColourPair {
  return palette[colour] ?? { light: colour, dark: colour };
}

function fontFamily(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const wanted = value.trim().toLowerCase();
  return Object.keys(FONTS).find((family) => family.toLowerCase() === wanted);
}

function fontSize(value: unknown): number | undefined {
  if (typeof value !== "number" || !Number.isFinite(value)) return undefined;
  return Math.min(FONT_SIZE.max, Math.max(FONT_SIZE.min, Math.round(value)));
}

function listOf<T extends string>(allowed: readonly T[], value: unknown, limit: number): T[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const kept = value.map((item) => oneOf(allowed, item)).filter((item): item is T => item !== undefined);
  return [...new Set(kept)].slice(0, limit);
}

function toolbar(value: unknown): Toolbar | null | undefined {
  if (value === false || value === "none") return null;
  if (!isRecord(value)) return undefined;
  const items = trimSeparators(listOf(TOOLBAR_ITEMS, value.items, MAX_TOOLBAR_ITEMS) ?? []);
  return {
    place: oneOf(TOOLBAR_PLACES, value.place) ?? "editor-top",
    surface: oneOf(TOOLBAR_SURFACES, value.surface) ?? "overlay",
    items: items.length ? items : DEFAULT_TOOLBAR_ITEMS,
  };
}

function trimSeparators(items: ToolbarItem[]): ToolbarItem[] {
  const isButton = (item: ToolbarItem) => item !== "separator";
  const first = items.findIndex(isButton);
  if (first < 0) return [];
  return items.slice(first, items.findLastIndex(isButton) + 1);
}

function statusBar(value: unknown): "shown" | "hidden" | undefined {
  return oneOf(["shown", "hidden"] as const, value);
}

const READERS: { [Key in keyof LookChanges]-?: (value: unknown) => LookChanges[Key] } = {
  accent: (value) => colourIn(ACCENT_COLOURS, value),
  link: (value) => colourIn(ACCENT_COLOURS, value),
  highlight: (value) => colourIn(HIGHLIGHT_COLOURS, value),
  font: fontFamily,
  fontSize,
  appearance: (value) => oneOf(APPEARANCES, value),
  toolbar,
  statusWidgets: (value) => listOf(STATUS_WIDGETS, value, STATUS_WIDGETS.length),
  statusBar,
};

/** Keeps only what the schema allows: unknown keys and invalid values are
    dropped, and sizes and lists are clamped. */
export function parseChanges(value: unknown): LookChanges {
  if (!isRecord(value)) return {};
  const changes: Record<string, unknown> = {};
  for (const [key, read] of Object.entries(READERS)) {
    const parsed = (read as (input: unknown) => unknown)(value[key]);
    if (parsed !== undefined) changes[key] = parsed;
  }
  return changes as LookChanges;
}

/** A short plain-text line, or undefined when there's nothing usable. */
export function parseReply(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const line = value.replace(/[\u0000-\u001f\u007f<>]/g, " ").replace(/\s+/g, " ").trim();
  if (!line) return undefined;
  return line.length > MAX_REPLY_LENGTH ? `${line.slice(0, MAX_REPLY_LENGTH - 1).trimEnd()}…` : line;
}

export function hasChanges(changes: LookChanges): boolean {
  return Object.keys(changes).length > 0;
}

/** Later changes replace earlier ones key by key, so requests build up. */
export function mergeChanges(current: LookChanges, next: LookChanges): LookChanges {
  return { ...current, ...next };
}
