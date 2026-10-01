"use client";

import { useEffect, useRef, useState, type ReactNode } from "react";
import { Caret } from "../Caret";

/** Starts the 3D sea once the page has painted, rebuilding it when the
    system switches between light and dark. three.js loads only here. */
function useSeaScene(canvas: React.RefObject<HTMLCanvasElement | null>) {
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
      current = await createSeaScene(element, { dark: scheme.matches, reducedMotion, onReady: () => setReady(true) });
    };
    void build();
    const rebuild = () => void build();
    scheme.addEventListener("change", rebuild);
    return () => {
      cancelled = true;
      scheme.removeEventListener("change", rebuild);
      current?.dispose();
    };
  }, [canvas]);
  return ready;
}

/** The first screen: a note's words floating on water, the humpback under
    them. Moving the pointer ripples the words; a click makes it breach
    there. The name and the download sit over the water. */
export function SeaHero({ children }: { children: ReactNode }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const ready = useSeaScene(canvas);
  return (
    <section className="relative h-[calc(100svh-4.5rem)] min-h-[36rem] max-h-[62rem] overflow-hidden">
      <div className="pointer-events-none absolute inset-x-0 top-0 mx-auto max-w-7xl px-4 pt-2 sm:px-8">
        <h1 className="wordmark select-none">Gasp</h1>
      </div>
      <canvas
        ref={canvas}
        aria-label="A page of words floating on water, with a humpback whale swimming beneath. Click the water to make it breach."
        role="img"
        className={`absolute inset-0 block size-full cursor-pointer touch-pan-y transition-opacity duration-700 ease-out-soft ${ready ? "opacity-100" : "opacity-0"}`}
      />
      <div className="pointer-events-none relative mx-auto flex h-full max-w-7xl flex-col justify-end px-4 pb-10 sm:px-8 sm:pb-14">
        <div className="flex flex-col items-start gap-6">
          <p className="lede max-w-[28rem] rounded-xl bg-paper/80 px-1 text-ink backdrop-blur-[2px] sm:text-2xl">
            A Markdown editor for Mac that&apos;s ready to edit in under half a second.
            <Caret />
          </p>
          <div className="pointer-events-auto">{children}</div>
        </div>
      </div>
    </section>
  );
}
