"use client";

import { ArrowCounterClockwise, CaretRight, CircleNotch, Lightning } from "@phosphor-icons/react";
import { useRef, useState, type FormEvent, type ReactNode } from "react";
import { builtinDemo, type BuiltinDemo } from "@/lib/builtins";
import type { ChangeAnswer, ChangeErrorCode } from "@/lib/changeHandler";
import { SUGGESTIONS, suggestionFor } from "@/lib/changeSuggestions";
import { settingsFiles } from "@/lib/configFiles";
import { hasChanges, mergePatches, type ConfigPatch } from "@/lib/configPatch";
import { describeChanges } from "@/lib/describeChanges";
import { COMMANDS } from "@/lib/gaspSchema";
import { GaspMockWindow } from "./GaspMockWindow";
import { Keycap } from "./mock/MockOverlays";

const MAX_REQUEST_LENGTH = 200;

type Outcome =
  | { kind: "idle" }
  | { kind: "asking" }
  | { kind: "answered"; seconds: number; reply?: string }
  | { kind: "example"; reply?: string }
  | { kind: "resting" };

type LiveResult = { answer: ChangeAnswer } | { error: ChangeErrorCode | "offline" };

async function askLive(request: string): Promise<LiveResult> {
  try {
    const response = await fetch("/api/change", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ request }),
    });
    const body = await response.json();
    return response.ok ? { answer: body as ChangeAnswer } : { error: body.error ?? "provider-failed" };
  } catch {
    return { error: "offline" };
  }
}

function Corner({ outcome }: { outcome: Outcome }) {
  if (outcome.kind === "answered") {
    return (
      <span className="flex items-center gap-1">
        <Lightning size={13} weight="fill" aria-hidden />
        {outcome.seconds.toFixed(2)} s
      </span>
    );
  }
  if (outcome.kind === "example") return "Saved example";
  return null;
}

const RESTING = "The live demo is resting right now, so try one of the examples.";

function statusText(outcome: Outcome): string | undefined {
  if (outcome.kind === "resting") return RESTING;
  if (outcome.kind === "answered" || outcome.kind === "example") return outcome.reply;
  return undefined;
}

function StatusLine({ outcome }: { outcome: Outcome }) {
  return <p className="small mt-4 min-h-[3em] text-ink-muted">{statusText(outcome)}</p>;
}

function RequestForm({
  value,
  asking,
  onChange,
  onSubmit,
}: {
  value: string;
  asking: boolean;
  onChange: (value: string) => void;
  onSubmit: (request: string) => void;
}) {
  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSubmit(value);
  };
  return (
    <form onSubmit={submit} aria-busy={asking} className="flex gap-2">
      <label htmlFor="change-request" className="sr-only">
        Tell Gasp what to change
      </label>
      <input
        id="change-request"
        value={value}
        maxLength={MAX_REQUEST_LENGTH}
        autoComplete="off"
        placeholder="Make my links blue"
        onChange={(event) => onChange(event.target.value)}
        className="body h-12 min-w-0 flex-1 rounded-xl border border-rule bg-surface px-4 text-ink placeholder:text-ink-muted focus-visible:border-ink focus-visible:ring-1 focus-visible:ring-ink focus-visible:outline-none"
      />
      <button
        type="submit"
        disabled={asking}
        className="relative h-12 shrink-0 cursor-pointer rounded-xl bg-button px-4 font-bold text-on-button shadow-lifted transition duration-150 ease-out-soft hover:opacity-90 active:scale-[0.98] disabled:cursor-wait"
      >
        <span className={asking ? "invisible" : ""}>Change it</span>
        {asking && (
          <span className="absolute inset-0 grid place-items-center">
            <CircleNotch size={20} weight="bold" className="animate-spin" aria-label="Asking" />
          </span>
        )}
      </button>
    </form>
  );
}

function Suggestions({ disabled, onPick }: { disabled: boolean; onPick: (request: string) => void }) {
  return (
    <ul className="mt-4 flex flex-wrap gap-2">
      {SUGGESTIONS.map(({ request }) => (
        <li key={request}>
          <button
            type="button"
            disabled={disabled}
            onClick={() => onPick(request)}
            className="small cursor-pointer rounded-full bg-fill px-3.5 py-1.5 text-left text-ink transition-[background-color,scale] duration-150 hover:bg-sea active:scale-[0.97] disabled:cursor-wait"
          >
            {request}
          </button>
        </li>
      ))}
    </ul>
  );
}

function WrittenSettings({ patch, onReset }: { patch: ConfigPatch; onReset: () => void }) {
  const changed = hasChanges(patch);
  return (
    <div className={`mt-4 transition-opacity duration-200 ${changed ? "" : "invisible opacity-0"}`}>
      <div className="flex items-start justify-between gap-4">
        <details className="group min-w-0 flex-1">
          <summary className="small flex w-fit cursor-pointer list-none items-center gap-1.5 rounded-md text-ink-muted hover:text-ink [&::-webkit-details-marker]:hidden">
            <CaretRight size={14} weight="bold" aria-hidden className="transition-transform duration-150 group-open:rotate-90" />
            Show the settings it wrote
          </summary>
          <div className="mt-3 grid gap-4 rounded-2xl bg-fill p-5">
            {settingsFiles(patch).map(({ file, lines }) => (
              <div key={file}>
                <p className="small code text-ink-muted">.gasp/{file}</p>
                <pre className="code mt-1.5 overflow-x-auto leading-relaxed text-ink">{lines}</pre>
              </div>
            ))}
          </div>
        </details>
        <button
          type="button"
          onClick={onReset}
          className="small flex shrink-0 cursor-pointer items-center gap-1.5 rounded-md text-ink-muted hover:text-ink"
        >
          <ArrowCounterClockwise size={14} weight="bold" aria-hidden />
          Reset the window
        </button>
      </div>
    </div>
  );
}

/** The notices an answer brings: each new shortcut and replacement. */
function noticesFor(patch: ConfigPatch): ReactNode[] {
  const keys = (patch.keys ?? []).map(({ keys: chord, command }) => (
    <>
      <Keycap chord={chord} />
      {COMMANDS.get(command)?.title ?? command}
    </>
  ));
  const replacements = (patch.replacements ?? []).map(({ from, to }) => `Typing ${from} now gives ${to}`);
  return [...keys, ...replacements];
}

type Look = { patch: ConfigPatch; demo?: BuiltinDemo; notices: ReactNode[] };

const DEFAULT_LOOK: Look = { patch: {}, notices: [] };

/** Asks for changes in plain words. Each answer applies on top of the
    last, and the window follows it the moment it arrives. */
function useLook() {
  const [look, setLook] = useState<Look>(DEFAULT_LOOK);
  const [version, setVersion] = useState(0);
  const [announcement, setAnnouncement] = useState("");
  const apply = (answer: ChangeAnswer) => {
    const demo = answer.builtin ? builtinDemo(answer.builtin) : undefined;
    if (!hasChanges(answer.patch) && !demo) return;
    setLook((current) => ({ patch: mergePatches(current.patch, answer.patch), demo, notices: noticesFor(answer.patch) }));
    setVersion((current) => current + 1);
    if (hasChanges(answer.patch)) setAnnouncement(`Changed the window: ${describeChanges(answer.patch)}.`);
  };
  const reset = () => {
    setLook(DEFAULT_LOOK);
    setVersion((current) => current + 1);
    setAnnouncement("Reset the window to Gasp's default look.");
  };
  return { look, version, announcement, setAnnouncement, apply, reset };
}

export function AskToChange() {
  const { look, version, announcement, setAnnouncement, apply, reset } = useLook();
  const [request, setRequest] = useState("");
  const [outcome, setOutcome] = useState<Outcome>({ kind: "idle" });
  const liveUnavailable = useRef(false);

  const showExampleOrRest = (wanted: string) => {
    const example = suggestionFor(wanted);
    if (example) {
      apply(example.answer);
      return setOutcome({ kind: "example", reply: example.answer.reply });
    }
    setOutcome({ kind: "resting" });
    setAnnouncement(RESTING);
  };

  const ask = async (wanted: string) => {
    if (!wanted.trim() || outcome.kind === "asking") return;
    if (liveUnavailable.current) return showExampleOrRest(wanted);
    setOutcome({ kind: "asking" });
    const started = performance.now();
    const result = await askLive(wanted.trim());
    if ("error" in result) {
      if (result.error === "no-key") liveUnavailable.current = true;
      return showExampleOrRest(wanted);
    }
    apply(result.answer);
    setOutcome({ kind: "answered", seconds: (performance.now() - started) / 1000, reply: result.answer.reply });
    if (result.answer.reply) setAnnouncement(result.answer.reply);
  };

  const pick = (suggestion: string) => {
    setRequest(suggestion);
    void ask(suggestion);
  };

  const asking = outcome.kind === "asking";
  return (
    <div className="flex flex-col gap-6 lg:grid lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)] lg:items-start lg:gap-12">
      <div className="order-2 lg:order-none">
        <RequestForm value={request} asking={asking} onChange={setRequest} onSubmit={(wanted) => void ask(wanted)} />
        <p className="small mt-2 text-ink-muted">
          We keep what you type here, with nothing that identifies you, to see what people want to change.
        </p>
        <Suggestions disabled={asking} onPick={pick} />
        <StatusLine outcome={outcome} />
      </div>
      <div className="order-1 lg:order-none">
        <GaspMockWindow patch={look.patch} demo={look.demo} notices={look.notices} version={version} corner={<Corner outcome={outcome} />} />
        <WrittenSettings patch={look.patch} onReset={reset} />
      </div>
      <p aria-live="polite" className="sr-only">
        {announcement}
      </p>
    </div>
  );
}
