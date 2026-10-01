"use client";

import { useState, type CSSProperties, type ReactNode } from "react";
import { fontSize, pixels, realPixels, showsInText, token } from "@/lib/mockConfig";
import { AppIcon } from "./AppIcon";
import { FloatingBar } from "./MockToolbar";
import { useMock, useSymbolShown, useToolbarsAt, type Nearness } from "./mockContext";

const COLOUR_EASE = "transition-colors duration-300 ease-out-soft";

const BODY: CSSProperties = {
  fontFamily: token("font.text"),
  fontSize: fontSize("font.scale.body"),
  lineHeight: token("font.line-height.body"),
  fontWeight: token("font.weight.regular"),
};

/** Markdown syntax, drawn in the dimmed markup colour when it shows. */
function Mark({ syntax, near, children }: { syntax: string; near?: Nearness; children: ReactNode }) {
  if (!useSymbolShown(syntax, near)) return null;
  return <span className={`text-(--g-color-text-faint) ${COLOUR_EASE}`}>{children}</span>;
}

function Strong({ children }: { children: ReactNode }) {
  return (
    <>
      <Mark syntax="strong">**</Mark>
      <strong style={{ fontWeight: token("font.weight.bold") }}>{children}</strong>
      <Mark syntax="strong">**</Mark>
    </>
  );
}

function WikiLink({ near, children }: { near?: Nearness; children: ReactNode }) {
  const preview = useMock().demo === "link-preview" && !near;
  return (
    <span className="relative">
      <Mark syntax="wikilink" near={near}>[[</Mark>
      <span
        className={`text-(--g-color-link) underline decoration-(--g-color-link-underline) underline-offset-[0.2em] ${COLOUR_EASE}`}
      >
        {children}
      </span>
      <Mark syntax="wikilink" near={near}>]]</Mark>
      {preview && <LinkPreview />}
    </span>
  );
}

function LinkPreview() {
  return (
    <span
      className="absolute top-full left-0 z-30 mt-1 block w-[16em] bg-(--g-color-popover) p-[0.8em] text-[0.85em] leading-snug text-(--g-color-text) shadow-lifted transition-[opacity,translate] duration-300 ease-out-soft starting:opacity-0 motion-safe:starting:translate-y-1"
      style={{ borderRadius: pixels("radius.lg"), boxShadow: `0 0 0 1px ${token("color.popover-ring")}, 0 ${realPixels(8)} ${realPixels(24)} ${token("color.popover-shadow")}` }}
    >
      <span className="block font-bold text-(--g-color-text-strong)">Food</span>
      <span className="mt-1 block text-(--g-color-text-muted)">Time Out Market for lunch. Cervejaria Ramiro for seafood, then pastéis de nata.</span>
    </span>
  );
}

function Highlight({ children }: { children: ReactNode }) {
  return (
    <>
      <Mark syntax="highlight">==</Mark>
      <mark className={`rounded-[0.15em] bg-(--g-color-highlight) px-[0.1em] text-inherit ${COLOUR_EASE}`}>{children}</mark>
      <Mark syntax="highlight">==</Mark>
    </>
  );
}

function InlineCode({ children }: { children: ReactNode }) {
  return (
    <>
      <Mark syntax="inline-code">`</Mark>
      <code
        className="bg-(--g-color-code-background)"
        style={{ fontFamily: token("font.code"), fontSize: `calc(1em * ${token("font.scale.code")})`, borderRadius: pixels("radius.sm"), paddingInline: pixels("space.xs", 1.5) }}
      >
        {children}
      </code>
      <Mark syntax="inline-code">`</Mark>
    </>
  );
}

/** A word the grammar checker flags, with its wavy underline. */
function Flagged({ kind, shown, children }: { kind: "spelling" | "mechanical"; shown: boolean; children: ReactNode }) {
  if (!shown) return children;
  const colour = kind === "spelling" ? "decoration-(--g-color-flag-spelling)" : "decoration-(--g-color-flag-mechanical)";
  return <span className={`underline decoration-wavy decoration-1 underline-offset-[0.25em] ${colour}`}>{children}</span>;
}

function useGrammar() {
  const { setting } = useMock();
  const enabled = setting("prose.grammar.enabled") === true;
  const variant = String(setting("prose.grammar.english"));
  return {
    mechanical: enabled,
    favourite: enabled && setting("prose.grammar.spelling") === true && variant === "american",
  };
}

function Quoted({ children }: { children: ReactNode }) {
  const curly = useMock().setting("editor.smart-quotes") === true;
  return (
    <>
      {curly ? "“" : '"'}
      {children}
      {curly ? "”" : '"'}
    </>
  );
}

const SENTENCE_TINTS = {
  short: "bg-(--g-color-sentence-short)",
  medium: "bg-(--g-color-sentence-medium)",
  long: "bg-(--g-color-sentence-long)",
};

function useSentenceTint(words: number): string {
  const { setting } = useMock();
  if (setting("prose.sentence-length.enabled") !== true) return "";
  if (words < Number(setting("prose.sentence-length.short-below"))) return SENTENCE_TINTS.short;
  if (words > Number(setting("prose.sentence-length.long-above"))) return SENTENCE_TINTS.long;
  return SENTENCE_TINTS.medium;
}

/** A sentence, tinted by its length when sentence-length highlighting is on. */
function Sentence({ words, children }: { words: number; children: ReactNode }) {
  const tint = useSentenceTint(words);
  return <span className={`box-decoration-clone ${tint} ${COLOUR_EASE}`}>{children} </span>;
}

function SelectionBars() {
  return useToolbarsAt("selection")
    .filter(showsInText)
    .map((toolbar) => (
      <FloatingBar key={toolbar.id} toolbar={toolbar} className="whitespace-nowrap" />
    ));
}

/** Selected text, drawn when a toolbar belongs with a selection, with the
    selection bar floating above it. */
function Selected({ children }: { children: ReactNode }) {
  const { toolbars, patch } = useMock();
  const needsSelection = toolbars.some((toolbar) => toolbar.place === "selection" || toolbar.behaviour === "with-selection");
  const recoloured = patch.theme?.["color.selection"] !== undefined;
  if (!needsSelection && !recoloured) return children;
  return (
    <span className={`relative bg-(--g-color-selection) ${COLOUR_EASE}`}>
      <span
        className="absolute bottom-full left-1/2 flex -translate-x-1/2 flex-col items-center"
        style={{ marginBottom: pixels("toolbar.float-gap"), gap: pixels("space.xs") }}
      >
        <SelectionBars />
      </span>
      {children}
    </span>
  );
}

function Intro() {
  const grammar = useGrammar();
  return (
    <p style={{ marginTop: pixels("space.xl") }}>
      <Sentence words={6}>
        Flights are <Strong>booked</Strong> for the 14th.
      </Sentence>
      <Sentence words={15}>
        Ana sent a list of her <Flagged kind="spelling" shown={grammar.favourite}>favourite</Flagged> places to eat, which I copied into{" "}
        <WikiLink>Food</WikiLink>.
      </Sentence>
      <Sentence words={4}>
        She wrote <Quoted>go early</Quoted>.
      </Sentence>
      <Sentence words={27}>
        <Highlight>
          Take the <Selected>tram up</Selected> to the castle
        </Highlight>{" "}
        before the queue, because <Flagged kind="mechanical" shown={grammar.mechanical}>the the</Flagged> light on the river is best before
        nine and the crowds come soon after.
      </Sentence>
    </p>
  );
}

function HeadingText({ level, children }: { level: 1 | 2; children: ReactNode }) {
  return (
    <>
      <Mark syntax="heading">{level === 1 ? "# " : "## "}</Mark>
      {children}
    </>
  );
}

type FoldableProps = { title: string; lines: number; folded: boolean; onToggle: () => void; children: ReactNode };

/** A heading and what's under it, folded as Gasp folds it: the chevron in
    the margin shows while the pointer is on the heading and stays once
    it's folded, and a folded heading shows how many lines it hides. */
function Foldable({ title, lines, folded, onToggle, children }: FoldableProps) {
  return (
    <section>
      <div
        className="group/heading relative text-(--g-color-text)"
        style={{
          fontSize: fontSize("font.scale.h2"),
          lineHeight: token("font.heading.line-height"),
          fontWeight: token("font.weight.bold"),
          marginTop: `calc(1em * (${token("font.heading.space-above")} + 0.5))`,
        }}
      >
        <button
          type="button"
          onClick={onToggle}
          aria-expanded={!folded}
          aria-label={`${folded ? "Unfold" : "Fold"} ${title}`}
          className={`absolute top-1/2 -left-[1.25em] grid size-[1em] -translate-y-1/2 cursor-pointer place-items-center rounded-md text-(--g-color-text-faint) transition-opacity duration-150 hover:bg-(--g-color-fill-strong) hover:text-(--g-color-text) focus-visible:opacity-100 ${folded ? "text-(--g-color-text-muted) opacity-100" : "opacity-0 group-hover/heading:opacity-100"}`}
        >
          <AppIcon
            name="caret-down"
            size="0.6em"
            className={`transition-transform duration-200 ease-out-soft ${folded ? "-rotate-90" : ""}`}
          />
        </button>
        <HeadingText level={2}>{title}</HeadingText>
        {folded && (
          <span
            className="ml-2 bg-(--g-color-fill) px-1.5 align-middle font-normal text-(--g-color-text-muted)"
            style={{ fontSize: fontSize("font.scale.small"), fontFamily: token("font.ui"), borderRadius: pixels("radius.sm") }}
          >
            {lines === 1 ? "1 line" : `${lines} lines`}
          </span>
        )}
      </div>
      {!folded && <div style={{ marginTop: pixels("space.sm") }}>{children}</div>}
    </section>
  );
}

const pulse = (on: boolean) => (on ? "ring-2 ring-(--g-color-accent) ring-offset-4 ring-offset-(--g-color-background) motion-safe:animate-pulse" : "");

function Callout() {
  return (
    <div
      className="relative overflow-hidden"
      style={{ borderRadius: pixels("radius.md"), padding: `${pixels("space.md")} ${pixels("space.lg")}`, marginTop: pixels("space.md") }}
    >
      <span
        aria-hidden
        className="absolute inset-0 bg-(--g-color-callout-tip)"
        style={{ opacity: token("opacity.callout") }}
      />
      <p className="relative flex items-center gap-[0.4em] text-(--g-color-callout-tip)">
        <Mark syntax="callout">&gt; [!tip] </Mark>
        <AppIcon name="lightbulb" size="1em" />
        Go early
      </p>
      <p className="relative">
        <Mark syntax="callout">&gt; </Mark>
        Pastéis de Belém opens at 8, and the line is short before 9.
      </p>
    </div>
  );
}

const TABLE_ROWS = [
  ["Thursday", "Alfama and the castle", "€0"],
  ["Friday", "Belém", "€15"],
  ["Saturday", "Sintra", "€32"],
];

function Cell({ header, children, end }: { header?: boolean; end?: boolean; children: ReactNode }) {
  const Tag = header ? "th" : "td";
  return (
    <Tag
      className={`${end ? "text-right" : "text-left"} font-[inherit]`}
      style={{
        padding: `${pixels("table.cell-padding-y")} ${pixels("table.cell-padding-x")}`,
        border: `${pixels("table.rule-width")} solid ${token("color.table.rule")}`,
        borderBottom: header ? `${pixels("table.header-rule-width")} solid ${token("color.table.header-rule")}` : undefined,
        fontWeight: header ? token("font.weight.bold") : undefined,
      }}
    >
      {children}
    </Tag>
  );
}

function Table() {
  const shown = useMock().demo === "table";
  return (
    <table className={`border-collapse ${pulse(shown)}`} style={{ marginTop: pixels("space.lg") }}>
      <thead className="bg-(--g-color-table-header)">
        <tr>
          <Cell header>Day</Cell>
          <Cell header>Where</Cell>
          <Cell header end>
            Cost
          </Cell>
        </tr>
      </thead>
      <tbody>
        {TABLE_ROWS.map(([day, where, cost]) => (
          <tr key={day}>
            <Cell>{day}</Cell>
            <Cell>{where}</Cell>
            <Cell end>{cost}</Cell>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** A task's box, as element.rs paints it: the accent filled, with the
    page's colour for the tick, or a faint outline. */
function Checkbox({ done }: { done: boolean }) {
  const style: CSSProperties = { width: "0.875em", height: "0.875em", borderRadius: pixels("radius.sm") };
  if (!done) {
    return <span className="inline-block shrink-0 translate-y-[0.1em]" style={{ ...style, boxShadow: `inset 0 0 0 ${realPixels(1.5)} ${token("color.text-faint")}` }} />;
  }
  return (
    <span className={`grid shrink-0 translate-y-[0.1em] place-items-center bg-(--g-color-accent) text-(--g-color-background) ${COLOUR_EASE}`} style={style}>
      <AppIcon name="check" size="0.7em" />
    </span>
  );
}

function TaskMarker({ done, near }: { done: boolean; near?: Nearness }) {
  const shown = useSymbolShown("list-marker", near);
  if (shown) return <span className="text-(--g-color-text-faint)">{done ? "- [x] " : "- [ ] "}</span>;
  return <Checkbox done={done} />;
}

function Caret() {
  return <span aria-hidden className={`ml-[0.04em] inline-block h-[1.1em] w-[2px] translate-y-[0.18em] bg-(--g-color-accent) animate-blink ${COLOUR_EASE}`} />;
}

function CursorLineBars() {
  return useToolbarsAt("cursor-line")
    .filter(showsInText)
    .map((toolbar) => <FloatingBar key={toolbar.id} toolbar={toolbar} className="ml-[0.5em] translate-y-[0.15em] align-middle" />);
}

function Tasks() {
  return (
    <ul className="grid" style={{ gap: pixels("space.xs") }}>
      <li className="flex items-baseline gap-[0.5em]">
        <TaskMarker done near="block" />
        <span className="text-(--g-color-text-muted) line-through">Book flights</span>
      </li>
      <li className="flex flex-wrap items-baseline gap-x-[0.5em]">
        <TaskMarker done={false} near="line" />
        <span>
          Pack light, and check <WikiLink near="element">Packing</WikiLink>
          <Caret />
          <CursorLineBars />
        </span>
      </li>
    </ul>
  );
}

function MathSource() {
  const coloured = useMock().setting("math.bracket-colours") === true;
  const bracket = (text: string) => <span style={coloured ? { color: token("color.math.bracket-1") } : undefined}>{text}</span>;
  return (
    <span style={{ fontFamily: token("font.code"), fontSize: `calc(1em * ${token("font.scale.code")})` }}>
      $2 \times {bracket("(")}150+45{bracket(")")} = 390$
    </span>
  );
}

/** Inline math as Gasp draws it, or its source when math symbols show. */
function InlineMath() {
  const source = useSymbolShown("math");
  const shown = useMock().demo === "math";
  if (source) return <MathSource />;
  return (
    <span className={`whitespace-nowrap rounded-sm ${pulse(shown)}`} style={{ fontFamily: token("font.text") }}>
      2 × (150 + 45) = 390
    </span>
  );
}

const CODE_LINES: [kind: string, text: string][][] = [
  [["comment", "// split the hotel"]],
  [["keyword", "const"], ["", " each = "], ["number", "390"], ["", " / "], ["number", "2"], ["", ";"]],
  [["", "console."], ["function", "log"], ["", "("], ["string", "`€${each}`"], ["", ");"]],
];

function CodeLine({ parts, number }: { parts: [string, string][]; number?: number }) {
  return (
    <div className="flex">
      {number !== undefined && <span className="w-[2em] shrink-0 pr-[0.8em] text-right text-(--g-color-text-faint)">{number}</span>}
      <span>
        {parts.map(([kind, text], index) => (
          <span key={index} style={kind ? { color: token(`color.code.${kind}`) } : undefined}>
            {text}
          </span>
        ))}
      </span>
    </div>
  );
}

function CodeBlock() {
  const numbered = useMock().setting("editor.code-line-numbers") === true;
  return (
    <div
      className="bg-(--g-color-code-background) whitespace-pre"
      style={{
        fontFamily: token("font.code"),
        fontSize: `calc(1em * ${token("font.scale.code")})`,
        lineHeight: token("font.line-height.code"),
        borderRadius: pixels("radius.md"),
        padding: pixels("space.lg"),
        marginTop: pixels("space.md"),
      }}
    >
      <Mark syntax="code-fence">
        <div>```js</div>
      </Mark>
      {CODE_LINES.map((parts, index) => (
        <CodeLine key={index} parts={parts} number={numbered ? index + 1 : undefined} />
      ))}
      <Mark syntax="code-fence">
        <div>```</div>
      </Mark>
    </div>
  );
}

function Quote() {
  return (
    <blockquote className="relative text-(--g-color-text-muted)" style={{ paddingLeft: pixels("space.xl"), marginTop: pixels("space.md") }}>
      <span aria-hidden className="absolute inset-y-0 left-0 bg-(--g-color-text-faint)" style={{ width: realPixels(3) }} />
      <Mark syntax="blockquote">&gt; </Mark>
      Lisbon is a city of seven hills, so pack shoes that grip.
    </blockquote>
  );
}

type SectionName = "plans" | "to-do" | "notes";

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

function InlineTitle() {
  const shown = useMock().setting("editor.show-inline-title") === true;
  if (!shown) return null;
  return (
    <p
      className={`text-(--g-color-text) ${COLOUR_EASE}`}
      style={{ fontSize: fontSize("font.scale.title"), lineHeight: token("font.heading.line-height"), fontWeight: token("font.weight.bold") }}
    >
      Trip to Lisbon
    </p>
  );
}

/** The note, remounted with each new answer so a request to fold headings
    folds the first section the moment it arrives. */
export function MockNote() {
  const { demo } = useMock();
  const { isFolded, toggle } = useFolds(demo === "fold");
  return (
    <div className={`text-(--g-color-text) ${COLOUR_EASE}`} style={BODY}>
      <InlineTitle />
      <Intro />
      <Foldable title="Plans" lines={9} folded={isFolded("plans")} onToggle={() => toggle("plans")}>
        <Callout />
        <Table />
      </Foldable>
      <Foldable title="To do" lines={2} folded={isFolded("to-do")} onToggle={() => toggle("to-do")}>
        <Tasks />
      </Foldable>
      <Foldable title="Notes" lines={8} folded={isFolded("notes")} onToggle={() => toggle("notes")}>
        <Quote />
        <p style={{ marginTop: pixels("space.md") }}>
          Take <InlineCode>tram 28</InlineCode> both ways. Two nights at the hotel come to <InlineMath /> euros.
        </p>
        <CodeBlock />
      </Foldable>
    </div>
  );
}
