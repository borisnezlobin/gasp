"use client";

import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import type { BuiltinDemo } from "@/lib/builtins";
import { COMMANDS } from "@/lib/gaspSchema";
import { defaultShortcut, macChord } from "@/lib/keyChords";
import { pixels, realPixels, token } from "@/lib/mockConfig";
import { AppIcon } from "./AppIcon";

const POPOVER: CSSProperties = {
  borderRadius: pixels("radius.lg"),
  boxShadow: `0 0 0 1px ${token("color.popover-ring")}, 0 ${realPixels(8)} ${realPixels(32)} ${token("color.popover-shadow")}`,
  fontFamily: token("font.ui"),
  fontSize: realPixels(14),
};

const APPEAR = "transition-[opacity,translate] duration-300 ease-out-soft starting:opacity-0 motion-safe:starting:-translate-y-1";

export function Keycap({ chord }: { chord: string }) {
  return (
    <kbd
      className="bg-(--g-color-fill-strong) px-[0.4em] text-(--g-color-text-muted)"
      style={{ fontFamily: token("font.ui"), borderRadius: pixels("radius.sm") }}
    >
      {macChord(chord)}
    </kbd>
  );
}

type Row = { icon: string; title: string; detail?: ReactNode; chord?: string };

function commandRow(id: string): Row {
  const command = COMMANDS.get(id);
  return { icon: command?.icon ?? "lightning", title: command?.title ?? id, chord: defaultShortcut(id) };
}

const noteRow = (title: string, folder: string): Row => ({ icon: "file-text", title, detail: folder });

const PICKERS: Partial<Record<BuiltinDemo, { query: string; rows: Row[] }>> = {
  palette: { query: "split", rows: ["pane.split-right", "pane.split-down", "pane.close", "pane.focus-right"].map(commandRow) },
  switcher: { query: "pa", rows: [noteRow("Packing", "Travel"), noteRow("Paris in May", "Travel"), noteRow("Pancakes", "Recipes")] },
  search: {
    query: "tram",
    rows: [
      { icon: "file-text", title: "Trip to Lisbon", detail: <SearchHit before="Take the " hit="tram" after=" up to the castle" /> },
      { icon: "file-text", title: "Packing", detail: <SearchHit before="Day pass for the " hit="tram" after=" and metro" /> },
    ],
  },
};

function SearchHit({ before, hit, after }: { before: string; hit: string; after: string }) {
  return (
    <>
      {before}
      <mark className="bg-(--g-color-search-match) text-inherit">{hit}</mark>
      {after}
    </>
  );
}

function PickerRow({ row, selected }: { row: Row; selected: boolean }) {
  return (
    <li
      className={`flex items-center gap-[0.6em] ${selected ? "bg-(--g-color-fill-strong)" : ""}`}
      style={{ borderRadius: pixels("radius.md"), padding: `${pixels("space.sm", 1.5)} ${pixels("space.lg")}` }}
    >
      <AppIcon name={row.icon} size={pixels("size.icon")} className="text-(--g-color-icon)" />
      <span className="min-w-0 flex-1 truncate text-(--g-color-text)">
        {row.title}
        {row.detail && <span className="ml-[0.6em] text-(--g-color-text-detail)">{row.detail}</span>}
      </span>
      {row.chord && <Keycap chord={row.chord} />}
    </li>
  );
}

function Picker({ query, rows }: { query: string; rows: Row[] }) {
  return (
    <div aria-hidden className={`absolute inset-x-[8%] top-[12%] z-30 bg-(--g-color-popover) ${APPEAR}`} style={POPOVER}>
      <div className="flex items-center gap-[0.6em] shadow-[0_1px_0_var(--g-color-divider)]" style={{ padding: pixels("space.lg") }}>
        <AppIcon name="magnifying-glass" size={pixels("size.icon")} className="text-(--g-color-icon)" />
        <span className="text-(--g-color-text)">{query}</span>
        <span className="-ml-[0.5em] h-[1.1em] w-[2px] bg-(--g-color-accent) animate-blink" />
      </div>
      <ul style={{ padding: pixels("space.sm", 1.5) }}>
        {rows.map((row, index) => (
          <PickerRow key={row.title} row={row} selected={index === 0} />
        ))}
      </ul>
    </div>
  );
}

const OUTLINE = [
  { title: "Plans", depth: 0 },
  { title: "To do", depth: 0 },
  { title: "Notes", depth: 0 },
];

function Outline() {
  return (
    <div aria-hidden className={`absolute top-[10%] right-[3%] z-30 w-[34%] bg-(--g-color-popover) ${APPEAR}`} style={{ ...POPOVER, padding: pixels("space.md") }}>
      <p className="text-(--g-color-text-detail)" style={{ padding: pixels("space.sm") }}>
        Trip to Lisbon
      </p>
      {OUTLINE.map(({ title }, index) => (
        <p
          key={title}
          className={`text-(--g-color-text) ${index === 1 ? "bg-(--g-color-fill-strong)" : ""}`}
          style={{ borderRadius: pixels("radius.sm"), padding: `${pixels("space.xs")} ${pixels("space.md")}` }}
        >
          {title}
        </p>
      ))}
    </div>
  );
}

function DemoPicture({ demo }: { demo?: BuiltinDemo }) {
  if (demo === "outline") return <Outline />;
  const picker = demo ? PICKERS[demo] : undefined;
  return picker ? <Picker query={picker.query} rows={picker.rows} /> : null;
}

/** What the window shows for a built-in feature with a picture of its own,
    until a click anywhere on the note puts it away. Remount it to show it again. */
export function DemoOverlay({ demo }: { demo?: BuiltinDemo }) {
  const [shown, setShown] = useState(true);
  const hasPicture = demo === "outline" || (demo !== undefined && PICKERS[demo] !== undefined);
  if (!shown || !hasPicture) return null;
  return (
    <div className="absolute inset-0 z-30" onPointerDown={() => setShown(false)}>
      <DemoPicture demo={demo} />
    </div>
  );
}

const NOTICE_MS = 5000;

/** Lines along the note's foot for a new shortcut or replacement, such as
    "⌘D Duplicate line", for a few seconds after they arrive. Remount it
    to show new ones. */
export function Notices({ notices }: { notices: ReactNode[] }) {
  const [shown, setShown] = useState(true);
  useEffect(() => {
    const timer = window.setTimeout(() => setShown(false), NOTICE_MS);
    return () => window.clearTimeout(timer);
  }, []);
  if (!shown || notices.length === 0) return null;
  return (
    <div aria-hidden className="pointer-events-none absolute inset-x-0 bottom-[6%] z-30 flex flex-col items-center gap-2">
      {notices.map((notice, index) => (
        <div
          key={index}
          className={`flex items-center gap-[0.6em] bg-(--g-color-tooltip) text-(--g-color-tooltip-text) ${APPEAR}`}
          style={{ ...POPOVER, padding: `${pixels("space.sm", 1.5)} ${pixels("space.lg")}` }}
        >
          {notice}
        </div>
      ))}
    </div>
  );
}
