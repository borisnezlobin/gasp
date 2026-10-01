/** Where in a line's Markdown source a click landed. The line is drawn
    formatted, with its marks hidden, so each run of text carries
    `data-source`: the source offset its first character comes from. The
    nearest gap between characters to the click wins, weighing a row's
    distance more than a column's so wrapped lines pick the right row. */
export function sourceOffsetAt(line: HTMLElement, x: number, y: number): number | null {
  let nearest: { offset: number; distance: number } | null = null;
  const range = document.createRange();
  for (const run of line.querySelectorAll<HTMLElement>("[data-source]")) {
    const text = run.firstChild;
    if (!text || text.nodeType !== Node.TEXT_NODE) continue;
    const start = Number(run.dataset.source);
    for (let index = 0; index <= (text.textContent ?? "").length; index++) {
      range.setStart(text, index);
      range.setEnd(text, index);
      const box = range.getBoundingClientRect();
      const distance = Math.abs(box.top + box.height / 2 - y) * 4 + Math.abs(box.left - x);
      if (!nearest || distance < nearest.distance) nearest = { offset: start + index, distance };
    }
  }
  return nearest?.offset ?? null;
}
