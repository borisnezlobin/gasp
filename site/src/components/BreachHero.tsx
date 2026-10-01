"use client";

import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import breachDark from "@/assets/breach-dark.png";
import breachLight from "@/assets/breach-light.png";
import { Caret } from "./Caret";
import { InkImage } from "./InkImage";
import { RippleLines, type RippleLinesHandle } from "./RippleLines";

/** The note's lines under the first one, as the app icon draws them. */
const SEA_LENGTHS = [1, 0.96, 0.88, 0.64, 0.92, 0.4];

/** The leap (`public/art/breach-leap-*.webp`): 22 frames from the resting
    image's camera, rising, rolling onto its back and falling in. Kept in
    step with `animate-leap-frames` in globals.css. */
const LEAP_FRAMES = 22;
const LEAP_MS = 1400;
const frameAt = (frame: number) => Math.round((frame / (LEAP_FRAMES - 1)) * LEAP_MS);

/** Where along the drawing the water is struck, from the render's
    `geometry.json`: the body and then the head coming down in frames 14
    and 16. */
type Splash = { at: number; x: number; strength: number };
const SPLASHES: Splash[] = [
  { at: frameAt(14), x: 0.53, strength: 1 },
  { at: frameAt(16), x: 0.8, strength: 1.5 },
];
const EXIT_SPLASH = { x: 0.28, strength: 0.8 };
/** How long the shape under the water takes to fade before the next breach. */
const SINK_MS = 350;

/** In the 900 by 1104 drawing the sea is at 54.4% of the height, the body
    crosses it 27.9% of the way in, and the top of the whale is at 10.6%. */
const stageGeometry = {
  "--crossing": 0.279,
  "--under": 0.456,
  "--headroom": 0.106,
  "--whale-h": "calc(var(--whale-w) * 1104 / 900)",
  "--whale-left": "calc(var(--cross) - var(--whale-w) * var(--crossing))",
  "--whale-top": "calc(var(--whale-h) * (var(--under) - 1))",
} as CSSProperties;

/** `emerging`: the resting drawing rises out along the body's line.
    `leaping`: the frames play. `under`: the last frame, on its back below
    the surface, sinks slowly. `sinking`: it fades before the next breach.
    With reduced motion `emerging` never animates, so the resting drawing
    simply stays. */
type Phase = "emerging" | "leaping" | "under" | "sinking";

/** Runs the breach: the whale leaves the water, leaps, falls in with two
    splashes and settles under the surface until it's asked to breach
    again. */
function useBreach(splash: (xShare: number, strength: number) => void) {
  const [phase, setPhase] = useState<Phase>("emerging");
  const [breaches, setBreaches] = useState(0);
  const timers = useRef<number[]>([]);

  useEffect(() => {
    const pending = timers.current;
    return () => pending.forEach(window.clearTimeout);
  }, []);

  const after = (ms: number, run: () => void) => timers.current.push(window.setTimeout(run, ms));

  const onEmerged = () => {
    setPhase("leaping");
    SPLASHES.forEach((each) => after(each.at, () => splash(each.x, each.strength)));
    after(LEAP_MS, () => setPhase("under"));
  };

  const onEmergeStart = () => splash(EXIT_SPLASH.x, EXIT_SPLASH.strength);

  const breachAgain = () => {
    if (phase !== "under") return;
    setPhase("sinking");
    after(SINK_MS, () => {
      setBreaches((count) => count + 1);
      setPhase("emerging");
    });
  };

  return { phase, breaches, onEmerged, onEmergeStart, breachAgain };
}

/** Breaches again when the hero comes back into view after leaving it. */
function useBreachOnReturn(target: React.RefObject<HTMLElement | null>, breachAgain: () => void) {
  const latest = useRef(breachAgain);
  useEffect(() => {
    latest.current = breachAgain;
  });
  useEffect(() => {
    const element = target.current;
    if (!element) return;
    let wasAway = false;
    const observer = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) wasAway = true;
      else if (wasAway) {
        wasAway = false;
        latest.current();
      }
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [target]);
}

/** The leap's frames side by side, stepped through by moving the strip. */
function LeapStrip() {
  return (
    <picture>
      <source srcSet="/art/breach-leap-dark.webp" media="(prefers-color-scheme: dark)" />
      <img
        src="/art/breach-leap-light.webp"
        alt=""
        decoding="async"
        fetchPriority="low"
        className="h-full max-w-none motion-safe:animate-leap-frames"
        style={{ width: `${LEAP_FRAMES * 100}%` }}
      />
    </picture>
  );
}

const AFTER_LEAP_STYLE: Partial<Record<Phase, string>> = {
  leaping: "",
  under: "translate-y-[3%] opacity-60 transition-[translate,opacity] duration-[2500ms] ease-out-soft",
  sinking: "translate-y-[3%] opacity-0 transition-opacity duration-300",
};

type WhaleProps = {
  phase: Phase;
  priority?: boolean;
  onEmergeStart?: () => void;
  onEmerged?: () => void;
};

/** One copy of the whale against the surface. The hero draws it twice,
    once above the surface and once below it through the water. */
function Whale({ phase, priority, onEmergeStart, onEmerged }: WhaleProps) {
  const resting = phase === "emerging";
  const afterLeap = AFTER_LEAP_STYLE[phase];
  return (
    <div className="absolute top-(--whale-top) left-(--whale-left) w-(--whale-w)">
      <div
        className={phase === "emerging" ? "motion-safe:animate-emerge" : ""}
        onAnimationStart={(event) => event.animationName === "emerge" && onEmergeStart?.()}
        onAnimationEnd={(event) => event.animationName === "emerge" && onEmerged?.()}
      >
        <InkImage
          light={breachLight}
          dark={breachDark}
          alt=""
          sizes="(min-width: 1024px) 40vw, 80vw"
          priority={priority}
          className={`pointer-events-none h-auto w-full select-none ${resting ? "" : "invisible"}`}
        />
      </div>
      {afterLeap !== undefined && (
        <div aria-hidden className={`absolute inset-0 overflow-hidden ${afterLeap}`}>
          <LeapStrip />
        </div>
      )}
    </div>
  );
}

/** The distortion under the surface: the whale seen through moving water. */
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

/** The app icon made into a page: the name set huge beside the lines of a
    note, which a humpback breaches out of as the page opens. It rolls onto
    its back, falls in and ripples the lines, then waits under the surface;
    touching the water, or coming back to the top, brings it up again. */
export function BreachHero({ children }: { children: ReactNode }) {
  const stage = useRef<HTMLDivElement>(null);
  const sea = useRef<RippleLinesHandle>(null);
  const water = useRef<HTMLDivElement>(null);
  const whale = useRef<HTMLDivElement>(null);

  const splash = (drawingShare: number, strength: number) => {
    const lines = water.current?.getBoundingClientRect();
    const drawing = whale.current?.getBoundingClientRect();
    if (!lines || !drawing) return;
    const x = drawing.left + drawing.width * drawingShare;
    sea.current?.drop((x - lines.left) / lines.width, strength);
  };
  const { phase, breaches, onEmerged, onEmergeStart, breachAgain } = useBreach(splash);
  useBreachOnReturn(stage, breachAgain);

  return (
    <section className="relative overflow-x-clip">
      <WaterFilter />
      <div
        ref={stage}
        style={stageGeometry}
        className="relative mx-auto flex max-w-7xl flex-col px-4 pt-4 pb-20 [--cross:46%] [--whale-w:min(80vw,26rem)] sm:px-8 md:[--cross:78%] md:[--whale-w:min(46vw,28rem)] lg:[--cross:70%] lg:[--whale-w:min(40vw,40rem)]"
      >
        <h1 className="wordmark order-1 select-none">Gasp</h1>

        <p className="lede relative z-10 order-4 mt-8 max-w-[26rem] text-ink sm:text-2xl lg:order-2 lg:mt-6 lg:max-w-[40%]">
          A Markdown editor optimized for speed and efficiency.
          <Caret />
        </p>

        <div
          ref={water}
          className="relative order-3 mt-[calc(var(--whale-h)*(1-var(--under)-var(--headroom))+1rem)] md:mt-10"
        >
          <div className="pointer-events-none absolute inset-x-0 bottom-full h-(--whale-h) [clip-path:inset(-100vh_-50vw_0_-50vw)]">
            <div className="absolute inset-x-0 top-full h-0">
              <div ref={whale} className="absolute top-(--whale-top) left-(--whale-left) h-(--whale-h) w-(--whale-w)" />
              <Whale
                key={breaches}
                phase={phase}
                priority={breaches === 0}
                onEmergeStart={onEmergeStart}
                onEmerged={onEmerged}
              />
            </div>
          </div>

          <button
            type="button"
            aria-label="Make the whale breach"
            onClick={breachAgain}
            className={`relative z-10 block min-h-[calc(var(--whale-h)*var(--under)*0.7)] w-full text-left ${phase === "under" ? "cursor-pointer" : "cursor-default"}`}
          >
            <RippleLines ref={sea} lengths={SEA_LENGTHS} playful />
          </button>

          <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 -bottom-8 [clip-path:inset(0_-50vw_0_-50vw)]">
            <div className="absolute inset-x-0 -top-6 bottom-0 [filter:url(#water)]">
              <div className="absolute inset-x-0 top-6 h-0">
                <Whale key={breaches} phase={phase} />
              </div>
            </div>
            <div className="absolute inset-0 bg-paper/65" />
          </div>
        </div>

        <div className="relative z-10 order-5 mt-10">{children}</div>
      </div>
    </section>
  );
}
