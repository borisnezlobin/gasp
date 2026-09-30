"use client";

import { FileText } from "@phosphor-icons/react";
import { useState } from "react";

type ConfigFile = { name: string; purpose: string; sample: string };

/** Trimmed from the app's built-in files (`crates/config/defaults/` and
    `crates/snippets/defaults/`). */
const FILES: ConfigFile[] = [
  {
    name: "rules.toml",
    purpose: "Shortcuts and what they do",
    sample: `[[rule]]
id   = "key.palette.open"
on   = "key"
keys = "Mod+P"
do   = "palette.open"`,
  },
  {
    name: "settings.toml",
    purpose: "Every setting",
    sample: `[markdown.symbols]
mode  = "around-cursor"
scope = "element"

[editor]
smart-quotes = true
auto-pair    = true`,
  },
  {
    name: "theme.toml",
    purpose: "Colours, fonts and spacing",
    sample: `[color]
caret-mark = "#c8283c"

[font]
text = "Charter"
code = "Courier New"`,
  },
  {
    name: "toolbars.toml",
    purpose: "Which buttons go where",
    sample: `[toolbar.status]
place     = "status-bar"
behaviour = "always"
items     = ["word-count", "reading-time",
             "cursor-position", "sync"]`,
  },
  {
    name: "snippets.txt",
    purpose: "Text that expands as you type",
    sample: `mk     → $●$                   text, instant
dm     → $$⏎●⏎$$               text, instant
reals  → \\mathbb{R}            math, instant`,
  },
];

/** The vault's `.gasp` folder: pick a file to see a piece of it. */
export function ConfigFiles() {
  const [open, setOpen] = useState(0);
  const file = FILES[open];
  return (
    <div className="overflow-hidden rounded-2xl bg-surface shadow-lifted md:grid md:grid-cols-[15rem_minmax(0,1fr)]">
      <div className="border-b border-rule p-3 md:border-r md:border-b-0">
        <p className="small px-3 pt-1 pb-2 text-ink-muted">
          <span className="code">.gasp</span> in your vault
        </p>
        <ul>
          {FILES.map((each, index) => (
            <li key={each.name}>
              <button
                type="button"
                aria-pressed={index === open}
                onClick={() => setOpen(index)}
                className={`flex h-10 w-full cursor-pointer items-center gap-2.5 rounded-lg px-3 text-left ${index === open ? "bg-fill font-bold" : "hover:bg-fill"}`}
              >
                <FileText size={18} className="shrink-0 text-ink-muted" aria-hidden />
                <span className="code truncate">{each.name}</span>
              </button>
            </li>
          ))}
        </ul>
      </div>
      <div className="min-w-0 p-6">
        <p className="small text-ink-muted">{file.purpose}</p>
        <pre className="code mt-4 overflow-x-auto leading-relaxed text-ink">{file.sample}</pre>
      </div>
    </div>
  );
}
