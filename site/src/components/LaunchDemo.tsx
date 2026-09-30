"use client";

import { ArrowCounterClockwise } from "@phosphor-icons/react";
import Image from "next/image";
import { useEffect, useRef, useState } from "react";
import icon from "@/app/icon.png";
import notesDark from "@/assets/shot-notes-dark.png";
import notesLight from "@/assets/shot-notes-light.png";
import { InkImage } from "./InkImage";

const OPEN_MS = 300;
const RULER_MS = 1000;
const TICKS = [0, 250, 500, 750, 1000];
/** The range usually given for one blink of an eye. */
const BLINK_MS: [number, number] = [100, 400];
/** A pause after the demo scrolls into view, so the launch is seen. */
const AUTOPLAY_DELAY_MS = 700;

type Phase = "waiting" | "opening" | "open";

const share = (ms: number) => `${(ms / RULER_MS) * 100}%`;

function useLaunchClock() {
  const [phase, setPhase] = useState<Phase>("waiting");
  const [elapsed, setElapsed] = useState(0);
  const frame = useRef(0);

  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  const start = () => {
    cancelAnimationFrame(frame.current);
    const began = performance.now();
    setPhase("opening");
    setElapsed(0);
    const tick = (now: number) => {
      const ms = Math.min(now - began, OPEN_MS);
      setElapsed(ms);
      if (ms < OPEN_MS) {
        frame.current = requestAnimationFrame(tick);
        return;
      }
      setPhase("open");
    };
    frame.current = requestAnimationFrame(tick);
  };

  return { phase, elapsed, start };
}

/** Runs `start` once, a moment after `target` is mostly on screen. */
function useAutoplay(target: React.RefObject<HTMLElement | null>, start: () => void) {
  const begin = useRef(start);
  useEffect(() => {
    begin.current = start;
  });
  useEffect(() => {
    const element = target.current;
    if (!element || window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    let timer = 0;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.disconnect();
        timer = window.setTimeout(() => begin.current(), AUTOPLAY_DELAY_MS);
      },
      { threshold: 0.6 },
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      window.clearTimeout(timer);
    };
  }, [target]);
}

function tickPlacement(ms: number): string {
  if (ms === 0) return "";
  if (ms === RULER_MS) return "-translate-x-full";
  return "-translate-x-1/2";
}

function Ruler({ elapsed }: { elapsed: number }) {
  return (
    <div className="relative mt-6 h-16" aria-hidden>
      <div
        className="absolute top-0 h-3 rounded-full bg-fill"
        style={{ left: share(BLINK_MS[0]), width: share(BLINK_MS[1] - BLINK_MS[0]) }}
      />
      <div className="absolute top-1 h-1 w-full rounded-full bg-sea" />
      <div className="absolute top-1 h-1 rounded-full bg-ink" style={{ width: share(elapsed) }} />
      <div
        className="absolute -top-1 h-5 w-[3px] -translate-x-1/2 rounded-full bg-caret"
        style={{ left: share(elapsed) }}
      />
      <span className="small absolute top-5 whitespace-nowrap text-ink-muted" style={{ left: share(BLINK_MS[0]) }}>
        One blink
      </span>
      {TICKS.map((ms) => (
        <span
          key={ms}
          className={`small figure absolute top-10 whitespace-nowrap text-ink-muted ${tickPlacement(ms)}`}
          style={{ left: share(ms) }}
        >
          {ms === RULER_MS ? "1 s" : `${ms} ms`}
        </span>
      ))}
    </div>
  );
}

function Launcher({ phase, elapsed, onOpen }: { phase: Phase; elapsed: number; onOpen: () => void }) {
  const opening = phase === "opening";
  return (
    <button
      type="button"
      onClick={onOpen}
      disabled={opening}
      className="group absolute inset-0 flex cursor-pointer flex-col items-center justify-center gap-3"
    >
      <Image
        src={icon}
        alt=""
        width={128}
        height={128}
        className={`size-24 transition-transform duration-150 ease-out-soft group-hover:scale-105 sm:size-32 ${opening ? "scale-95" : ""}`}
      />
      <span className="figure small font-bold text-ink-soft">{opening ? `${Math.round(elapsed)} ms` : "Open Gasp"}</span>
    </button>
  );
}

/** Replays the app's launch at its real speed: the window, with the note
    in it, appears 300 ms after the icon is pressed, while a ruler counts.
    It plays once by itself when it scrolls into view. */
export function LaunchDemo() {
  const { phase, elapsed, start } = useLaunchClock();
  const stage = useRef<HTMLDivElement>(null);
  useAutoplay(stage, start);
  const isOpen = phase === "open";

  return (
    <div ref={stage} className="mx-auto max-w-5xl">
      <div className="relative aspect-[1280/820] overflow-hidden rounded-2xl bg-surface shadow-lifted">
        <InkImage
          light={notesLight}
          dark={notesDark}
          alt="Gasp with a note on humpback song open: a heading, a table of recordings and a callout."
          sizes="(min-width: 1024px) 64rem, 100vw"
          loading="eager"
          className={`h-full w-full object-cover ${isOpen ? "" : "invisible"}`}
        />
        {!isOpen && <Launcher phase={phase} elapsed={elapsed} onOpen={start} />}
      </div>
      <Ruler elapsed={elapsed} />
      <div className="flex min-h-10 items-center justify-between gap-4" aria-live="polite">
        <p className="body text-ink-soft">
          {isOpen ? "Open, with your note on screen, in 300 ms." : "This replays the launch at its real speed."}
        </p>
        <button
          type="button"
          onClick={start}
          className={`inline-flex h-10 shrink-0 cursor-pointer items-center gap-2 rounded-lg px-3 font-bold hover:bg-fill ${isOpen ? "" : "invisible"}`}
        >
          <ArrowCounterClockwise size={18} aria-hidden />
          Again
        </button>
      </div>
    </div>
  );
}
