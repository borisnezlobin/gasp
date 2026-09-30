"use client";

import { useEffect, useRef, useState } from "react";

const LINES = 48_000;
const MEMORY_MB = 300;
const MAC_MEMORY_MB = 8 * 1024;
const FILL_MS = 1600;
const LINE_HEIGHT = 3;
const COLUMN_WIDTH = 16;

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

function drawLines(canvas: HTMLCanvasElement, count: number) {
  const context = canvas.getContext("2d");
  if (!context) return;
  const scale = window.devicePixelRatio || 1;
  const { width, height } = canvas.getBoundingClientRect();
  canvas.width = Math.round(width * scale);
  canvas.height = Math.round(height * scale);
  context.scale(scale, scale);
  context.fillStyle = getComputedStyle(canvas).color;
  const rows = Math.floor(height / LINE_HEIGHT);
  const columns = Math.floor(width / COLUMN_WIDTH);
  const perLine = LINES / (rows * columns);
  const shown = Math.floor(count / perLine);
  for (let cell = 0; cell < shown; cell++) {
    const column = Math.floor(cell / rows);
    const row = cell % rows;
    context.fillRect(column * COLUMN_WIDTH, row * LINE_HEIGHT, (COLUMN_WIDTH - 4) * lineLength(cell), 1.5);
  }
}

function useFillWhenSeen(target: React.RefObject<HTMLElement | null>) {
  const [count, setCount] = useState(0);
  useEffect(() => {
    const element = target.current;
    if (!element) return;
    let frame = 0;
    const run = () => {
      if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
        setCount(LINES);
        return;
      }
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

/** A 48,000-line note drawn to scale as a field of lines, and under it a
    Mac's 8 GB of memory with the part Gasp uses for that note marked. */
export function MemoryField() {
  const box = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const count = useFillWhenSeen(box);
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

  useEffect(() => {
    if (canvas.current) drawLines(canvas.current, count);
  });

  const filled = count / LINES;
  return (
    <div ref={box}>
      <p className="figure text-2xl font-bold sm:text-3xl">{count.toLocaleString("en")} lines in one note</p>
      <canvas
        ref={canvas}
        aria-label="A field of 48,000 short lines standing for one very long note."
        role="img"
        className="mt-4 h-56 w-full text-ink-muted/60 sm:h-72"
      />
      <div className="mt-10">
        <div className="relative h-4 rounded-full bg-fill" role="img" aria-label="Gasp uses under 300 MB of a Mac's 8 GB of memory for it.">
          <div
            className="absolute inset-y-0 left-0 rounded-full bg-caret transition-[width] duration-700 ease-out-soft"
            style={{ width: `${(MEMORY_MB / MAC_MEMORY_MB) * 100 * filled}%`, minWidth: filled > 0 ? "0.5rem" : 0 }}
          />
        </div>
        <div className="figure mt-3 flex justify-between gap-4">
          <p>
            <span className="text-2xl font-bold sm:text-3xl">Under 300 MB</span>
          </p>
          <p className="small self-end text-ink-muted">of a Mac&apos;s 8 GB</p>
        </div>
      </div>
    </div>
  );
}
