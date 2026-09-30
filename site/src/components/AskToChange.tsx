"use client";

import { Check, FileText, Highlighter, LinkSimple, TextB, TextItalic } from "@phosphor-icons/react";
import { useState } from "react";

type ChangeId = "links" | "font" | "toolbar" | "status";

type Change = {
  id: ChangeId;
  request: string;
  file: string;
  lines: string;
};

/** Each request, and what an agent writes into the vault's `.gasp` folder
    for it. Keys and values follow `crates/config/defaults/`. */
const CHANGES: Change[] = [
  {
    id: "links",
    request: "Make my links green",
    file: "theme.toml",
    lines: `[color]
link = "#1f7a4d"`,
  },
  {
    id: "font",
    request: "Set my notes in a sans-serif font",
    file: "theme.toml",
    lines: `[font]
text = "Helvetica Neue"`,
  },
  {
    id: "toolbar",
    request: "Float a formatting bar over my notes",
    file: "toolbars.toml",
    lines: `[toolbar.formatting]
place   = "editor-top"
surface = "overlay"
items   = ["format.bold", "format.italic",
           "format.highlight", "format.link"]`,
  },
  {
    id: "status",
    request: "Keep only the word count at the bottom",
    file: "toolbars.toml",
    lines: `[toolbar.status]
items = ["spacer", "word-count"]`,
  },
];

const FORMAT_ICONS = [TextB, TextItalic, Highlighter, LinkSimple];

function FormattingBar() {
  return (
    <div className="absolute top-3 left-1/2 flex -translate-x-1/2 gap-1 rounded-full bg-surface p-1 shadow-lifted">
      {FORMAT_ICONS.map((Icon, index) => (
        <span key={index} className="grid size-8 place-items-center rounded-full text-ink-soft">
          <Icon size={17} aria-hidden />
        </span>
      ))}
    </div>
  );
}

function StatusBar({ onlyWords }: { onlyWords: boolean }) {
  const widgets = onlyWords ? ["58 words"] : ["58 words", "312 characters", "1 min read", "4:12"];
  return (
    <div className="small figure flex h-9 items-center justify-end gap-5 px-5 text-ink-muted">
      {widgets.map((widget) => (
        <span key={widget}>{widget}</span>
      ))}
    </div>
  );
}

/** A small Gasp window whose look follows the changes switched on. */
function MockWindow({ on }: { on: Set<ChangeId> }) {
  const link = on.has("links") ? "text-[#1f7a4d] dark:text-[#5cc393]" : "text-ink";
  const font = on.has("font") ? "font-['Helvetica_Neue',Helvetica,Arial,sans-serif]" : "";
  return (
    <div className="overflow-hidden rounded-2xl bg-surface shadow-lifted">
      <div className="flex h-10 items-center gap-2 px-4">
        {["bg-[#ff5f57]", "bg-[#febc2e]", "bg-[#28c840]"].map((light) => (
          <span key={light} aria-hidden className={`size-3 rounded-full ${light}`} />
        ))}
        <span className="small ml-3 text-ink-muted">Trip to Lisbon</span>
      </div>
      <div className={`relative px-6 pt-14 pb-6 sm:px-10 ${font}`}>
        {on.has("toolbar") && <FormattingBar />}
        <p className="text-2xl font-bold">Trip to Lisbon</p>
        <p className="body mt-3 text-ink-soft">
          Flights are booked for the 14th. Ana sent a list of places to eat, which I copied into{" "}
          <span className={`underline decoration-current/40 underline-offset-4 ${link}`}>Food</span>. The tram up to
          the castle is the one to take early, before the queue.
        </p>
        <p className="body mt-3 text-ink-soft">
          Pack light: the flat has a washing machine, and{" "}
          <span className={`underline decoration-current/40 underline-offset-4 ${link}`}>Packing</span> has the rest.
        </p>
      </div>
      <StatusBar onlyWords={on.has("status")} />
    </div>
  );
}

/** What the agent wrote, on its own surface so it reads as the reply. */
function AgentReply({ change }: { change: Change | undefined }) {
  return (
    <div className="mt-4 min-h-40 rounded-2xl bg-fill p-5">
      {change ? (
        <>
          <p className="small flex items-center gap-2 text-ink-muted">
            <FileText size={16} aria-hidden />
            <span>
              Changed <span className="code text-ink">.gasp/{change.file}</span>
            </span>
          </p>
          <pre className="code mt-3 overflow-x-auto leading-relaxed text-ink">{change.lines}</pre>
        </>
      ) : (
        <p className="small text-ink-muted">Pick a request, and the lines it adds to Gasp&apos;s settings show here.</p>
      )}
    </div>
  );
}

function RequestButton({ change, on, onToggle }: { change: Change; on: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      onClick={onToggle}
      className={`flex w-full cursor-pointer items-center justify-between gap-3 rounded-xl px-4 py-3 text-left font-bold transition-[background-color,color,scale] duration-150 active:scale-[0.96] ${on ? "bg-button text-on-button" : "bg-fill text-ink hover:bg-sea"}`}
    >
      {change.request}
      <Check size={18} weight="bold" aria-hidden className={`shrink-0 transition-opacity duration-150 ${on ? "" : "opacity-0"}`} />
    </button>
  );
}

/** Requests someone might type to an agent, each switching on the change
    it would make, so the window beside them updates the instant one is
    picked, as the app does when its files are saved. */
export function AskToChange() {
  const [on, setOn] = useState<Set<ChangeId>>(new Set());
  const [latest, setLatest] = useState<ChangeId | null>(null);

  const toggle = (id: ChangeId) => {
    const turningOn = !on.has(id);
    setOn((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
    setLatest(turningOn ? id : null);
  };

  return (
    <div className="lg:grid lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)] lg:items-start lg:gap-12">
      <ul className="grid gap-2">
        {CHANGES.map((change) => (
          <li key={change.id}>
            <RequestButton change={change} on={on.has(change.id)} onToggle={() => toggle(change.id)} />
          </li>
        ))}
      </ul>
      <div className="mt-8 lg:mt-0">
        <MockWindow on={on} />
        <AgentReply change={CHANGES.find((change) => change.id === latest)} />
      </div>
    </div>
  );
}
