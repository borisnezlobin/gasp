/** The name written in note lines: the letters of "Gasp" filled with rows
    of short horizontal strokes, the way a page of text fills its column.
    Each stroke becomes a target a word can fly to. */

export type LetterStroke = { x: number; y: number; length: number };

const FONT = "700 400px Charter, 'Charis SIL', Georgia, serif";
const ALPHA_INSIDE = 128;
const LONGEST = 6;
const GAP = 0.32;

/** Where each row of the word is ink, as runs of [start, end] in pixels. */
function inkRuns(pixels: Uint8ClampedArray, width: number, y: number): [number, number][] {
  const runs: [number, number][] = [];
  let start = -1;
  for (let x = 0; x < width; x++) {
    const inside = pixels[(y * width + x) * 4 + 3] > ALPHA_INSIDE;
    if (inside && start < 0) start = x;
    if (!inside && start >= 0) {
      runs.push([start, x]);
      start = -1;
    }
  }
  if (start >= 0) runs.push([start, width]);
  return runs;
}

/** A long run split into strokes no longer than the longest word. */
function strokesOf(from: number, to: number, y: number): LetterStroke[] {
  const length = to - from;
  const pieces = Math.max(1, Math.ceil(length / LONGEST));
  const each = (length - GAP * (pieces - 1)) / pieces;
  return Array.from({ length: pieces }, (_, index) => {
    const start = from + index * (each + GAP);
    return { x: start + each / 2, y, length: each };
  });
}

/** The strokes of `text` `width` metres wide, centred on the origin, with
    rows `pitch` metres apart. Runs shorter than a word's caps are dropped. */
export function letterStrokes(text: string, width: number, pitch: number): LetterStroke[] {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return [];
  context.font = FONT;
  const metrics = context.measureText(text);
  const ascent = Math.ceil(metrics.actualBoundingBoxAscent);
  const descent = Math.ceil(metrics.actualBoundingBoxDescent);
  canvas.width = Math.ceil(metrics.width);
  canvas.height = ascent + descent;
  context.font = FONT;
  context.fillText(text, 0, ascent);
  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  const scale = width / canvas.width;
  const height = canvas.height * scale;
  const rows = Math.max(4, Math.round(height / pitch));
  const strokes: LetterStroke[] = [];
  for (let row = 0; row < rows; row++) {
    const y = Math.round(((row + 0.5) / rows) * canvas.height);
    const worldY = height / 2 - (row + 0.5) * (height / rows);
    for (const [from, to] of inkRuns(pixels, canvas.width, y)) {
      const start = from * scale - width / 2;
      const end = to * scale - width / 2;
      if (end - start < 0.6) continue;
      strokes.push(...strokesOf(start, end, worldY));
    }
  }
  return strokes;
}
