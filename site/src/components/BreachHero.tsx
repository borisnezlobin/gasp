"use client";

import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import breachDark from "@/assets/breach-dark.png";
import breachLight from "@/assets/breach-light.png";
import { Caret } from "./Caret";
import { InkImage } from "./InkImage";
import { RippleLines, type RippleLinesHandle } from "./RippleLines";

/** How far the whale leans towards the pointer, at most. */
const LEAN_DEGREES = 5;
/** The note's lines under the first one, as the app icon draws them. */
const SEA_LENGTHS = [1, 0.96, 0.88, 0.64, 0.92, 0.4];

/** The leap (`public/art/breach-leap-*.webp`): 22 frames at 20 fps from
    the same camera as the resting image, rising, rolling onto its back and
    falling back in. */
const LEAP_FRAMES = 22;
const LEAP_MS = 1100;
/** When the head meets the water (frame 16), and where along the drawing. */
const SPLASH_MS = 800;
const SPLASH_AT = 0.69;
/** How long the water stays empty before the whale breaches again. */
const RETURN_DELAY_MS = 700;

/** From `geometry.json` of the render: in the 900 by 1104 drawing the sea is
    at 54.4% of the height, the body crosses it 27.9% of the way in, and
    the top of the whale is at 10.6%. */
const stageGeometry = {
  "--crossing": 0.279,
  "--under": 0.456,
  "--headroom": 0.106,
  "--whale-h": "calc(var(--whale-w) * 1104 / 900)",
  "--whale-left": "calc(var(--cross) - var(--whale-w) * var(--crossing))",
  "--whale-top": "calc(var(--whale-h) * (var(--under) - 1))",
} as CSSProperties;

type Phase = "resting" | "leaping" | "gone";

const prefersReducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** Writes the pointer's pull (`--lean`) and how far the hero has scrolled
    away (`--breach`, 0 to 1) onto the stage, once a frame at most. */
function useStageMotion() {
  const stage = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = stage.current;
    if (!element || prefersReducedMotion()) return;
    let frame = 0;
    let pointerX: number | null = null;
    const update = () => {
      frame = 0;
      const box = element.getBoundingClientRect();
      const breach = Math.min(1, Math.max(0, -box.top / Math.max(box.height, 1)));
      element.style.setProperty("--breach", breach.toFixed(4));
      if (pointerX === null) return;
      const whaleX = box.left + box.width * 0.6;
      const pull = (pointerX - whaleX) / Math.max(window.innerWidth / 2, 1);
      element.style.setProperty("--lean", `${(Math.max(-1, Math.min(1, pull)) * LEAN_DEGREES).toFixed(2)}deg`);
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    const onPointer = (event: PointerEvent) => {
      pointerX = event.clientX;
      schedule();
    };
    window.addEventListener("pointermove", onPointer, { passive: true });
    window.addEventListener("scroll", schedule, { passive: true });
    update();
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("pointermove", onPointer);
      window.removeEventListener("scroll", schedule);
    };
  }, []);
  return stage;
}

/** Runs one leap: the frames play, the lines splash where the whale comes
    down, and after a moment it breaches again. */
function useLeap(splash: (xShare: number) => void) {
  const [phase, setPhase] = useState<Phase>("resting");
  const [rises, setRises] = useState(0);
  const timers = useRef<number[]>([]);

  useEffect(() => () => timers.current.forEach(window.clearTimeout), []);

  const leap = () => {
    if (phase !== "resting" || prefersReducedMotion()) return;
    setPhase("leaping");
    timers.current = [
      window.setTimeout(() => splash(SPLASH_AT), SPLASH_MS),
      window.setTimeout(() => setPhase("gone"), LEAP_MS),
      window.setTimeout(() => {
        setRises((count) => count + 1);
        setPhase("resting");
      }, LEAP_MS + RETURN_DELAY_MS),
    ];
  };

  return { phase, rises, leap };
}

/** The leap's frames side by side, stepped through by moving the strip. */
function LeapStrip({ playing }: { playing: boolean }) {
  return (
    <div aria-hidden className={`absolute inset-0 overflow-hidden ${playing ? "" : "invisible"}`}>
      <picture>
        <source srcSet="/art/breach-leap-dark.webp" media="(prefers-color-scheme: dark)" />
        <img
          src="/art/breach-leap-light.webp"
          alt=""
          decoding="async"
          fetchPriority="low"
          className={`h-full max-w-none ${playing ? "animate-leap-frames" : ""}`}
          style={{ width: `${LEAP_FRAMES * 100}%` }}
        />
      </picture>
    </div>
  );
}

/** One copy of the whale against the surface. The hero draws it twice,
    once above the surface and once below it through the water. */
function Whale({ phase, rises, priority }: { phase: Phase; rises: number; priority?: boolean }) {
  return (
    <div className="absolute top-(--whale-top) left-(--whale-left) w-(--whale-w)">
      <div
        key={rises}
        className="origin-[27.9%_54.4%] translate-y-[calc(var(--breach,0)*-10vh)] rotate-[calc(var(--breach,0)*6deg)] motion-safe:animate-rise"
      >
        <div className="relative origin-[27.9%_54.4%] rotate-(--lean,0deg) transition-[rotate] duration-700 ease-out-soft">
          <InkImage
            light={breachLight}
            dark={breachDark}
            alt=""
            sizes="(min-width: 1024px) 44vw, 80vw"
            priority={priority}
            className={`pointer-events-none h-auto w-full select-none ${phase === "resting" ? "" : "invisible"}`}
          />
          <LeapStrip playing={phase === "leaping"} />
        </div>
      </div>
    </div>
  );
}

/** The distortion under the surface: the whale's submerged half seen
    through moving water. */
function WaterFilter() {
  return (
    <svg aria-hidden className="absolute size-0">
      <filter id="water" x="-10%" y="-10%" width="120%" height="120%">
        <feTurbulence type="fractalNoise" baseFrequency="0.008 0.05" numOctaves="2" seed="4" result="noise">
          <animate
            attributeName="baseFrequency"
            dur="9s"
            values="0.008 0.05;0.011 0.07;0.008 0.05"
            repeatCount="indefinite"
          />
        </feTurbulence>
        <feDisplacementMap in="SourceGraphic" in2="noise" scale="14" xChannelSelector="R" yChannelSelector="G" />
      </filter>
    </svg>
  );
}

/** The app icon made into a page: the name set huge behind a humpback
    breaching out of the lines of a note. It rises as the page opens, leans
    towards the pointer and, when clicked, rolls onto its back and falls in,
    rippling the lines, before it breaches again. */
export function BreachHero({ children }: { children: ReactNode }) {
  const stage = useStageMotion();
  const sea = useRef<RippleLinesHandle>(null);
  const water = useRef<HTMLDivElement>(null);
  const whale = useRef<HTMLDivElement>(null);

  const splash = (drawingShare: number) => {
    const lines = water.current?.getBoundingClientRect();
    const drawing = whale.current?.getBoundingClientRect();
    if (!lines || !drawing) return;
    const x = drawing.left + drawing.width * drawingShare;
    sea.current?.drop((x - lines.left) / lines.width, 1.4);
  };
  const { phase, rises, leap } = useLeap(splash);

  return (
    <section className="relative overflow-x-clip">
      <WaterFilter />
      <div
        ref={stage}
        style={stageGeometry}
        className="relative mx-auto flex max-w-7xl flex-col px-4 pt-4 pb-20 [--cross:46%] [--whale-w:min(80vw,26rem)] sm:px-8 md:[--cross:78%] md:[--whale-w:min(46vw,28rem)] lg:[--cross:70%] lg:[--whale-w:min(40vw,40rem)]"
      >
        <h1 className="wordmark order-1 translate-y-[calc(var(--breach,0)*8vh)] select-none">Gasp</h1>

        <p className="lede relative z-10 order-4 mt-8 max-w-[26rem] text-ink sm:text-2xl lg:order-2 lg:mt-6 lg:max-w-[40%]">
          A Markdown editor optimized for speed and efficiency.
          <Caret />
        </p>

        <div
          ref={water}
          className="relative order-3 mt-[calc(var(--whale-h)*(1-var(--under)-var(--headroom))+1rem)] md:mt-10"
        >
          <div className="absolute inset-x-0 bottom-full h-(--whale-h) [clip-path:inset(-100vh_-50vw_0_-50vw)]">
            <div className="absolute inset-x-0 top-full h-0">
              <div ref={whale} className="absolute top-(--whale-top) left-(--whale-left) h-(--whale-h) w-(--whale-w)" />
              <Whale phase={phase} rises={rises} priority />
            </div>
          </div>

          <div className="relative z-10 min-h-[calc(var(--whale-h)*var(--under)*0.7)]">
            <RippleLines ref={sea} lengths={SEA_LENGTHS} />
          </div>

          <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 -bottom-8 [clip-path:inset(0_-50vw_0_-50vw)]">
            <div className="absolute inset-x-0 -top-6 bottom-0 [filter:url(#water)]">
              <div className="absolute inset-x-0 top-6 h-0">
                <Whale phase={phase} rises={rises} />
              </div>
            </div>
            <div className="absolute inset-0 bg-paper/65" />
          </div>

          <button
            type="button"
            aria-label="Make the whale leap"
            onClick={leap}
            disabled={phase !== "resting"}
            className="absolute top-[calc(var(--whale-top)+var(--whale-h)*0.14)] left-[calc(var(--whale-left)+var(--whale-w)*0.18)] z-20 h-[calc(var(--whale-h)*0.42)] w-[calc(var(--whale-w)*0.52)] cursor-pointer rounded-full disabled:cursor-default"
          />
        </div>

        <div className="relative z-10 order-5 mt-10">{children}</div>
      </div>
    </section>
  );
}
