"use client";

import {
  CaretDown,
  CheckSquare,
  CloudCheck,
  Code,
  Highlighter,
  LinkSimple,
  ListBullets,
  ListNumbers,
  TextB,
  TextItalic,
  TextStrikethrough,
  TextUnderline,
  type Icon,
} from "@phosphor-icons/react";
import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import {
  ACCENT_COLOURS,
  FONTS,
  FONT_SIZE,
  HIGHLIGHT_COLOURS,
  STATUS_WIDGETS,
  colourPair,
  type ColourPair,
  type LookChanges,
  type StatusWidget,
  type Toolbar,
  type ToolbarItem,
} from "@/lib/lookChanges";

type Palette = {
  window: string;
  page: string;
  text: string;
  strong: string;
  muted: string;
  fill: string;
  rule: string;
  accent: string;
  "on-accent": string;
  link: string;
  underline: string;
  highlight: string;
  popover: string;
};

/** Gasp's own light and dark tokens, from `crates/config/defaults/theme.toml`. */
const GASP_PALETTES: Record<"light" | "dark", Palette> = {
  light: {
    window: "#f6f6f7",
    page: "#ffffff",
    text: "#27272a",
    strong: "#18181b",
    muted: "#71717a",
    fill: "rgba(0, 0, 0, 0.05)",
    rule: "#e4e4e7",
    accent: "#000000",
    "on-accent": "#ffffff",
    link: "#000000",
    underline: "rgba(0, 0, 0, 0.3)",
    highlight: "#fff59d",
    popover: "#ffffff",
  },
  dark: {
    window: "#151412",
    page: "#1c1b19",
    text: "#e2ded7",
    strong: "#f7f5f1",
    muted: "#958f86",
    fill: "rgba(255, 255, 255, 0.055)",
    rule: "#312f2c",
    accent: "#ebe7e0",
    "on-accent": "#1c1b19",
    link: "#ebe7e0",
    underline: "rgba(235, 231, 224, 0.4)",
    highlight: "#4f4418",
    popover: "#262522",
  },
};

/** Each token resolves to its light or dark value by the visitor's system,
    the way Gasp's "match-system" appearance does. */
const TOKEN_VARIABLES =
  "[--mock-window:var(--light-window)] dark:[--mock-window:var(--dark-window)] " +
  "[--mock-page:var(--light-page)] dark:[--mock-page:var(--dark-page)] " +
  "[--mock-text:var(--light-text)] dark:[--mock-text:var(--dark-text)] " +
  "[--mock-strong:var(--light-strong)] dark:[--mock-strong:var(--dark-strong)] " +
  "[--mock-muted:var(--light-muted)] dark:[--mock-muted:var(--dark-muted)] " +
  "[--mock-fill:var(--light-fill)] dark:[--mock-fill:var(--dark-fill)] " +
  "[--mock-rule:var(--light-rule)] dark:[--mock-rule:var(--dark-rule)] " +
  "[--mock-accent:var(--light-accent)] dark:[--mock-accent:var(--dark-accent)] " +
  "[--mock-on-accent:var(--light-on-accent)] dark:[--mock-on-accent:var(--dark-on-accent)] " +
  "[--mock-link:var(--light-link)] dark:[--mock-link:var(--dark-link)] " +
  "[--mock-underline:var(--light-underline)] dark:[--mock-underline:var(--dark-underline)] " +
  "[--mock-highlight:var(--light-highlight)] dark:[--mock-highlight:var(--dark-highlight)] " +
  "[--mock-popover:var(--light-popover)] dark:[--mock-popover:var(--dark-popover)]";

const BODY_PIXELS = 17;
const COLOUR_EASE = "transition-colors duration-300 ease-out-soft";

function withColours(base: Palette, look: LookChanges, mode: keyof ColourPair): Palette {
  const pick = (palette: Record<string, ColourPair>, colour: string | undefined) =>
    colour ? colourPair(palette, colour)[mode] : undefined;
  const accent = pick(ACCENT_COLOURS, look.accent) ?? base.accent;
  return {
    ...base,
    accent,
    link: pick(ACCENT_COLOURS, look.link) ?? accent,
    highlight: pick(HIGHLIGHT_COLOURS, look.highlight) ?? base.highlight,
  };
}

/** Which of Gasp's palettes the light and dark slots hold: both the same
    when the look forces one appearance. */
function paletteModes(look: LookChanges): { light: "light" | "dark"; dark: "light" | "dark" } {
  if (look.appearance === "light") return { light: "light", dark: "light" };
  if (look.appearance === "dark") return { light: "dark", dark: "dark" };
  return { light: "light", dark: "dark" };
}

function paletteStyle(look: LookChanges): CSSProperties {
  const modes = paletteModes(look);
  const style: Record<string, string> = {};
  for (const slot of ["light", "dark"] as const) {
    const mode = modes[slot];
    const palette = withColours(GASP_PALETTES[mode], look, mode);
    for (const [token, value] of Object.entries(palette)) style[`--${slot}-${token}`] = value;
  }
  return style;
}

function noteStyle(look: LookChanges): CSSProperties {
  const scale = (look.fontSize ?? FONT_SIZE.default) / FONT_SIZE.default;
  return {
    fontFamily: look.font ? FONTS[look.font] : undefined,
    fontSize: `${BODY_PIXELS * scale}px`,
  };
}

const TOOLBAR_ICONS: Record<Exclude<ToolbarItem, "separator">, Icon> = {
  "format.bold": TextB,
  "format.italic": TextItalic,
  "format.underline": TextUnderline,
  "format.strikethrough": TextStrikethrough,
  "format.highlight": Highlighter,
  "format.code": Code,
  "format.link": LinkSimple,
  "format.bullet-list": ListBullets,
  "format.numbered-list": ListNumbers,
  "edit.toggle-task": CheckSquare,
};

const APPEAR = "transition-[opacity,translate] duration-300 ease-out-soft starting:opacity-0 motion-safe:starting:translate-y-1";

function ToolbarButtons({ items }: { items: ToolbarItem[] }) {
  return items.map((item, index) => {
    if (item === "separator") return <span key={`separator-${index}`} className="mx-1 h-5 w-px bg-(--mock-rule)" />;
    const Icon = TOOLBAR_ICONS[item];
    return (
      <span key={item} className="grid size-8 place-items-center rounded-md text-(--mock-text)">
        <Icon size={17} aria-hidden />
      </span>
    );
  });
}

const BAR_POSITIONS = {
  overlay: {
    "editor-top": "top-2.5 left-1/2 -translate-x-1/2 rounded-full",
    "editor-bottom": "bottom-2.5 left-1/2 -translate-x-1/2 rounded-full",
  },
  strip: {
    "editor-top": "inset-x-0 top-0 shadow-[0_1px_0_var(--mock-rule)]",
    "editor-bottom": "inset-x-0 bottom-0 shadow-[0_-1px_0_var(--mock-rule)]",
  },
};

const BAR_SURFACES = {
  overlay: "gap-0.5 bg-(--mock-popover) p-1 shadow-lifted",
  strip: "h-11 gap-1 bg-(--mock-window) px-4 sm:px-8",
};

/** The formatting bar, in the band the note keeps free for it, so the
    text never moves when it appears. */
function FormattingBar({ toolbar }: { toolbar: Toolbar }) {
  const position = BAR_POSITIONS[toolbar.surface][toolbar.place];
  return (
    <div
      key={`${toolbar.place}-${toolbar.surface}`}
      aria-hidden
      className={`absolute z-10 flex items-center ${position} ${BAR_SURFACES[toolbar.surface]} ${COLOUR_EASE} ${APPEAR}`}
    >
      <ToolbarButtons items={toolbar.items} />
    </div>
  );
}

const WIDGET_TEXT: Record<Exclude<StatusWidget, "sync">, string> = {
  "word-count": "64 words",
  "character-count": "351 characters",
  "reading-time": "1 min read",
  "edit-time": "4 min editing",
  "cursor-position": "5:38",
};

function Widget({ widget }: { widget: StatusWidget }) {
  if (widget === "sync") return <CloudCheck size={15} aria-hidden className={APPEAR} />;
  return <span className={APPEAR}>{WIDGET_TEXT[widget]}</span>;
}

function StatusBar({ look }: { look: LookChanges }) {
  const widgets = look.statusWidgets ?? STATUS_WIDGETS;
  const hidden = look.statusBar === "hidden";
  return (
    <div
      aria-hidden
      className={`small figure flex h-9 items-center justify-end gap-4 px-5 text-(--mock-muted) transition-opacity duration-300 ${hidden ? "opacity-0" : ""}`}
    >
      {widgets.map((widget) => (
        <Widget key={widget} widget={widget} />
      ))}
    </div>
  );
}

function Link({ children }: { children: ReactNode }) {
  return (
    <span className={`text-(--mock-link) underline decoration-(--mock-underline) underline-offset-4 ${COLOUR_EASE}`}>
      {children}
    </span>
  );
}

function Task({ done, children }: { done: boolean; children: ReactNode }) {
  return (
    <li className="flex items-baseline gap-[0.5em]">
      <span
        className={`grid size-[0.85em] shrink-0 translate-y-[0.1em] place-items-center rounded-[0.2em] ${COLOUR_EASE} ${done ? "bg-(--mock-accent) text-(--mock-on-accent)" : "shadow-[inset_0_0_0_1.5px_var(--mock-muted)]"}`}
      >
        {done && (
          <svg viewBox="0 0 12 12" className="size-[0.65em]" aria-hidden>
            <path d="M2.5 6.2 5 8.6l4.5-5" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </span>
      <span className={done ? "text-(--mock-muted) line-through decoration-(--mock-muted)" : ""}>{children}</span>
    </li>
  );
}

function AccentCaret() {
  return (
    <span
      aria-hidden
      className={`ml-[0.08em] inline-block h-[1.05em] w-[2px] translate-y-[0.16em] bg-(--mock-accent) animate-blink ${COLOUR_EASE}`}
    />
  );
}

type FoldableSectionProps = { title: string; lines: number; folded: boolean; onToggle: () => void; children: ReactNode };

/** A heading and what's under it, folded as Gasp folds it: the chevron in
    the margin shows while the pointer is on the heading and stays once
    it's folded, and a folded heading shows how many lines it hides. */
function FoldableSection({ title, lines, folded, onToggle, children }: FoldableSectionProps) {
  return (
    <section className="mt-[0.8em]">
      <div className="group/heading relative">
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={!folded}
          aria-label={`${folded ? "Unfold" : "Fold"} ${title}`}
          className={`absolute top-1/2 -left-[1.5em] grid size-[1.3em] -translate-y-1/2 cursor-pointer place-items-center rounded-md text-(--mock-muted) transition-opacity duration-150 focus-visible:opacity-100 ${folded ? "opacity-100" : "opacity-0 group-hover/heading:opacity-100"}`}
        >
          <CaretDown aria-hidden className={`size-[0.8em] transition-transform duration-200 ease-out-soft ${folded ? "-rotate-90" : ""}`} />
        </button>
        <p className={`text-[1.12em] font-bold text-(--mock-strong) ${COLOUR_EASE}`}>
          {title}
          {folded && (
            <span className="small ml-2 rounded-md bg-(--mock-rule) px-1.5 py-0.5 align-middle font-normal text-(--mock-muted)">
              {lines === 1 ? "1 line" : `${lines} lines`}
            </span>
          )}
        </p>
      </div>
      {!folded && <div className="mt-[0.3em]">{children}</div>}
    </section>
  );
}

type SectionName = "plans" | "to-do";

function useFolds(foldOnArrival: boolean) {
  const [folded, setFolded] = useState<Set<SectionName>>(() => new Set(foldOnArrival ? ["plans"] : []));
  const toggle = (name: SectionName) =>
    setFolded((current) => {
      const next = new Set(current);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  return { isFolded: (name: SectionName) => folded.has(name), toggle };
}

/** Remounted with each new look, so a request to fold headings folds the
    first section the moment it arrives. */
function Note({ look }: { look: LookChanges }) {
  const { isFolded, toggle } = useFolds(look.foldHeadings === true);
  return (
    <div style={noteStyle(look)} className="leading-[1.6] transition-[font-size] duration-300 ease-out-soft">
      <p className={`text-[1.45em] leading-tight font-bold text-(--mock-strong) ${COLOUR_EASE}`}>Trip to Lisbon</p>
      <FoldableSection title="Plans" lines={2} folded={isFolded("plans")} onToggle={() => toggle("plans")}>
        <p>
          Flights are booked for the 14th. Ana sent a list of places to eat, which I copied into <Link>Food</Link>.{" "}
          <mark className={`rounded-[0.15em] bg-(--mock-highlight) px-[0.1em] text-inherit ${COLOUR_EASE}`}>
            Take the tram up to the castle early
          </mark>
          , before the queue.
        </p>
      </FoldableSection>
      <FoldableSection title="To do" lines={2} folded={isFolded("to-do")} onToggle={() => toggle("to-do")}>
        <ul className="grid gap-[0.25em]">
          <Task done>Book flights</Task>
          <Task done={false}>
            Pack light, and check <Link>Packing</Link>
            <AccentCaret />
          </Task>
        </ul>
      </FoldableSection>
    </div>
  );
}

/** Fades the note back in each time the look changes, so the change reads
    as one event even when it's only a colour. */
function usePulseOn(version: number) {
  const target = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = target.current;
    if (!element || version === 0) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    element.animate([{ opacity: 0.35, filter: "blur(3px)" }, { opacity: 1, filter: "blur(0)" }], {
      duration: 360,
      easing: "cubic-bezier(0.22, 1, 0.36, 1)",
    });
  }, [version]);
  return target;
}

type MockWindowProps = { look: LookChanges; version: number; corner?: ReactNode };

/** A small Gasp window drawn from the same keys the app reads. Its size
    never changes: bands for toolbars and the status bar are always kept. */
export function GaspMockWindow({ look, version, corner }: MockWindowProps) {
  const note = usePulseOn(version);
  return (
    <div
      style={paletteStyle(look)}
      className={`${TOKEN_VARIABLES} overflow-hidden rounded-2xl bg-(--mock-window) text-(--mock-text) shadow-lifted ${COLOUR_EASE}`}
    >
      <div className="flex h-10 items-center gap-2 px-4">
        {["bg-[#ff5f57]", "bg-[#febc2e]", "bg-[#28c840]"].map((light) => (
          <span key={light} aria-hidden className={`size-3 rounded-full ${light}`} />
        ))}
        <span className={`small ml-3 text-(--mock-muted) ${COLOUR_EASE}`}>Trip to Lisbon</span>
        <span className="small figure ml-auto text-(--mock-muted)">{corner}</span>
      </div>
      <div className={`relative h-72 overflow-hidden bg-(--mock-page) ${COLOUR_EASE}`}>
        {look.toolbar && <FormattingBar toolbar={look.toolbar} />}
        <div
          ref={note}
          className="h-full overflow-hidden px-6 py-14 [mask-image:linear-gradient(to_bottom,black_80%,transparent_94%)] sm:px-10"
        >
          <Note key={version} look={look} />
        </div>
      </div>
      <StatusBar look={look} />
    </div>
  );
}
