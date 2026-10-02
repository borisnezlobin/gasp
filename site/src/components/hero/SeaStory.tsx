"use client";

import { useEffect, useRef, useState, type ReactNode, type RefObject } from "react";
import { Caret } from "../Caret";

type StoryRefs = {
  section: RefObject<HTMLElement | null>;
  canvas: RefObject<HTMLCanvasElement | null>;
  hero: RefObject<HTMLHeadingElement | null>;
  final: RefObject<HTMLParagraphElement | null>;
};

/** How far through its scroll the story is, 0 at the top of the page and 1
    when the section is about to let go of the screen. */
function progressOf(section: HTMLElement | null): number {
  if (!section) return 0;
  const box = section.getBoundingClientRect();
  const travel = box.height - window.innerHeight;
  if (travel <= 0) return 0;
  return Math.min(1, Math.max(0, -box.top / travel));
}

/** Mirrors the scroll progress into a CSS variable, so the page's own type
    can fade with the story without React rendering every frame. */
function useStoryVariable(section: RefObject<HTMLElement | null>) {
  useEffect(() => {
    const update = () => section.current?.style.setProperty("--story", progressOf(section.current).toFixed(4));
    update();
    window.addEventListener("scroll", update, { passive: true });
    window.addEventListener("resize", update);
    return () => {
      window.removeEventListener("scroll", update);
      window.removeEventListener("resize", update);
    };
  }, [section]);
}

/** Starts the 3D story once the page has painted, rebuilding it when the
    system switches between light and dark. three.js loads only here. */
function useSeaScene({ section, canvas, hero, final }: StoryRefs) {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const scheme = window.matchMedia("(prefers-color-scheme: dark)");
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    let current: { dispose: () => void } | null = null;
    let cancelled = false;
    const build = async () => {
      current?.dispose();
      const { createSeaScene } = await import("./scene");
      if (cancelled) return;
      current = await createSeaScene(element, {
        dark: scheme.matches,
        reducedMotion,
        progress: () => progressOf(section.current),
        text: { hero: hero.current, final: final.current },
        onReady: () => setReady(true),
      });
    };
    void build();
    const rebuild = () => void build();
    scheme.addEventListener("change", rebuild);
    return () => {
      cancelled = true;
      scheme.removeEventListener("change", rebuild);
      current?.dispose();
    };
  }, [section, canvas, hero, final]);
  return ready;
}

/** The first screen and the story under it. At the top, a note's words
    float on water with the humpback beneath: the pointer ripples them and
    a click makes it breach. Scrolling dives under, swims with the whale,
    follows its breach and lands the words as the name. The display type is
    drawn by the scene so it bends with the water; the HTML copy stays for
    screen readers and as the fallback. */
export function SeaStory({ children }: { children: ReactNode }) {
  const section = useRef<HTMLElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const hero = useRef<HTMLHeadingElement>(null);
  const final = useRef<HTMLParagraphElement>(null);
  useStoryVariable(section);
  const ready = useSeaScene({ section, canvas, hero, final });
  const drawnByScene = ready ? "text-transparent" : "";
  return (
    <section ref={section} className="relative -mt-18 h-[520svh] motion-reduce:h-svh">
      <div className="sticky top-0 h-svh overflow-hidden">
        <div className="pointer-events-none absolute inset-x-0 top-0 mx-auto max-w-7xl px-4 pt-20 sm:px-8">
          <h1 ref={hero} className={`wordmark select-none ${drawnByScene}`}>
            Gasp
          </h1>
        </div>
        <p
          ref={final}
          aria-hidden
          className={`story-line pointer-events-none absolute inset-x-0 top-[13svh] text-center max-sm:top-[33svh] select-none ${ready ? "text-transparent" : "opacity-0"}`}
        >
          It will make you
        </p>
        <canvas
          ref={canvas}
          aria-label="A page of words floating on water, with a humpback whale swimming beneath. Click the water to make it breach, or scroll to dive in."
          role="img"
          className={`absolute inset-0 block size-full cursor-pointer touch-pan-y transition-opacity duration-700 ease-out-soft ${ready ? "opacity-100" : "opacity-0"}`}
        />
        <div className="pointer-events-none relative mx-auto flex h-full max-w-7xl flex-col justify-end px-4 pb-10 opacity-[clamp(0,calc(1-var(--story,0)*16),1)] sm:px-8 sm:pb-14">
          <div className="flex flex-col items-start gap-6">
            <p className="lede max-w-[28rem] rounded-xl bg-paper/80 px-1 text-ink backdrop-blur-[2px] sm:text-2xl">
              A Markdown editor for Mac that&apos;s ready to edit in under half a second.
              <Caret />
            </p>
            <div className="pointer-events-auto">{children}</div>
          </div>
        </div>
      </div>
    </section>
  );
}
