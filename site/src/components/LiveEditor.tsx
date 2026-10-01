"use client";

import { Check } from "@phosphor-icons/react";
import { useLayoutEffect, useRef, useState, type KeyboardEvent, type ReactNode } from "react";
import { sourceOffsetAt } from "@/lib/caretFromPoint";
import { parseBlock, parseInline, toggleTask, type Block, type BlockKind, type Inline } from "@/lib/markdown";

const START = [
  "# A note to play with",
  "Click any line to see the **Markdown** behind it.",
  "The marks fade once your cursor leaves, so a note reads ==clean== while it stays plain text.",
  "## Things to try",
  "- [ ] Put `**` around a word",
  "- [x] Tick a task",
  "- Link to another note, like [[Ideas]]",
  "> Up and down move between lines, and Enter starts a new one.",
];

const BLOCK_STYLES: Record<BlockKind, string> = {
  h1: "text-3xl font-bold leading-tight mb-2",
  h2: "text-xl font-bold leading-snug mt-4",
  task: "",
  bullet: "",
  quote: "text-ink-soft",
  paragraph: "",
};

const INLINE_STYLES: Record<Inline["kind"], string> = {
  text: "",
  strong: "font-bold",
  emphasis: "italic",
  highlight: "rounded-sm bg-highlight px-0.5 text-ink",
  code: "code rounded bg-fill px-1 py-0.5",
  link: "underline decoration-ink-muted underline-offset-4",
};

/** The body formatted, each run marked with where its text starts in the
    source line (`from` is the length of the block's marker). */
function Formatted({ text, from }: { text: string; from: number }) {
  let consumed = from;
  return parseInline(text).map((inline, index) => {
    const source = consumed + inline.open.length;
    consumed += inline.open.length + inline.text.length + inline.close.length;
    return (
      <span key={index} data-source={source} className={INLINE_STYLES[inline.kind]}>
        {inline.text}
      </span>
    );
  });
}

function RawMarks({ text }: { text: string }) {
  return parseInline(text).map((inline, index) => (
    <span key={index}>
      <span className="text-ink-muted">{inline.open}</span>
      {inline.text}
      <span className="text-ink-muted">{inline.close}</span>
    </span>
  ));
}

function Gutter({ block, onToggle }: { block: Block; onToggle: () => void }) {
  if (block.kind === "task") {
    return (
      <button
        type="button"
        role="checkbox"
        aria-checked={block.done}
        aria-label={block.done ? "Done" : "Not done"}
        onClick={onToggle}
        className={`mt-[0.3em] mr-3 grid size-[1.05em] shrink-0 cursor-pointer place-items-center rounded-[0.3em] ${block.done ? "bg-ink text-paper" : "ring-[1.5px] ring-ink-muted ring-inset"}`}
      >
        {block.done && <Check size={12} weight="bold" aria-hidden />}
      </button>
    );
  }
  if (block.kind === "bullet") {
    return <span aria-hidden className="mt-[0.65em] mr-3 ml-1.5 size-1.5 shrink-0 rounded-full bg-ink" />;
  }
  if (block.kind === "quote") {
    return <span aria-hidden className="mr-4 w-[3px] shrink-0 self-stretch bg-sea" />;
  }
  return null;
}

type EditingLineProps = {
  line: string;
  caret: number;
  onChange: (line: string) => void;
  onKeyDown: (event: KeyboardEvent<HTMLTextAreaElement>) => void;
  onBlur: () => void;
};

/** The line under the cursor: its plain text in a field laid over a copy
    whose marks are tinted, so the Markdown shows while it's edited. */
function EditingLine({ line, caret, onChange, onKeyDown, onBlur }: EditingLineProps) {
  const field = useRef<HTMLTextAreaElement>(null);
  const block = parseBlock(line);
  useLayoutEffect(() => {
    field.current?.focus();
    field.current?.setSelectionRange(caret, caret);
  }, [caret]);
  return (
    <div className="relative">
      <div aria-hidden className="whitespace-pre-wrap break-words">
        <span className="text-ink-muted">{block.marker}</span>
        <RawMarks text={block.body} />
        {"​"}
      </div>
      <textarea
        ref={field}
        value={line}
        rows={1}
        spellCheck={false}
        aria-label="Line of the note"
        onChange={(event) => onChange(event.target.value.replace(/\n/g, ""))}
        onKeyDown={onKeyDown}
        onBlur={onBlur}
        className="absolute inset-0 resize-none overflow-hidden bg-transparent p-0 text-transparent caret-caret outline-none selection:bg-highlight"
        style={{ font: "inherit", letterSpacing: "inherit" }}
      />
    </div>
  );
}

type LineProps = { line: string; children?: ReactNode; onToggle: () => void; onActivate: (at: number) => void };

function Line({ line, children, onToggle, onActivate }: LineProps) {
  const block = parseBlock(line);
  const editing = children !== undefined;
  return (
    <div className={`flex min-h-[1.65em] ${BLOCK_STYLES[block.kind]}`}>
      {!editing && <Gutter block={block} onToggle={onToggle} />}
      {editing ? (
        <div className="min-w-0 flex-1">{children}</div>
      ) : (
        <button
          type="button"
          onClick={(event) => onActivate(sourceOffsetAt(event.currentTarget, event.clientX, event.clientY) ?? line.length)}
          className={`min-w-0 flex-1 cursor-text text-left ${block.done ? "text-ink-muted line-through" : ""}`}
        >
          <Formatted text={block.body} from={block.marker.length} />
          {block.body === "" && "​"}
        </button>
      )}
    </div>
  );
}

type Cursor = { line: number; at: number } | null;

function continuedMarker(line: string): string {
  const block = parseBlock(line);
  if (block.kind === "task") return "- [ ] ";
  if (block.kind === "bullet") return "- ";
  return "";
}

function useNote() {
  const [lines, setLines] = useState(START);
  const [cursor, setCursor] = useState<Cursor>(null);

  const setLine = (index: number, text: string) =>
    setLines((all) => all.map((line, at) => (at === index ? text : line)));

  const splitAt = (index: number, at: number) => {
    const line = lines[index];
    const block = parseBlock(line);
    if (block.marker && block.body === "") {
      setLine(index, "");
      setCursor({ line: index, at: 0 });
      return;
    }
    const marker = continuedMarker(line);
    const next = marker + line.slice(at);
    setLines((all) => [...all.slice(0, index), line.slice(0, at), next, ...all.slice(index + 1)]);
    setCursor({ line: index + 1, at: marker.length });
  };

  const joinWithPrevious = (index: number): boolean => {
    if (index < 1) return false;
    const previous = lines[index - 1];
    setLines((all) => [...all.slice(0, index - 1), previous + all[index], ...all.slice(index + 1)]);
    setCursor({ line: index - 1, at: previous.length });
    return true;
  };

  const moveTo = (index: number, column: number): boolean => {
    if (index < 0 || index >= lines.length) return false;
    setCursor({ line: index, at: Math.min(column, lines[index].length) });
    return true;
  };

  return { lines, cursor, setCursor, setLine, splitAt, joinWithPrevious, moveTo };
}

function wordCount(lines: string[]): number {
  return lines.join(" ").split(/\s+/).filter((word) => /\w/.test(word)).length;
}

/** A small live-preview editor: every line shows formatted except the one
    being edited, which shows its Markdown, as in the app. */
export function LiveEditor() {
  const note = useNote();
  const { lines, cursor } = note;

  const onKeyDown = (event: KeyboardEvent<HTMLTextAreaElement>) => {
    if (!cursor) return;
    const { selectionStart, selectionEnd } = event.currentTarget;
    const collapsed = selectionStart === selectionEnd;
    const { line } = cursor;
    const keys: Record<string, () => boolean> = {
      Enter: () => {
        note.splitAt(line, selectionStart);
        return true;
      },
      ArrowUp: () => collapsed && note.moveTo(line - 1, selectionStart),
      ArrowDown: () => collapsed && note.moveTo(line + 1, selectionStart),
      Backspace: () => collapsed && selectionStart === 0 && note.joinWithPrevious(line),
      Escape: () => {
        note.setCursor(null);
        return true;
      },
    };
    if (keys[event.key]?.()) event.preventDefault();
  };

  return (
    <div className="rounded-2xl bg-surface shadow-lifted">
      <div className="body px-5 pt-7 pb-4 sm:px-10 sm:pt-9">
        {lines.map((line, index) => (
          <Line
            key={index}
            line={line}
            onToggle={() => note.setLine(index, toggleTask(line))}
            onActivate={(at) => note.moveTo(index, at)}
          >
            {cursor?.line === index ? (
              <EditingLine
                line={line}
                caret={cursor.at}
                onChange={(text) => note.setLine(index, text)}
                onKeyDown={onKeyDown}
                onBlur={() => note.setCursor((now) => (now?.line === index ? null : now))}
              />
            ) : undefined}
          </Line>
        ))}
      </div>
      <p className="small figure px-5 pb-4 text-right text-ink-muted sm:px-10">{wordCount(lines)} words</p>
    </div>
  );
}
