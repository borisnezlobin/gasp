"use client";

import { useEffect, useImperativeHandle, useRef, type Ref } from "react";

export type RippleLinesHandle = {
  /** Starts a ring at `xShare` of the way along the first line. */
  drop: (xShare: number, strength?: number) => void;
};

type Ripple = { x: number; y: number; born: number; strength: number };

type RippleLinesProps = {
  /** Each line's length, as a share of the full width. */
  lengths: number[];
  /** Distance between one line's centre and the next, in px. */
  pitch?: number;
  thickness?: number;
  /** Rings follow the pointer and spread from clicks. */
  playful?: boolean;
  /** Where a ring starts once the lines first scroll into view. */
  dropWhenSeen?: number;
  className?: string;
  ref?: Ref<RippleLinesHandle>;
};

const VIEW_WIDTH = 1000;
const STEP = 5;
const MAX_LIFT = 9;
const SPEED = 360;
const WAVELENGTH = 64;
const PACKET = 80;
const FADE_SECONDS = 0.8;
const LIFE_SECONDS = 2.6;
const TRAIL_SPACING = 56;

const prefersReducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

function lift(ripple: Ripple, x: number, y: number, now: number): number {
  const age = (now - ripple.born) / 1000;
  const distance = Math.hypot(x - ripple.x, y - ripple.y);
  const behindFront = distance - SPEED * age;
  if (behindFront > PACKET * 2.5 || behindFront < -PACKET * 3) return 0;
  const envelope = Math.exp(-((behindFront / PACKET) ** 2)) * Math.exp(-age / FADE_SECONDS);
  return MAX_LIFT * ripple.strength * envelope * Math.sin((2 * Math.PI * behindFront) / WAVELENGTH);
}

function linePath(length: number, y: number, liftAt?: (xView: number) => number): string {
  const end = VIEW_WIDTH * length;
  const points: string[] = [];
  for (let x = 0; x < end; x += STEP) points.push(`${x},${(y + (liftAt?.(x) ?? 0)).toFixed(2)}`);
  points.push(`${end},${(y + (liftAt?.(end) ?? 0)).toFixed(2)}`);
  return `M${points.join("L")}`;
}

/** Owns the rings on the water and redraws the lines once a frame while
    any ring is still moving; with none left the lines lie still and
    nothing runs. */
function useRippleEngine(svg: React.RefObject<SVGSVGElement | null>, lengths: number[], centreOf: (line: number) => number) {
  const ripples = useRef<Ripple[]>([]);
  const frame = useRef(0);

  const draw = (now: number) => {
    const element = svg.current;
    if (!element) return;
    const scale = element.getBoundingClientRect().width / VIEW_WIDTH;
    ripples.current = ripples.current.filter((ripple) => now - ripple.born < LIFE_SECONDS * 1000);
    const paths = element.querySelectorAll("path");
    lengths.forEach((length, line) => {
      const y = centreOf(line);
      const liftAt = (xView: number) =>
        ripples.current.reduce((sum, ripple) => sum + lift(ripple, xView * scale, y, now), 0);
      paths[line]?.setAttribute("d", linePath(length, y, liftAt));
    });
    frame.current = ripples.current.length ? requestAnimationFrame(draw) : 0;
  };

  const add = (x: number, y: number, strength: number) => {
    if (prefersReducedMotion()) return;
    ripples.current.push({ x, y, born: performance.now(), strength });
    if (!frame.current) frame.current = requestAnimationFrame(draw);
  };

  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  return add;
}

/** The app icon's lines of a note, drawn as a water surface: a ring set
    off anywhere on them travels outward and lifts each line it crosses. */
export function RippleLines({
  lengths,
  pitch = 30,
  thickness = 10,
  playful = false,
  dropWhenSeen,
  className = "",
  ref,
}: RippleLinesProps) {
  const svg = useRef<SVGSVGElement>(null);
  const lastTrail = useRef<{ x: number; y: number } | null>(null);
  const centreOf = (line: number) => MAX_LIFT + thickness / 2 + line * pitch;
  const height = centreOf(lengths.length - 1) + thickness / 2 + MAX_LIFT;
  const addRipple = useRippleEngine(svg, lengths, centreOf);

  const widthOf = () => svg.current?.getBoundingClientRect().width ?? 0;

  useImperativeHandle(ref, () => ({
    drop: (xShare, strength = 1) => addRipple(xShare * widthOf(), centreOf(0), strength),
  }));

  useEffect(() => {
    const element = svg.current;
    if (dropWhenSeen === undefined || !element) return;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.disconnect();
        addRipple(dropWhenSeen * widthOf(), centreOf(0), 1);
      },
      { threshold: 1 },
    );
    observer.observe(element);
    return () => observer.disconnect();
  });

  const pointerAt = (event: React.PointerEvent) => {
    const box = event.currentTarget.getBoundingClientRect();
    return { x: event.clientX - box.left, y: event.clientY - box.top };
  };

  const onPointerMove = (event: React.PointerEvent) => {
    const at = pointerAt(event);
    const last = lastTrail.current;
    if (last && Math.hypot(at.x - last.x, at.y - last.y) < TRAIL_SPACING) return;
    lastTrail.current = at;
    addRipple(at.x, at.y, 0.35);
  };

  return (
    <svg
      ref={svg}
      aria-hidden
      viewBox={`0 0 ${VIEW_WIDTH} ${height}`}
      preserveAspectRatio="none"
      className={`block w-full overflow-visible ${className}`}
      style={{ height, marginBlock: -MAX_LIFT }}
      onPointerMove={playful ? onPointerMove : undefined}
      onPointerDown={playful ? (event) => addRipple(pointerAt(event).x, pointerAt(event).y, 1) : undefined}
      onPointerLeave={() => (lastTrail.current = null)}
    >
      {lengths.map((length, line) => (
        <path
          key={line}
          d={linePath(length, centreOf(line))}
          fill="none"
          stroke="var(--sea)"
          strokeWidth={thickness}
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />
      ))}
    </svg>
  );
}
