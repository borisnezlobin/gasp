"use client";

import type { StaticImageData } from "next/image";
import { useState, type KeyboardEvent } from "react";
import mathDark from "@/assets/shot-math-dark.png";
import mathLight from "@/assets/shot-math-light.png";
import notesDark from "@/assets/shot-notes-dark.png";
import notesLight from "@/assets/shot-notes-light.png";
import paletteDark from "@/assets/shot-palette-dark.png";
import paletteLight from "@/assets/shot-palette-light.png";
import searchDark from "@/assets/shot-search-dark.png";
import searchLight from "@/assets/shot-search-light.png";
import { InkImage } from "./InkImage";

type Shot = {
  id: string;
  label: string;
  caption: string;
  alt: string;
  light: StaticImageData;
  dark: StaticImageData;
};

const SHOTS: Shot[] = [
  {
    id: "notes",
    label: "Notes",
    caption: "Tables, callouts and highlights are formatted in place, and the Markdown comes back on the line you're editing.",
    alt: "A note with a heading, a highlighted phrase, a table of recording sites and a note callout.",
    light: notesLight,
    dark: notesDark,
  },
  {
    id: "math",
    label: "Math and code",
    caption: "Every equation is plain LaTeX between dollar signs in a normal Markdown file, so the same note opens in any other Markdown editor.",
    alt: "Fourier series study notes in Gasp: Parseval's identity worked down to π²/8, the discrete Fourier transform and its inverse, the 4×4 DFT matrix, and a short NumPy check.",
    light: mathLight,
    dark: mathDark,
  },
  {
    id: "search",
    label: "Search",
    caption: "⌘⇧F searches every note in the vault as you type, and shows each line that matched.",
    alt: "Vault search for the word song, listing four notes and the matching line from each.",
    light: searchLight,
    dark: searchDark,
  },
  {
    id: "commands",
    label: "Commands",
    caption: "Letters only need to come in order, so “snw” finds Sync now. Commands you ran recently rise to the top.",
    alt: "The command palette filtered to sync, with Sync now and its shortcut at the top.",
    light: paletteLight,
    dark: paletteDark,
  },
];

const STEP_KEYS: Record<string, number> = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 };

function ShotTab({ shot, selected, onSelect }: { shot: Shot; selected: boolean; onSelect: () => void }) {
  return (
    <button
      id={`shot-tab-${shot.id}`}
      type="button"
      role="tab"
      aria-selected={selected}
      aria-controls="shot-panel"
      tabIndex={selected ? 0 : -1}
      onClick={onSelect}
      className={`shrink-0 cursor-pointer rounded-xl px-4 py-2.5 text-left transition-colors duration-150 lg:w-full lg:py-4 ${selected ? "bg-surface shadow-lifted" : "hover:bg-fill"}`}
    >
      <span className={`block font-bold ${selected ? "text-ink" : "text-ink-soft"}`}>{shot.label}</span>
      <span className="small mt-1 hidden text-ink-muted lg:block">{shot.caption}</span>
    </button>
  );
}

/** The screens as a list beside the picture on wide windows, each with its
    caption, so the list, the picture and what it shows fit on one screen.
    Narrow windows put the names in a row and the caption under the
    picture. */
export function ScreenshotTabs() {
  const [selected, setSelected] = useState(0);
  const shot = SHOTS[selected];

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = STEP_KEYS[event.key];
    if (!step) return;
    event.preventDefault();
    const next = (selected + step + SHOTS.length) % SHOTS.length;
    setSelected(next);
    document.getElementById(`shot-tab-${SHOTS[next].id}`)?.focus();
  };

  return (
    <div className="lg:grid lg:grid-cols-[18rem_minmax(0,1fr)] lg:items-start lg:gap-10">
      <div
        role="tablist"
        aria-label="Screens"
        aria-orientation="vertical"
        onKeyDown={onKeyDown}
        className="-mx-4 flex gap-1 overflow-x-auto px-4 pb-2 [scrollbar-width:none] lg:mx-0 lg:flex-col lg:gap-2 lg:overflow-visible lg:px-0 lg:pb-0"
      >
        {SHOTS.map((each, index) => (
          <ShotTab key={each.id} shot={each} selected={index === selected} onSelect={() => setSelected(index)} />
        ))}
      </div>
      <div id="shot-panel" role="tabpanel" aria-labelledby={`shot-tab-${shot.id}`} className="mt-4 lg:mt-0">
        <div className="relative aspect-[1280/820] overflow-hidden rounded-2xl bg-surface shadow-lifted">
          {SHOTS.map((each, index) => (
            <div key={each.id} className={index === selected ? "" : "hidden"}>
              <InkImage
                light={each.light}
                dark={each.dark}
                alt={each.alt}
                sizes="(min-width: 1024px) 56rem, 100vw"
                className="h-auto w-full"
              />
            </div>
          ))}
        </div>
        <p className="body mt-4 min-h-[3.3em] text-ink-soft lg:hidden">{shot.caption}</p>
      </div>
    </div>
  );
}
