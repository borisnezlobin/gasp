"use client";

import { useEffect, useRef, useState } from "react";

const LINES = 48_000;
const FILL_MS = 700;
/** A drawn line and the gap under it, in device pixels. */
const ROW_PITCH = 2;

/** The same pseudo-random line lengths on every draw, with a short last
    line and a blank one closing each paragraph. */
function lineLength(index: number): number {
  const x = Math.sin(index * 12.9898) * 43758.5453;
  const fraction = x - Math.floor(x);
  const place = index % 9;
  if (place === 8) return 0;
  if (place === 7) return 0.2 + fraction * 0.4;
  return 0.75 + fraction * 0.25;
}

/** Lays out exactly `LINES` marks, one device pixel tall each, in as many
    columns as the canvas needs, and draws the first `count` of them. */
function drawLines(canvas: HTMLCanvasElement, count: number) {
  const context = canvas.getContext("2d");
  if (!context) return;
  const scale = window.devicePixelRatio || 1;
  const { width, height } = canvas.getBoundingClientRect();
  canvas.width = Math.round(width * scale);
  canvas.height = Math.round(height * scale);
  context.fillStyle = getComputedStyle(canvas).color;
  const rows = Math.floor(canvas.height / ROW_PITCH);
  const columns = Math.ceil(LINES / rows);
  const columnWidth = canvas.width / columns;
  const markWidth = Math.max(1, columnWidth - Math.max(1, Math.round(scale * 2)));
  for (let line = 0; line < count; line++) {
    const column = Math.floor(line / rows);
    const row = line % rows;
    context.fillRect(column * columnWidth, row * ROW_PITCH, markWidth * lineLength(line), 1);
  }
}

function useCountWhenSeen(target: React.RefObject<HTMLElement | null>) {
  const [count, setCount] = useState(0);
  useEffect(() => {
    const element = target.current;
    if (!element) return;
    let frame = 0;
    const run = () => {
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return setCount(LINES);
      const began = performance.now();
      const step = (now: number) => {
        const share = Math.min(1, (now - began) / FILL_MS);
        setCount(Math.round(LINES * (1 - (1 - share) ** 3)));
        if (share < 1) frame = requestAnimationFrame(step);
      };
      frame = requestAnimationFrame(step);
    };
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.disconnect();
        run();
      },
      { threshold: 0.35 },
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      cancelAnimationFrame(frame);
    };
  }, [target]);
  return count;
}

function useRedrawOnResize() {
  const [, redraw] = useState(0);
  useEffect(() => {
    const onChange = () => redraw((n) => n + 1);
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    window.addEventListener("resize", onChange);
    scheme.addEventListener("change", onChange);
    return () => {
      window.removeEventListener("resize", onChange);
      scheme.removeEventListener("change", onChange);
    };
  }, []);
}

/** A 48,000-line note drawn to scale, one mark per line. */
export function LineField() {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const count = useCountWhenSeen(box);
  useRedrawOnResize();

  useEffect(() => {
    if (canvas.current) drawLines(canvas.current, count);
  });

  return (
    <div ref={box}>
      <p className="figure text-2xl font-bold sm:text-3xl">{count.toLocaleString("en")} lines</p>
      <canvas
        ref={canvas}
        role="img"
        aria-label="48,000 short marks, one for each line of a very long note."
        className="mt-4 h-56 w-full text-ink-muted/70 sm:h-72"
      />
    </div>
  );
}
