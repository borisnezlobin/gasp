"use client";

import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import breachDark from "@/assets/breach-dark.png";
import breachLight from "@/assets/breach-light.png";
import { Caret } from "./Caret";
import { InkImage } from "./InkImage";

/** How far the whale leans towards the pointer, at most. */
const LEAN_DEGREES = 6;
/** The lines under the first one, as the app icon draws them. */
const SEA_LINES = ["96%", "88%", "64%", "92%", "40%"];

/** From the tour (`src/tour/hello.rs`): the body crosses the surface a
    quarter of the way in from the drawing's left edge, and a third of it
    stays under. */
const stageGeometry = {
  "--crossing": 0.24,
  "--under": 0.34,
  "--whale-h": "calc(var(--whale-w) * 873 / 900)",
  "--whale-left": "calc(var(--cross) - var(--whale-w) * var(--crossing))",
  "--whale-top": "calc(var(--whale-h) * (var(--under) - 1))",
} as CSSProperties;

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
      const whaleX = box.left + box.width * 0.72;
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

/** One copy of the whale, placed against the surface. The hero draws it
    twice, once above the surface and once below it through the water. */
function Whale({ leaping, priority }: { leaping: boolean; priority?: boolean }) {
  return (
    <div className="absolute top-(--whale-top) left-(--whale-left) w-(--whale-w) motion-safe:animate-rise">
      <div className="translate-y-[calc(var(--breach,0)*-14vh)] rotate-[calc(var(--breach,0)*16deg)] will-change-transform">
        <div className={leaping ? "motion-safe:animate-leap" : ""}>
          <div className="origin-[24%_66%] rotate-(--lean,0deg) transition-transform duration-700 ease-out-soft">
            <InkImage
              light={breachLight}
              dark={breachDark}
              alt=""
              sizes="(min-width: 1024px) 40vw, 72vw"
              priority={priority}
              className="pointer-events-none h-auto w-full select-none"
            />
          </div>
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

/** The app icon made into a page: the name set huge behind the humpback,
    which breaches out of the lines of a note. It rises as the page opens,
    leans towards the pointer, arcs over as the page scrolls and leaps when
    clicked, as it does in the app's welcome tour. */
export function BreachHero({ children }: { children: ReactNode }) {
  const stage = useStageMotion();
  const [leaping, setLeaping] = useState(false);

  return (
    <section className="relative overflow-x-clip">
      <WaterFilter />
      <div
        ref={stage}
        style={stageGeometry}
        className="relative mx-auto flex max-w-7xl flex-col px-4 pt-4 pb-20 [--cross:52%] [--whale-w:min(62vw,20rem)] sm:px-8 md:[--cross:60%] md:[--whale-w:min(48vw,28rem)] lg:[--cross:64%] lg:[--whale-w:min(40vw,36rem)]"
      >
        <h1 className="wordmark order-1 translate-y-[calc(var(--breach,0)*8vh)] select-none">Gasp</h1>

        <p className="lede relative z-10 order-4 mt-8 max-w-[26rem] text-ink sm:text-2xl lg:order-2 lg:mt-6 lg:max-w-[40%]">
          A Markdown editor optimized for speed and efficiency.
          <Caret />
        </p>

        <div
          className="relative order-3 mt-[max(2rem,calc(var(--whale-h)*(1-var(--under))-var(--wordmark-size)*0.7))] lg:mt-10"
          onAnimationEnd={(event) => event.animationName === "leap" && setLeaping(false)}
        >
          <div className="absolute inset-x-0 top-(--whale-top) h-[calc(var(--whale-top)*-1)] overflow-hidden">
            <div className="absolute inset-x-0 top-[calc(var(--whale-top)*-1)] h-0">
              <Whale leaping={leaping} priority />
            </div>
          </div>

          <div className="relative flex min-h-[calc(var(--whale-h)*var(--under)+1rem)] flex-col gap-5">
            {SEA_LINES.map((width) => (
              <div key={width} aria-hidden className="sea-line relative z-10" style={{ width }} />
            ))}
          </div>

          <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 -bottom-8 [clip-path:inset(0_0_0_0)]">
            <div className="absolute inset-x-0 -top-6 bottom-0 [filter:url(#water)]">
              <div className="absolute inset-x-0 top-6 h-0">
                <Whale leaping={leaping} />
              </div>
            </div>
            <div className="absolute inset-0 bg-paper/65" />
          </div>

          <button
            type="button"
            aria-label="Make the whale leap"
            onClick={() => setLeaping(true)}
            className="absolute top-[calc(var(--whale-top)*0.9)] left-[calc(var(--cross)-var(--whale-w)*0.05)] z-20 h-[calc(var(--whale-h)*0.6)] w-[calc(var(--whale-w)*0.5)] cursor-pointer rounded-full"
          />
        </div>

        <div className="relative z-10 order-5 mt-10">{children}</div>
      </div>
    </section>
  );
}
