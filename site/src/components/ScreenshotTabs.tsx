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
    caption: "LaTeX between dollar signs renders as you type, and code blocks are coloured by language.",
    alt: "The same note further down, with a rendered power-law equation, ticked tasks and a Python code block.",
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
    caption: "⌘P finds any command by a few of its letters, with its shortcut beside it.",
    alt: "The command palette filtered to sync, with Sync now and its shortcut at the top.",
    light: paletteLight,
    dark: paletteDark,
  },
];

const STEP_KEYS: Record<string, number> = { ArrowRight: 1, ArrowLeft: -1 };

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
    <div>
      <div role="tablist" aria-label="Screens" onKeyDown={onKeyDown} className="flex flex-wrap gap-1">
        {SHOTS.map((each, index) => {
          const isSelected = index === selected;
          return (
            <button
              key={each.id}
              id={`shot-tab-${each.id}`}
              type="button"
              role="tab"
              aria-selected={isSelected}
              aria-controls="shot-panel"
              tabIndex={isSelected ? 0 : -1}
              onClick={() => setSelected(index)}
              className={`h-10 shrink-0 cursor-pointer rounded-lg px-4 font-bold transition-colors ${isSelected ? "bg-button text-on-button" : "text-ink-soft hover:bg-fill"}`}
            >
              {each.label}
            </button>
          );
        })}
      </div>
      <div id="shot-panel" role="tabpanel" aria-labelledby={`shot-tab-${shot.id}`} className="mt-6">
        <div className="relative aspect-[1280/820] overflow-hidden rounded-2xl bg-surface shadow-lifted">
          {SHOTS.map((each, index) => (
            <div key={each.id} className={index === selected ? "" : "hidden"}>
              <InkImage
                light={each.light}
                dark={each.dark}
                alt={each.alt}
                sizes="(min-width: 1152px) 72rem, 100vw"
                className="h-auto w-full"
              />
            </div>
          ))}
        </div>
        <p className="body mt-4 max-w-2xl text-ink-soft">{shot.caption}</p>
      </div>
    </div>
  );
}
