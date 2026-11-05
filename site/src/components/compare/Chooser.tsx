"use client";

import { Check, Plus } from "@phosphor-icons/react";
import { useState } from "react";
import type { Side } from "@/lib/comparison";
import { ChoiceChip } from "../ChoiceChip";
import { LineIcon } from "../LineIcon";

type Pick = `${Side}:${number}`;

/** How far the caret may lean from the middle, as a share of half the bar. */
const MOST_LEAN = 0.8;

function leanOf(picked: Set<Pick>) {
  const sides = [...picked].map((pick) => pick.split(":")[0]);
  const gasp = sides.filter((side) => side === "gasp").length;
  return { gasp, rival: sides.length - gasp };
}

function verdictOf(gasp: number, rival: number, rivalName: string) {
  if (gasp + rival === 0) return "Pick what you need from either list.";
  if (gasp === rival) return "Either one fits what you picked.";
  return gasp > rival ? "Gasp fits what you picked." : `${rivalName} fits what you picked.`;
}

function Reason({ text, chosen, onToggle }: { text: string; chosen: boolean; onToggle: () => void }) {
  const Mark = chosen ? Check : Plus;
  return (
    <li>
      <ChoiceChip chosen={chosen} onClick={onToggle}>
        <LineIcon>
          <Mark size={18} weight="bold" aria-hidden className={chosen ? "text-ink" : "text-ink-muted"} />
        </LineIcon>
        {text}
      </ChoiceChip>
    </li>
  );
}

function Reasons({
  side,
  name,
  reasons,
  picked,
  toggle,
}: {
  side: Side;
  name: string;
  reasons: string[];
  picked: Set<Pick>;
  toggle: (pick: Pick) => void;
}) {
  return (
    <div>
      <h3 className="subheading">{name} fits if you</h3>
      <ul className="mt-4 space-y-2">
        {reasons.map((reason, index) => {
          const pick: Pick = `${side}:${index}`;
          return <Reason key={reason} text={reason} chosen={picked.has(pick)} onToggle={() => toggle(pick)} />;
        })}
      </ul>
    </div>
  );
}

/** The bar between the two apps, with the red caret leaning toward the one
    that fits what the reader picked. */
function Balance({ rival, gaspCount, rivalCount }: { rival: string; gaspCount: number; rivalCount: number }) {
  const total = gaspCount + rivalCount;
  const lean = total === 0 ? 0 : ((gaspCount - rivalCount) / total) * MOST_LEAN;
  return (
    <div data-overlap="design" className="sticky bottom-0 z-10 mt-6 bg-paper pt-4 pb-5 md:static md:mt-10 md:bg-transparent md:p-0">
      <div aria-hidden className="relative h-10">
        <div className="absolute inset-x-0 top-1/2 h-0.5 -translate-y-1/2 rounded-full bg-sea" />
        <div className="absolute top-1/2 left-1/2 h-3 w-0.5 -translate-x-1/2 -translate-y-1/2 rounded-full bg-ink-muted" />
        <span
          className="absolute top-0 h-10 w-1 -translate-x-1/2 rounded-full bg-caret transition-[left] duration-300 ease-out-soft"
          style={{ left: `${50 + lean * 50}%` }}
        />
      </div>
      <div className="mt-2 flex justify-between font-bold">
        <span>{rival}</span>
        <span>Gasp</span>
      </div>
      <p aria-live="polite" className="lede mt-2 text-center font-bold md:mt-4">
        {verdictOf(gaspCount, rivalCount, rival)}
      </p>
    </div>
  );
}

/** The two lists of reasons to pick each app. Choosing the ones that apply
    to you leans the caret toward the app that fits. */
export function Chooser({ rival, reasons }: { rival: string; reasons: { gasp: string[]; rival: string[] } }) {
  const [picked, setPicked] = useState<Set<Pick>>(new Set());
  const toggle = (pick: Pick) =>
    setPicked((current) => {
      const next = new Set(current);
      if (!next.delete(pick)) next.add(pick);
      return next;
    });
  const { gasp, rival: rivalCount } = leanOf(picked);
  return (
    <div>
      <div className="grid gap-10 md:grid-cols-2 md:gap-8">
        <Reasons side="rival" name={rival} reasons={reasons.rival} picked={picked} toggle={toggle} />
        <Reasons side="gasp" name="Gasp" reasons={reasons.gasp} picked={picked} toggle={toggle} />
      </div>
      <Balance rival={rival} gaspCount={gasp} rivalCount={rivalCount} />
    </div>
  );
}
