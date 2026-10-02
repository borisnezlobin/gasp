"use client";

import { useEffect, useId, useRef } from "react";
import { subscribeGlyphs } from "./glyphEngine";
import { createBody, stepBody, transformOf, type GlyphBody, type Pointer } from "./glyphPhysics";
import { RippleFilter, filterIdOf } from "./RippleFilter";

/** Markdown's own marks, rising through the whole page like the whale's
    breath. Each mark has its own spot somewhere down the page and keeps
    rising from near it: it fades in, sways as it climbs a short way, grows
    a little the way a bubble does, fades out and starts again. They scroll
    with the page, only the ones near the screen move, and they shy from
    the pointer like the headings. */

const MARKS = ["#", "**", "*", "_", ">", "-", "[ ]", "`", "1.", "~~", "[[", "]]", "##", "a", "g", "s", "p", "=="];
const AREA_PER_MARK = 90_000;
const FEWEST = 8;
const MOST = 180;
const FADE_SHARE = 0.25;
const MARK_CLASS = "absolute top-0 left-0 font-bold whitespace-nowrap opacity-0";

type Bubble = {
  span: HTMLElement;
  body: GlyphBody;
  spotX: number;
  spotY: number;
  x: number;
  startY: number;
  risen: number;
  climb: number;
  speed: number;
  sway: number;
  swayRate: number;
  opacity: number;
};

function seeded(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1664525 + 1013904223) % 4294967296;
    return state / 4294967296;
  };
}

/** Starts `bubble` on a new climb from near its own spot on the page. */
function restart(bubble: Bubble, random: () => number, pageHeight: number) {
  const size = 16 + random() ** 2 * 44;
  bubble.span.textContent = MARKS[Math.floor(random() * MARKS.length)];
  bubble.span.style.fontSize = `${size}px`;
  bubble.body = createBody(size, random() * 10);
  bubble.x = Math.min(0.98, Math.max(0.02, bubble.spotX + (random() - 0.5) * 0.12));
  bubble.climb = 260 + random() * 520;
  bubble.startY = bubble.spotY * pageHeight + bubble.climb / 2 + (random() - 0.5) * 120;
  bubble.risen = 0;
  bubble.speed = 16 + random() * 26;
  bubble.sway = 6 + random() * 18;
  bubble.swayRate = 0.8 + random() * 1.2;
  bubble.opacity = 0.22 + random() * 0.3;
}

/** A mark for every so much of the page, spread evenly down it so every
    part of the page has a few. */
function fillPage(layer: HTMLElement, bubbles: Bubble[], random: () => number, filter: string) {
  const want = Math.min(MOST, Math.max(FEWEST, Math.round((layer.offsetWidth * layer.offsetHeight) / AREA_PER_MARK)));
  while (bubbles.length > want) bubbles.pop()?.span.remove();
  while (bubbles.length < want) {
    const span = document.createElement("span");
    span.className = MARK_CLASS;
    span.style.setProperty("filter", filter);
    layer.append(span);
    bubbles.push({ span, spotX: random() } as Bubble);
  }
  bubbles.forEach((bubble, index) => {
    bubble.spotY = (index + random()) / bubbles.length;
    if (bubble.body) return;
    restart(bubble, random, layer.offsetHeight);
    bubble.risen = random() * bubble.climb;
  });
}

/** Fades a bubble in as its climb starts and out as it ends. */
const visibilityOf = (progress: number) => Math.min(1, progress / FADE_SHARE, (1 - progress) / FADE_SHARE);

/** Whether `y` on the page is on screen or close to it. */
function nearScreen(y: number) {
  const margin = window.innerHeight * 0.3;
  return y > window.scrollY - margin && y < window.scrollY + window.innerHeight + margin;
}

function stepBubble(bubble: Bubble, dt: number, time: number, pointer: Pointer, random: () => number, pageHeight: number) {
  if (!nearScreen(bubble.startY - bubble.risen)) return;
  bubble.risen += bubble.speed * dt;
  if (bubble.risen >= bubble.climb) restart(bubble, random, pageHeight);
  const y = bubble.startY - bubble.risen;
  const progress = bubble.risen / bubble.climb;
  const homeX = bubble.x * window.innerWidth + bubble.sway * Math.sin(time * bubble.swayRate + bubble.body.phase);
  stepBody(bubble.body, homeX, y - window.scrollY, pointer, dt);
  bubble.span.style.transform = transformOf(bubble.body, time, homeX, y, 0.85 + 0.3 * progress);
  bubble.span.style.opacity = (bubble.opacity * visibilityOf(progress)).toFixed(3);
}

export function GlyphDrift() {
  const layer = useRef<HTMLDivElement>(null);
  const filterId = filterIdOf(useId());
  useEffect(() => {
    const element = layer.current;
    if (!element) return;
    const random = seeded(Date.now() % 100_000);
    const bubbles: Bubble[] = [];
    let pageHeight = element.offsetHeight;
    const refill = () => {
      pageHeight = element.offsetHeight;
      fillPage(element, bubbles, random, `url(#${filterId})`);
    };
    const resizing = new ResizeObserver(refill);
    resizing.observe(element);
    const unsubscribe = subscribeGlyphs((dt, time, pointer) => {
      for (const bubble of bubbles) stepBubble(bubble, dt, time, pointer, random, pageHeight);
    });
    return () => {
      unsubscribe();
      resizing.disconnect();
      for (const bubble of bubbles) bubble.span.remove();
    };
  }, [filterId]);
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0 -z-10 overflow-hidden motion-reduce:hidden">
      <RippleFilter id={filterId} scale={4} softness={0.5} />
      <div ref={layer} className="absolute inset-0 text-ink-muted" />
    </div>
  );
}
