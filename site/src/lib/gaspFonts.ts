/** Font families the demo can write to `font.text`, `font.ui` and
    `font.code`, each with the stack the page draws it with: the family a
    Mac has, then what other systems fall back to. Gasp itself falls back
    the same way when a family is missing. */
export const FONT_STACKS: Record<string, string> = {
  Charter: 'Charter, "Bitstream Charter", var(--font-charis), Georgia, serif',
  "Iowan Old Style": '"Iowan Old Style", Palatino, "Palatino Linotype", serif',
  Palatino: 'Palatino, "Palatino Linotype", "Book Antiqua", serif',
  Georgia: "Georgia, serif",
  "New York": '"New York", ui-serif, Georgia, serif',
  Baskerville: 'Baskerville, "Baskerville Old Face", Georgia, serif',
  "Times New Roman": '"Times New Roman", Times, serif',
  "Helvetica Neue": '"Helvetica Neue", Helvetica, Arial, sans-serif',
  "SF Pro": 'system-ui, -apple-system, "Segoe UI", sans-serif',
  "Avenir Next": '"Avenir Next", Avenir, "Segoe UI", sans-serif',
  Futura: 'Futura, "Century Gothic", sans-serif',
  "Gill Sans": '"Gill Sans", "Gill Sans MT", Calibri, sans-serif',
  Optima: 'Optima, Candara, "Segoe UI", sans-serif',
  Verdana: "Verdana, Geneva, sans-serif",
  Menlo: "Menlo, ui-monospace, Consolas, monospace",
  "SF Mono": '"SF Mono", ui-monospace, Menlo, monospace',
  Monaco: "Monaco, Menlo, ui-monospace, monospace",
  "Courier New": '"Courier New", Courier, monospace',
  "American Typewriter": '"American Typewriter", "Courier New", serif',
  "Comic Sans MS": '"Comic Sans MS", "Comic Sans", "Chalkboard SE", cursive',
};

export const FONT_FAMILIES = Object.keys(FONT_STACKS);

export function fontFamily(value: unknown): string | undefined {
  if (typeof value !== "string") return undefined;
  const wanted = value.trim().toLowerCase();
  return FONT_FAMILIES.find((family) => family.toLowerCase() === wanted);
}

export function fontStack(family: string): string {
  return FONT_STACKS[family] ?? `"${family.replaceAll('"', "")}", serif`;
}
