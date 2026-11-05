import type { Pointer } from "./glyphPhysics";

/** One clock and one pointer for every soft glyph on the page. Each part
    that draws glyphs subscribes; the loop runs only while one is subscribed
    and motion is allowed. Every frame reads the page first and writes styles
    after, so no read makes the browser redo the layout a write just dirtied. */

export type GlyphView = {
  scrollY: number;
  width: number;
  height: number;
  /** The page moved within the last moment, so nobody can see fine ripples. */
  scrolling: boolean;
  /** How long the last frame really took, which `dt` caps. */
  frameSeconds: number;
};

export type GlyphSubscriber = {
  read?: () => void;
  step: (dt: number, time: number, pointer: Pointer, view: GlyphView) => void;
};

const LONGEST_STEP = 1 / 30;
const SCROLL_SETTLE_MS = 150;
const subscribers = new Set<GlyphSubscriber>();
let pointer: Pointer = null;
let frame = 0;
let last = 0;
let lastScrollY = 0;
let lastScrollAt = 0;

const motionAllowed = () => !window.matchMedia("(prefers-reduced-motion: reduce)").matches;

const onMove = (event: PointerEvent) => {
  pointer = { x: event.clientX, y: event.clientY };
};
const onLeave = () => {
  pointer = null;
};

function tick(now: number) {
  frame = requestAnimationFrame(tick);
  const frameSeconds = (now - last) / 1000;
  const dt = Math.min(LONGEST_STEP, frameSeconds);
  last = now;
  const scrollY = window.scrollY;
  if (scrollY !== lastScrollY) lastScrollAt = now;
  lastScrollY = scrollY;
  const scrolling = now - lastScrollAt < SCROLL_SETTLE_MS;
  const view = { scrollY, width: window.innerWidth, height: window.innerHeight, scrolling, frameSeconds };
  for (const subscriber of subscribers) subscriber.read?.();
  for (const subscriber of subscribers) subscriber.step(dt, now / 1000, pointer, view);
}

function start() {
  window.addEventListener("pointermove", onMove, { passive: true });
  window.addEventListener("pointerdown", onMove, { passive: true });
  document.documentElement.addEventListener("pointerleave", onLeave);
  window.addEventListener("blur", onLeave);
  last = performance.now();
  lastScrollY = window.scrollY;
  frame = requestAnimationFrame(tick);
}

function stop() {
  cancelAnimationFrame(frame);
  window.removeEventListener("pointermove", onMove);
  window.removeEventListener("pointerdown", onMove);
  document.documentElement.removeEventListener("pointerleave", onLeave);
  window.removeEventListener("blur", onLeave);
}

/** Runs `subscriber` every frame until the returned function is called. With
    reduced motion nothing runs and the glyphs stay where the text put
    them. */
export function subscribeGlyphs(subscriber: GlyphSubscriber): () => void {
  if (!motionAllowed()) return () => {};
  subscribers.add(subscriber);
  if (subscribers.size === 1) start();
  return () => {
    subscribers.delete(subscriber);
    if (subscribers.size === 0) stop();
  };
}
