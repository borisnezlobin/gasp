"use client";

import { useEffect, useId, useRef } from "react";
import { subscribeGlyphs } from "./glyphEngine";
import { createBody, stepBody, transformOf, type GlyphBody, type Pointer } from "./glyphPhysics";
import { RippleFilter, filterIdOf } from "./RippleFilter";

/** Markdown's own marks, rising through the page like the whale's breath.
    Each one sways as it climbs, grows a little the way a bubble does, and
    shies from the pointer like the headings. */

const MARKS = ["#", "**", "*", "_", ">", "-", "[ ]", "`", "1.", "~~", "[[", "]]", "##", "a", "g", "s", "p", "=="];
const MOST = 18;
const AREA_PER_MARK = 80_000;

type Bubble = {
  span: HTMLElement;
  body: GlyphBody;
  x: number;
  y: number;
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

/** Starts `bubble` again at a new place, below the screen or, on the first
    pass, anywhere on it. */
function respawn(bubble: Bubble, random: () => number, anywhere: boolean) {
  const size = 16 + random() ** 2 * 44;
  bubble.span.textContent = MARKS[Math.floor(random() * MARKS.length)];
  bubble.span.style.fontSize = `${size}px`;
  bubble.body = createBody(size, random() * 10);
  bubble.x = random();
  bubble.y = anywhere ? random() * window.innerHeight : window.innerHeight + size;
  bubble.speed = 16 + random() * 26;
  bubble.sway = 6 + random() * 18;
  bubble.swayRate = 0.8 + random() * 1.2;
  bubble.opacity = 0.22 + random() * 0.3;
}

/** Fades a bubble in as it rises from the bottom and out near the top. */
function visibilityAt(y: number, height: number) {
  const share = y / height;
  return Math.min(1, Math.max(0, (1 - share) / 0.12), Math.max(0, (share - 0.04) / 0.22));
}

function stepBubble(bubble: Bubble, dt: number, time: number, pointer: Pointer, random: () => number) {
  const { innerWidth: width, innerHeight: height } = window;
  bubble.y -= bubble.speed * dt;
  if (bubble.y < -bubble.body.size * 2) respawn(bubble, random, false);
  const homeX = bubble.x * width + bubble.sway * Math.sin(time * bubble.swayRate + bubble.body.phase);
  stepBody(bubble.body, homeX, bubble.y, pointer, dt);
  const grow = 0.8 + 0.35 * (1 - bubble.y / height);
  bubble.span.style.transform = transformOf(bubble.body, time, homeX, bubble.y, grow);
  bubble.span.style.opacity = (bubble.opacity * visibilityAt(bubble.y, height)).toFixed(3);
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
      respawn(bubble, random, true);
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
    <div aria-hidden className="pointer-events-none fixed inset-0 -z-10 overflow-hidden motion-reduce:hidden">
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
