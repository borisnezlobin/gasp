"use client";

import { useEffect, useId, useRef } from "react";
import { subscribeGlyphs } from "./glyphEngine";
import { createBody, stepBody, transformOf, type GlyphBody, type Pointer } from "./glyphPhysics";
import { RippleFilter, filterIdOf } from "./RippleFilter";

/** Markdown's own marks, rising through the page like the whale's breath.
    They belong to the page, so they scroll with it. Each one is born
    somewhere on screen, fades in, sways as it climbs a short way, grows a
    little the way a bubble does and fades out again, shying from the
    pointer like the headings. */

const MARKS = ["#", "**", "*", "_", ">", "-", "[ ]", "`", "1.", "~~", "[[", "]]", "##", "a", "g", "s", "p", "=="];
const MOST = 18;
const AREA_PER_MARK = 80_000;

type Bubble = {
  span: HTMLElement;
  body: GlyphBody;
  x: number;
  startY: number;
  risen: number;
  climb: number;
  speed: number;
  sway: number;
  swayRate: number;
  opacity: number;
};

const FADE_SHARE = 0.25;

function seeded(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1664525 + 1013904223) % 4294967296;
    return state / 4294967296;
  };
}

/** Starts `bubble` again somewhere in the part of the page on screen, in
    page coordinates, leaning towards the lower half so it rises into view. */
function respawn(bubble: Bubble, random: () => number) {
  const size = 16 + random() ** 2 * 44;
  bubble.span.textContent = MARKS[Math.floor(random() * MARKS.length)];
  bubble.span.style.fontSize = `${size}px`;
  bubble.body = createBody(size, random() * 10);
  bubble.x = random();
  bubble.startY = window.scrollY + window.innerHeight * (0.15 + Math.sqrt(random()) * 0.95);
  bubble.risen = 0;
  bubble.climb = 260 + random() * 520;
  bubble.speed = 16 + random() * 26;
  bubble.sway = 6 + random() * 18;
  bubble.swayRate = 0.8 + random() * 1.2;
  bubble.opacity = 0.22 + random() * 0.3;
}

/** How far through its climb the bubble is, 0 to 1. */
const progressOf = (bubble: Bubble) => bubble.risen / bubble.climb;

/** Fades a bubble in as its climb starts and out as it ends. */
function visibilityOf(progress: number) {
  return Math.min(1, progress / FADE_SHARE, (1 - progress) / FADE_SHARE);
}

/** Whether the page has scrolled the bubble well out of sight. */
function scrolledAway(y: number) {
  const margin = window.innerHeight * 0.5;
  return y < window.scrollY - margin || y > window.scrollY + window.innerHeight + margin;
}

function stepBubble(bubble: Bubble, dt: number, time: number, pointer: Pointer, random: () => number) {
  bubble.risen += bubble.speed * dt;
  const y = bubble.startY - bubble.risen;
  if (progressOf(bubble) >= 1 || scrolledAway(y)) {
    respawn(bubble, random);
    return;
  }
  const homeX = bubble.x * window.innerWidth + bubble.sway * Math.sin(time * bubble.swayRate + bubble.body.phase);
  stepBody(bubble.body, homeX, y - window.scrollY, pointer, dt);
  const progress = progressOf(bubble);
  bubble.span.style.transform = transformOf(bubble.body, time, homeX, y, 0.85 + 0.3 * progress);
  bubble.span.style.opacity = (bubble.opacity * visibilityOf(progress)).toFixed(3);
}

const markCount = () => Math.min(MOST, Math.max(6, Math.round((window.innerWidth * window.innerHeight) / AREA_PER_MARK)));

export function GlyphDrift() {
  const layer = useRef<HTMLDivElement>(null);
  const filterId = filterIdOf(useId());
  useEffect(() => {
    const element = layer.current;
    if (!element) return;
    const random = seeded(Date.now() % 100_000);
    const spans = Array.from(element.querySelectorAll<HTMLElement>("[data-mark]"));
    const bubbles: Bubble[] = spans.map((span) => {
      const bubble = { span } as Bubble;
      respawn(bubble, random);
      bubble.risen = random() * bubble.climb;
      return bubble;
    });
    return subscribeGlyphs((dt, time, pointer) => {
      const count = markCount();
      bubbles.forEach((bubble, index) => {
        bubble.span.hidden = index >= count;
        if (index < count) stepBubble(bubble, dt, time, pointer, random);
      });
    });
  }, []);
  return (
    <div aria-hidden className="pointer-events-none absolute inset-0 -z-10 overflow-hidden motion-reduce:hidden">
      <RippleFilter id={filterId} scale={4} softness={0.5} />
      <div ref={layer} className="absolute inset-0 text-ink-muted">
        {Array.from({ length: MOST }, (_, index) => (
          <span
            key={index}
            data-mark
            className="absolute top-0 left-0 font-bold whitespace-nowrap opacity-0"
            style={{ filter: `url(#${filterId})` }}
          />
        ))}
      </div>
    </div>
  );
}
