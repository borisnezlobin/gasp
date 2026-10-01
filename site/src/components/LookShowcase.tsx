"use client";

import Image, { type StaticImageData } from "next/image";
import { useEffect, useRef, useState } from "react";
import asItComesDark from "@/assets/looks/as-it-comes-dark.webp";
import asItComesLight from "@/assets/looks/as-it-comes-light.webp";
import filesDark from "@/assets/looks/files-dark.webp";
import filesLight from "@/assets/looks/files-light.webp";
import markdownDark from "@/assets/looks/markdown-dark.webp";
import markdownLight from "@/assets/looks/markdown-light.webp";
import selectionDark from "@/assets/looks/selection-dark.webp";
import selectionLight from "@/assets/looks/selection-light.webp";
import sentencesDark from "@/assets/looks/sentences-dark.webp";
import sentencesLight from "@/assets/looks/sentences-light.webp";

type SettingsFile = { path: string; text: string };

type Look = {
  id: string;
  request: string;
  quoted: boolean;
  file?: SettingsFile;
  alt: string;
  light: StaticImageData;
  dark: StaticImageData;
};

const LOOKS: Look[] = [
  {
    id: "as-it-comes",
    request: "Gasp as it comes",
    quoted: false,
    alt: "A trip-planning note in Gasp with its default settings: Markdown symbols hidden, a tip callout, a table of days, a packing checklist and a short Python block.",
    light: asItComesLight,
    dark: asItComesDark,
  },
  {
    id: "markdown",
    request: "Show me the Markdown while I write",
    quoted: true,
    file: { path: ".gasp/settings.toml", text: '[markdown.symbols]\nmode = "always-shown"' },
    alt: "The same note with every Markdown symbol showing: the asterisks around Alfama, the equals signs around the highlight, the link's address, the table's pipes and the code fence.",
    light: markdownLight,
    dark: markdownDark,
  },
  {
    id: "sentences",
    request: "Highlight my long sentences",
    quoted: true,
    file: { path: ".gasp/settings.toml", text: "[prose.sentence-length]\nenabled = true" },
    alt: "The same note with each sentence tinted by length: the long sentence about the palaces in red, medium ones in amber and short ones in blue.",
    light: sentencesLight,
    dark: sentencesDark,
  },
  {
    id: "selection",
    request: "Give me buttons when I select text",
    quoted: true,
    file: { path: ".gasp/toolbars.toml", text: "[toolbar.selection]\nenabled = true" },
    alt: "The same note with “before the crowds” selected and a small bar above it holding bold, italic, highlight, link and code buttons.",
    light: selectionLight,
    dark: selectionDark,
  },
  {
    id: "files",
    request: "Keep my file list open",
    quoted: true,
    file: { path: ".gasp/settings.toml", text: '[sidebar.files]\nreveal = "always"\nmode   = "push"' },
    alt: "The same note with the file list open on the left, showing the Recipes and Travel folders, with Trip to Lisbon selected inside Travel.",
    light: filesLight,
    dark: filesDark,
  },
];

const LAST_LOOK = LOOKS.length - 1;

/** How much of the scroll between two looks one crossfade takes. */
const FADE_SPAN = 0.4;

const REDUCED_MOTION = "(prefers-reduced-motion: reduce)";

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

/** Where the reader is through the looks, from 0 (the first) to the last
    look's index, measured by how far the tall wrapper has scrolled past. */
function positionThrough(wrapper: HTMLElement): number {
  const travel = wrapper.offsetHeight - window.innerHeight;
  if (travel <= 0) return 0;
  return clamp(-wrapper.getBoundingClientRect().top / travel, 0, 1) * LAST_LOOK;
}

/** Each look sits over the ones before it and fades in halfway between its
    neighbour and itself, so only two pictures are ever part-visible. */
function opacityOf(index: number, position: number, reduced: boolean): number {
  if (index === 0) return 1;
  const midpoint = index - 0.5;
  if (reduced) return position >= midpoint ? 1 : 0;
  return clamp((position - midpoint) / FADE_SPAN + 0.5, 0, 1);
}

function scrollToLook(wrapper: HTMLElement, index: number) {
  const travel = wrapper.offsetHeight - window.innerHeight;
  const wrapperTop = wrapper.getBoundingClientRect().top + window.scrollY;
  const reduced = window.matchMedia(REDUCED_MOTION).matches;
  window.scrollTo({ top: wrapperTop + (travel * index) / LAST_LOOK, behavior: reduced ? "auto" : "smooth" });
}

/** Decodes the pictures the current theme shows once the section is near,
    so no crossfade waits on a decode. */
function decodeVisibleImages(wrapper: HTMLElement) {
  wrapper.querySelectorAll("img").forEach((image) => {
    if (image.getClientRects().length > 0) image.decode().catch(() => {});
  });
}

function useScrollPosition(wrapperRef: React.RefObject<HTMLDivElement | null>, layerRefs: React.RefObject<(HTMLDivElement | null)[]>) {
  const [active, setActive] = useState(0);

  useEffect(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    const motion = window.matchMedia(REDUCED_MOTION);
    let frame = 0;

    const paint = () => {
      frame = 0;
      const position = positionThrough(wrapper);
      layerRefs.current.forEach((layer, index) => {
        if (layer) layer.style.opacity = String(opacityOf(index, position, motion.matches));
      });
      setActive(Math.round(position));
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(paint);
    };

    paint();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    motion.addEventListener("change", schedule);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
      motion.removeEventListener("change", schedule);
    };
  }, [wrapperRef, layerRefs]);

  return active;
}

function useDecodeWhenNear(wrapperRef: React.RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const wrapper = wrapperRef.current;
    if (!wrapper) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (!entries.some((entry) => entry.isIntersecting)) return;
        decodeVisibleImages(wrapper);
        observer.disconnect();
      },
      { rootMargin: "100% 0px" },
    );
    observer.observe(wrapper);
    return () => observer.disconnect();
  }, [wrapperRef]);
}

function requestText(look: Look): string {
  return look.quoted ? `“${look.request}”` : look.request;
}

function Requests({ active }: { active: number }) {
  return (
    <div className="grid">
      {LOOKS.map((look, index) => (
        <p
          key={look.id}
          aria-hidden={index !== active}
          className={`request col-start-1 row-start-1 transition-opacity duration-300 ease-out-soft motion-reduce:transition-none ${index === active ? "opacity-100" : "opacity-0"}`}
        >
          {requestText(look)}
        </p>
      ))}
    </div>
  );
}

function FileContents({ file }: { file?: SettingsFile }) {
  if (!file) return <p className="small text-ink-muted">No settings files, so every default applies.</p>;
  return (
    <>
      <p className="small code text-ink-muted">{file.path}</p>
      <pre className="code mt-2 rounded-xl bg-fill px-4 py-3 leading-relaxed whitespace-pre text-ink">{file.text}</pre>
    </>
  );
}

function Files({ active }: { active: number }) {
  return (
    <div className="grid">
      {LOOKS.map((look, index) => (
        <div
          key={look.id}
          aria-hidden={index !== active}
          className={`col-start-1 row-start-1 transition-opacity duration-300 ease-out-soft motion-reduce:transition-none ${index === active ? "opacity-100" : "opacity-0"}`}
        >
          <FileContents file={look.file} />
        </div>
      ))}
    </div>
  );
}

function Marks({ active, onPick }: { active: number; onPick: (index: number) => void }) {
  return (
    <div className="-mx-1 flex">
      {LOOKS.map((look, index) => (
        <button
          key={look.id}
          type="button"
          aria-label={`Jump to ${requestText(look)}`}
          aria-current={index === active}
          onClick={() => onPick(index)}
          className="group flex h-8 w-10 cursor-pointer items-center px-1"
        >
          <span
            className={`h-1 w-full rounded-full transition-colors duration-150 ${index === active ? "bg-ink" : "bg-sea group-hover:bg-ink-muted"}`}
          />
        </button>
      ))}
    </div>
  );
}

function Frame({ layerRefs }: { layerRefs: React.RefObject<(HTMLDivElement | null)[]> }) {
  return (
    <div className="relative mx-auto aspect-[1280/820] w-full max-w-[calc((100svh-15rem)*1.5625)] overflow-hidden rounded-2xl bg-surface shadow-lifted lg:max-w-[calc((100svh-6rem)*1.5625)]">
      {LOOKS.map((look, index) => (
        <div
          key={look.id}
          ref={(layer) => {
            layerRefs.current[index] = layer;
          }}
          style={{ opacity: index === 0 ? 1 : 0 }}
          className="absolute inset-0"
        >
          {[
            { src: look.light, theme: "dark:hidden" },
            { src: look.dark, theme: "hidden dark:block" },
          ].map(({ src, theme }) => (
            <Image
              key={theme}
              src={src}
              alt={look.alt}
              loading={index === 0 ? "eager" : "lazy"}
              sizes="(min-width: 1024px) 56rem, 100vw"
              className={`h-full w-full ${theme}`}
            />
          ))}
        </div>
      ))}
      <span aria-hidden className="pointer-events-none absolute inset-0 rounded-[inherit] ring-1 ring-image-outline ring-inset" />
    </div>
  );
}

/** One note drawn by Gasp with five different sets of settings files.
    Scrolling through the tall wrapper crossfades from one look to the next
    while the frame stays pinned, and the marks jump straight to a look. */
export function LookShowcase() {
  const wrapperRef = useRef<HTMLDivElement>(null);
  const layerRefs = useRef<(HTMLDivElement | null)[]>([]);
  const active = useScrollPosition(wrapperRef, layerRefs);
  useDecodeWhenNear(wrapperRef);

  const pick = (index: number) => {
    if (wrapperRef.current) scrollToLook(wrapperRef.current, index);
  };

  return (
    <div ref={wrapperRef} className="relative" style={{ height: `${LOOKS.length * 100}svh` }}>
      <div className="sticky top-0 flex h-svh flex-col justify-center gap-5 py-6 lg:grid lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)] lg:items-center lg:gap-12">
        <div className="flex flex-col gap-5 lg:gap-8">
          <Requests active={active} />
          <div className="hidden lg:block">
            <Files active={active} />
          </div>
          <div className="hidden lg:block">
            <Marks active={active} onPick={pick} />
          </div>
        </div>
        <Frame layerRefs={layerRefs} />
        <div className="flex flex-col gap-3 lg:hidden">
          <Files active={active} />
          <Marks active={active} onPick={pick} />
        </div>
        <p aria-live="polite" className="sr-only">
          {`Showing ${LOOKS[active].request}`}
        </p>
      </div>
    </div>
  );
}
