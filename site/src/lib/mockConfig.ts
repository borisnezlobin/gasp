import type { CSSProperties } from "react";
import type { ConfigPatch } from "./configPatch";
import { fontStack } from "./gaspFonts";
import { SCHEMA, SETTINGS, TOOLBARS, type SettingValue } from "./gaspSchema";
import { layeredTokens, resolveTokens, type Mode, type TokenMap } from "./themeTokens";

/** How the demo window turns a patch into what it draws: every theme token
    becomes a CSS variable, and settings and toolbars resolve the way
    `crates/config` resolves them. */

export const tokenVariable = (key: string) => `--g-${key.replaceAll(".", "-")}`;
export const token = (key: string) => `var(${tokenVariable(key)})`;

/** A pixel token at the window's scale: one of the app's logical pixels is `--px` here. */
export const pixels = (key: string, times = 1) => `calc(${token(key)} * ${times} * var(--px))`;
export const realPixels = (count: number) => `calc(${count} * var(--px))`;

/** A font size from a `font.scale.*` token, as the app does: the base size in
    points, at 96 pixels to the inch, times the scale. */
export const fontSize = (scaleKey: string) => `calc(var(--g-base) * 1.3333 * ${token(scaleKey)} * var(--px))`;

type Appearance = "light" | "dark" | "match-system";

const COLOR_SCHEMES: Record<Appearance, CSSProperties["colorScheme"]> = {
  light: "light",
  dark: "dark",
  "match-system": "light dark",
};

export function settingValue(patch: ConfigPatch, key: string): SettingValue {
  const value = patch.settings?.[key] ?? SETTINGS.get(key)?.default;
  return value as SettingValue;
}

const TOKEN_TYPES = new Map(SCHEMA.theme.map((each) => [each.key, each.type]));

function cssValue(key: string, light: TokenMap, dark: TokenMap, appearance: Appearance): string {
  const type = TOKEN_TYPES.get(key);
  const pick = appearance === "dark" ? dark : light;
  if (type === "font") return fontStack(String(pick[key]));
  if (type !== "colour") return String(pick[key]);
  return `light-dark(${light[key]}, ${dark[key]})`;
}

/** The CSS variables and colour scheme for the window. Colours switch with
    `light-dark()` and the element's colour scheme, so following the
    system needs no script. */
export function mockStyle(patch: ConfigPatch): CSSProperties {
  const appearance = settingValue(patch, "appearance.theme") as Appearance;
  const theme = patch.theme ?? {};
  const resolved: Record<Mode, TokenMap> = {
    light: resolveTokens(layeredTokens(theme, "light")),
    dark: resolveTokens(layeredTokens(theme, "dark")),
  };
  const style: Record<string, string> = {
    colorScheme: COLOR_SCHEMES[appearance] ?? "light dark",
    "--g-base": String(settingValue(patch, "appearance.base-font-size")),
  };
  for (const { key } of SCHEMA.theme) style[tokenVariable(key)] = cssValue(key, resolved.light, resolved.dark, appearance);
  return style as CSSProperties;
}

export type ResolvedToolbar = {
  id: string;
  title: string;
  place: string;
  behaviour: string;
  contexts: string[];
  style: string;
  density: string;
  surface: string;
  items: string[];
};

/** The toolbars the desktop draws, built-in first, each with the defaults
    `resolve_toolbar` in toolbars.rs fills in. */
export function resolvedToolbars(patch: ConfigPatch): ResolvedToolbar[] {
  const ids = [...new Set([...Object.keys(TOOLBARS.builtIn), ...Object.keys(patch.toolbars ?? {})])];
  return ids
    .map((id) => ({ id, spec: { ...TOOLBARS.builtIn[id], ...patch.toolbars?.[id] } }))
    .filter(({ spec }) => spec.enabled !== false)
    .map(({ id, spec }) => ({
      id,
      title: spec.title ?? id,
      place: spec.place ?? "editor-top",
      behaviour: spec.behaviour ?? "always",
      contexts: spec.contexts ?? [],
      style: spec.style ?? "icons",
      density: spec.density ?? "compact",
      surface: spec.surface ?? "strip",
      items: spec.items ?? [],
    }))
    .filter((toolbar) => toolbar.items.length > 0 && TOOLBARS.desktopPlaces.includes(toolbar.place));
}

/** Whether a bar shows while the cursor sits in text with a selection, as
    the window draws it; on-hover bars show while the pointer is on the note. */
export function showsInText(toolbar: ResolvedToolbar): boolean {
  if (toolbar.behaviour === "in-context") return toolbar.contexts.includes("text");
  return true;
}
