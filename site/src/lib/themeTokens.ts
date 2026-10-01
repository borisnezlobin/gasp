import { REFERENCE, luminance, parseColour, type ColourRole } from "./colours";
import { DARK_PREFIX, SCHEMA, THEME_TOKENS } from "./gaspSchema";

export type TokenValue = string | number;
export type TokenMap = Record<string, TokenValue>;
export type Mode = "light" | "dark";

const EMBEDDED_REFERENCE = /\{([a-z0-9.-]+)\}/g;
const MOST_HOPS = 12;

const LIGHT_DEFAULTS: TokenMap = Object.fromEntries(SCHEMA.theme.map((token) => [token.key, token.light]));
const DARK_DEFAULTS: TokenMap = Object.fromEntries(
  SCHEMA.theme.filter((token) => token.dark !== undefined).map((token) => [token.key, token.dark as TokenValue]),
);

const isDark = (key: string) => key.startsWith(DARK_PREFIX);

function splitByMode(theme: TokenMap): Record<Mode, TokenMap> {
  const light: TokenMap = {};
  const dark: TokenMap = {};
  for (const [key, value] of Object.entries(theme)) {
    if (isDark(key)) dark[key.slice(DARK_PREFIX.length)] = value;
    else light[key] = value;
  }
  return { light, dark };
}

/** The raw tokens of one mode, as Gasp layers them: a vault's tokens replace
    the built-in ones by name, then every `dark.<name>` replaces `<name>`
    while the app is dark. */
export function layeredTokens(theme: TokenMap, mode: Mode): TokenMap {
  const user = splitByMode(theme);
  const light = { ...LIGHT_DEFAULTS, ...user.light };
  return mode === "light" ? light : { ...light, ...DARK_DEFAULTS, ...user.dark };
}

function resolveOne(tokens: TokenMap, value: TokenValue, hops: number): TokenValue {
  if (typeof value !== "string" || hops > MOST_HOPS) return value;
  const whole = value.match(REFERENCE);
  if (whole) return resolveOne(tokens, tokens[whole[1]] ?? value, hops + 1);
  return value.replace(EMBEDDED_REFERENCE, (match, name: string) => String(resolveOne(tokens, tokens[name] ?? match, hops + 1)));
}

/** Every `{name}` reference replaced by the value it names. */
export function resolveTokens(tokens: TokenMap): TokenMap {
  return Object.fromEntries(Object.entries(tokens).map(([key, value]) => [key, resolveOne(tokens, value, 0)]));
}

const RESOLVED_LIGHT_DEFAULTS = resolveTokens(LIGHT_DEFAULTS);

/** Whether a colour token is drawn behind things or as ink on them, judged
    from its built-in light value; translucent fills have no role. */
export function colourRole(key: string): ColourRole | null {
  const colour = parseColour(String(RESOLVED_LIGHT_DEFAULTS[key] ?? ""));
  if (!colour || colour.a < 1) return null;
  return luminance(colour) > 0.6 ? "surface" : "ink";
}

/** Whether Gasp's own dark palette gives this token a value of its own,
    directly or through the token it refers to. */
export function changesInDark(key: string): boolean {
  if (key in DARK_DEFAULTS) return true;
  const reference = String(LIGHT_DEFAULTS[key] ?? "").match(REFERENCE)?.[1];
  return reference !== undefined && THEME_TOKENS.has(reference) && changesInDark(reference);
}
