"use client";

import { ArrowCounterClockwise } from "@phosphor-icons/react";
import { useEffect, useRef, useState } from "react";

type Racer = {
  name: string;
  video: string;
  /** Seconds from the launch to the window's first frame on screen. */
  appears: number;
  /** Seconds from the launch to a note on screen, ready to edit. */
  ready: number;
};

/** Measured frame by frame from screen recordings on a MacBook Pro (M2 Pro),
    each app quit first and then opened, timed from the launch command. */
const RACERS: Racer[] = [
  { name: "Gasp", video: "/race/gasp.mp4", appears: 0.417, ready: 0.42 },
  { name: "Apple Notes", video: "/race/notes.mp4", appears: 0.767, ready: 1.53 },
  { name: "Obsidian", video: "/race/obsidian.mp4", appears: 0.864, ready: 4.66 },
];

const RACE_SECONDS = Math.max(...RACERS.map((racer) => racer.ready)) + 0.4;
const AUTOPLAY_DELAY_MS = 400;

const prefersReducedMotion = () => window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/** One clock for all three windows: each video starts the moment its app's
    window appeared in the recording, so the three stay in step. */
function useRaceClock(videos: React.RefObject<(HTMLVideoElement | null)[]>) {
  const [elapsed, setElapsed] = useState<number | null>(null);
  const frame = useRef(0);

  const showFinish = () => {
    videos.current.forEach((video) => {
      if (!video) return;
      video.pause();
      video.currentTime = Math.max(0, video.duration - 0.05) || 0;
    });
    setElapsed(RACE_SECONDS);
  };

  const start = () => {
    cancelAnimationFrame(frame.current);
    if (prefersReducedMotion()) return showFinish();
    videos.current.forEach((video) => {
      if (!video) return;
      video.pause();
      video.currentTime = 0;
    });
    const began = performance.now();
    const started = new Set<number>();
    const tick = (now: number) => {
      const seconds = (now - began) / 1000;
      RACERS.forEach((racer, index) => {
        if (started.has(index) || seconds < racer.appears) return;
        started.add(index);
        void videos.current[index]?.play();
      });
      setElapsed(Math.min(seconds, RACE_SECONDS));
      if (seconds < RACE_SECONDS) frame.current = requestAnimationFrame(tick);
    };
    frame.current = requestAnimationFrame(tick);
  };

  useEffect(() => () => cancelAnimationFrame(frame.current), []);
  return { elapsed, start };
}

function useStartWhenSeen(target: React.RefObject<HTMLElement | null>, start: () => void) {
  const begin = useRef(start);
  useEffect(() => {
    begin.current = start;
  });
  useEffect(() => {
    const element = target.current;
    if (!element) return;
    let timer = 0;
    const observer = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        observer.disconnect();
        timer = window.setTimeout(() => begin.current(), AUTOPLAY_DELAY_MS);
      },
      { threshold: 0.5 },
    );
    observer.observe(element);
    return () => {
      observer.disconnect();
      window.clearTimeout(timer);
    };
  }, [target]);
}

function Lane({ racer, elapsed, video }: { racer: Racer; elapsed: number | null; video: Ref }) {
  const seconds = elapsed ?? 0;
  const done = elapsed !== null && seconds >= racer.ready;
  const shown = done ? racer.ready : seconds;
  const windowUp = elapsed !== null && seconds >= racer.appears;
  return (
    <li className="min-w-0">
      <div className="relative aspect-[3/4] overflow-hidden rounded-xl bg-fill sm:aspect-[3/2]">
        <video
          ref={video}
          src={racer.video}
          muted
          playsInline
          preload="auto"
          aria-hidden
          className={`absolute inset-0 h-full w-full object-cover object-left-top ${windowUp ? "" : "invisible"}`}
        />
      </div>
      <p className="mt-3 flex flex-wrap items-baseline justify-between gap-x-3">
        <span className="font-bold">{racer.name}</span>
        <span className={`figure text-2xl font-bold sm:text-3xl ${done ? "text-ink" : "text-ink-muted"}`}>
          {shown.toFixed(2)} s
        </span>
      </p>
    </li>
  );
}

type Ref = (element: HTMLVideoElement | null) => void;

/** Gasp, Apple Notes and Obsidian opened side by side, replayed from
    screen recordings at their real speed, each with a clock that stops
    when a note is on screen. */
export function LaunchRace() {
  const videos = useRef<(HTMLVideoElement | null)[]>([]);
  const stage = useRef<HTMLDivElement>(null);
  const { elapsed, start } = useRaceClock(videos);
  useStartWhenSeen(stage, start);
  const finished = elapsed !== null && elapsed >= RACE_SECONDS;

  return (
    <div ref={stage}>
      <ol className="grid grid-cols-3 gap-3 sm:gap-6" aria-label="Seconds from launch to a note on screen">
        {RACERS.map((racer, index) => (
          <Lane
            key={racer.name}
            racer={racer}
            elapsed={elapsed}
            video={(element) => {
              videos.current[index] = element;
            }}
          />
        ))}
      </ol>
      <div className="mt-6 flex flex-wrap items-center justify-between gap-4">
        <p className="small max-w-2xl text-ink-muted">
          Recorded on a MacBook Pro with M2 Pro. Each app was quit, then opened, and timed from launch to a
          note on screen. Notes and Obsidian show real notes, blurred.
        </p>
        <button
          type="button"
          onClick={start}
          disabled={!finished}
          className="inline-flex h-10 shrink-0 cursor-pointer items-center gap-2 rounded-lg px-3 font-bold transition-[background-color,opacity] duration-150 hover:bg-fill active:scale-[0.96] disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent"
        >
          <ArrowCounterClockwise size={18} aria-hidden />
          Replay
        </button>
      </div>
    </div>
  );
}
