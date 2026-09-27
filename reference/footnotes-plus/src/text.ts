// Convert an absolute offset into a {line, ch} position within a given string.
// Used when we need coordinates in the POST-edit document (the editor's own
// offsetToPos maps against the current doc, which is wrong for a text we're
// about to swap in).

export interface Pos {
  line: number;
  ch: number;
}

export function posInText(text: string, offset: number): Pos {
  let line = 0;
  let lineStart = 0;
  const limit = Math.min(offset, text.length);
  for (let i = 0; i < limit; i++) {
    if (text[i] === "\n") {
      line++;
      lineStart = i + 1;
    }
  }
  return { line, ch: limit - lineStart };
}
