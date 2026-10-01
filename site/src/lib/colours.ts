/** Colour values the demo accepts for Gasp's colour tokens: hex, rgb() and
    rgba(), or a `{color.name}` reference to another token. */

export type Rgba = { r: number; g: number; b: number; a: number };

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/;
const RGB = /^rgba?\(\s*(\d{1,3})\s*,\s*(\d{1,3})\s*,\s*(\d{1,3})\s*(?:,\s*(0|1|0?\.\d+|1\.0+)\s*)?\)$/;
export const REFERENCE = /^\{([a-z0-9.-]+)\}$/;

const expandShortHex = (digits: string) => (digits.length === 3 ? [...digits].map((digit) => digit + digit).join("") : digits);

function hexToRgba(hex: string): Rgba {
  const digits = expandShortHex(hex.slice(1));
  const channel = (index: number) => parseInt(digits.slice(index * 2, index * 2 + 2), 16);
  return { r: channel(0), g: channel(1), b: channel(2), a: digits.length === 8 ? channel(3) / 255 : 1 };
}

export function parseColour(value: string): Rgba | null {
  const text = value.trim().toLowerCase();
  if (HEX.test(text)) return hexToRgba(text);
  const match = text.match(RGB);
  if (!match) return null;
  const [r, g, b] = [match[1], match[2], match[3]].map(Number);
  if ([r, g, b].some((channel) => channel > 255)) return null;
  return { r, g, b, a: match[4] === undefined ? 1 : Number(match[4]) };
}

const hexPair = (channel: number) => Math.round(Math.min(255, Math.max(0, channel))).toString(16).padStart(2, "0");

export function toHex({ r, g, b }: Rgba): string {
  return `#${hexPair(r)}${hexPair(g)}${hexPair(b)}`;
}

/** A colour as the theme file writes it: lowercase hex, or rgba() for a translucent one. */
export function formatColour(colour: Rgba): string {
  if (colour.a >= 1) return toHex(colour);
  return `rgba(${Math.round(colour.r)}, ${Math.round(colour.g)}, ${Math.round(colour.b)}, ${Number(colour.a.toFixed(3))})`;
}

function linear(channel: number): number {
  const value = channel / 255;
  return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
}

export function luminance({ r, g, b }: Rgba): number {
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

export function contrast(first: Rgba, second: Rgba): number {
  const [lighter, darker] = [luminance(first), luminance(second)].sort((a, b) => b - a);
  return (lighter + 0.05) / (darker + 0.05);
}

export function mix(from: Rgba, to: Rgba, amount: number): Rgba {
  const blend = (a: number, b: number) => a + (b - a) * amount;
  return { r: blend(from.r, to.r), g: blend(from.g, to.g), b: blend(from.b, to.b), a: from.a };
}

const WHITE: Rgba = { r: 255, g: 255, b: 255, a: 1 };

/** Gasp's dark note surface, `dark.color.background`. */
export const DARK_PAGE: Rgba = { r: 0x1c, g: 0x1b, b: 0x19, a: 1 };

/** Lightens a colour toward white until it reads on the dark page. */
function liftForDark(colour: Rgba): Rgba {
  for (let step = 0; step <= 10; step += 1) {
    const lifted = mix(colour, WHITE, step / 10);
    if (contrast(lifted, DARK_PAGE) >= 4.5) return lifted;
  }
  return WHITE;
}

type Hsl = { h: number; s: number; l: number };

function hueOf({ r, g, b }: Rgba, max: number, delta: number): number {
  if (delta === 0) return 0;
  if (max === r) return ((g - b) / delta + 6) % 6;
  if (max === g) return (b - r) / delta + 2;
  return (r - g) / delta + 4;
}

function toHsl(colour: Rgba): Hsl {
  const [r, g, b] = [colour.r / 255, colour.g / 255, colour.b / 255];
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const delta = max - min;
  const s = delta === 0 ? 0 : delta / (1 - Math.abs(2 * l - 1));
  return { h: hueOf({ r, g, b, a: 1 }, max, delta) * 60, s, l };
}

function fromHsl({ h, s, l }: Hsl, a: number): Rgba {
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const x = chroma * (1 - Math.abs(((h / 60) % 2) - 1));
  const sector = Math.floor(h / 60) % 6;
  const [r, g, b] = [
    [chroma, x, 0],
    [x, chroma, 0],
    [0, chroma, x],
    [0, x, chroma],
    [x, 0, chroma],
    [chroma, 0, x],
  ][sector];
  const lift = l - chroma / 2;
  return { r: (r + lift) * 255, g: (g + lift) * 255, b: (b + lift) * 255, a };
}

/** A pale surface for the dark page: the same hue, as far below the dark
    page's lightness as the colour sat below white, as Gasp pairs its
    highlight #fff59d with #4f4418. */
function sinkForDark(colour: Rgba): Rgba {
  const { h, s, l } = toHsl(colour);
  return fromHsl({ h, s: s * 0.6, l: toHsl(DARK_PAGE).l + (1 - l) * 0.55 }, colour.a);
}

export type ColourRole = "surface" | "ink";

/** The dark-mode partner of a colour chosen for light mode, the way Gasp's
    own dark palette pairs them: ink is lifted until it reads on the dark
    page, and a pale surface sinks into the page as a tint of itself. */
export function darkTwin(colour: Rgba, role: ColourRole): Rgba {
  if (role === "ink") return { ...liftForDark(colour), a: colour.a };
  if (luminance(colour) > 0.6) return sinkForDark(colour);
  return colour;
}
