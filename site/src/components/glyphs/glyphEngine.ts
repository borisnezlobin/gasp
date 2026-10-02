import type { Pointer } from "./glyphPhysics";

/** One clock and one pointer for every soft glyph on the page. Each part
    that draws glyphs subscribes a `step`; the loop runs only while one is
    subscribed and motion is allowed. */

export type GlyphStep = (dt: number, time: number, pointer: Pointer) => void;

const LONGEST_STEP = 1 / 30;
const steps = new Set<GlyphStep>();
let pointer: Pointer = null;
let frame = 0;
let last = 0;

const motionAllowed = () => !window.matchMedia("(prefers-reduced-motion: reduce)").matches;

const onMove = (event: PointerEvent) => {
  pointer = { x: event.clientX, y: event.clientY };
};
const onLeave = () => {
  pointer = null;
};

function tick(now: number) {
  frame = requestAnimationFrame(tick);
  const dt = Math.min(LONGEST_STEP, (now - last) / 1000);
  last = now;
  for (const step of steps) step(dt, now / 1000, pointer);
}

function start() {
  window.addEventListener("pointermove", onMove, { passive: true });
  window.addEventListener("pointerdown", onMove, { passive: true });
  document.documentElement.addEventListener("pointerleave", onLeave);
  window.addEventListener("blur", onLeave);
  last = performance.now();
  frame = requestAnimationFrame(tick);
}

function stop() {
  cancelAnimationFrame(frame);
  window.removeEventListener("pointermove", onMove);
  window.removeEventListener("pointerdown", onMove);
  document.documentElement.removeEventListener("pointerleave", onLeave);
  window.removeEventListener("blur", onLeave);
}

/** Runs `step` every frame until the returned function is called. With
    reduced motion nothing runs and the glyphs stay where the text put
    them. */
export function subscribeGlyphs(step: GlyphStep): () => void {
  if (!motionAllowed()) return () => {};
  steps.add(step);
  if (steps.size === 1) start();
  return () => {
    steps.delete(step);
    if (steps.size === 0) stop();
  };
}
