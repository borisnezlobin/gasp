"use client";

import { Check, Robot } from "@phosphor-icons/react";
import { useState } from "react";
import { ChoiceChip } from "./ChoiceChip";
import { LineIcon } from "./LineIcon";

type Exchange = { request: string; calls: string[]; reply: string };

/** Requests an agent can carry out through Gasp's MCP server, with the tool
    calls it makes, named and shaped as crates/mcp defines them. */
const EXCHANGES: Exchange[] = [
  {
    request: "What did I write about Parseval's identity?",
    calls: ['search({ query: "Parseval" })', 'read_note({ path: "Math/Fourier series" })'],
    reply: "Your Fourier series note works Parseval's identity down to π²/8, then turns to the discrete Fourier transform.",
  },
  {
    request: "Make my highlights pale blue",
    calls: ['set_theme_token({ token: "color.highlight", value: "#cfe8ff" })'],
    reply: "Done. Every highlight in Gasp is pale blue now, on every device that syncs this vault.",
  },
  {
    request: "Move every note tagged #draft into Drafts",
    calls: [
      'search({ query: "tag:draft" })',
      'move_note({ from: "Essay on tides", to: "Drafts/Essay on tides" })',
      'move_note({ from: "Grid frequency", to: "Drafts/Grid frequency" })',
    ],
    reply: "Moved two notes into Drafts.",
  },
  {
    request: "Open today's daily note with ⌘⇧D",
    calls: ["get_rules()", 'set_rules({ text: "… keys = \\"Mod+Shift+D\\", do = \\"daily.open\\" …" })'],
    reply: "⌘⇧D now opens today's daily note.",
  },
];

function Turn({ exchange, shown }: { exchange: Exchange; shown: boolean }) {
  return (
    <div aria-hidden={!shown} className={`min-w-0 [grid-area:1/1] ${shown ? "" : "invisible"}`}>
      <p className="font-bold">You</p>
      <p className="body mt-1">{exchange.request}</p>
      <div className="mt-6 rounded-2xl bg-surface p-5 shadow-lifted">
        <p className="flex items-center gap-2 font-bold">
          <Robot size={20} aria-hidden />
          Your assistant
        </p>
        <ol className="mt-4 space-y-2">
          {exchange.calls.map((call) => (
            <li key={call} className="flex gap-2.5 text-ink-soft">
              <LineIcon>
                <Check size={16} weight="bold" aria-hidden className="text-ink-muted" />
              </LineIcon>
              <code className="code min-w-0 break-words">{call}</code>
            </li>
          ))}
        </ol>
        <p className="body mt-4">{exchange.reply}</p>
      </div>
    </div>
  );
}

/** Pick something to ask, and see the agent's turn: each tool it calls in
    Gasp, then its answer. Every turn is laid out in the same cell, so the
    tallest one holds the space and nothing below moves. */
export function AgentDemo() {
  const [chosen, setChosen] = useState(0);
  return (
    <div className="grid gap-8 md:grid-cols-[minmax(0,18rem)_1fr]">
      <ul className="space-y-2" aria-label="Requests">
        {EXCHANGES.map((item, index) => (
          <li key={item.request}>
            <ChoiceChip chosen={index === chosen} onClick={() => setChosen(index)}>
              {item.request}
            </ChoiceChip>
          </li>
        ))}
      </ul>
      <div aria-live="polite" className="grid min-w-0 content-start">
        {EXCHANGES.map((item, index) => (
          <Turn key={item.request} exchange={item} shown={index === chosen} />
        ))}
      </div>
    </div>
  );
}
