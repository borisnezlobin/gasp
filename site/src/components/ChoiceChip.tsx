import type { ReactNode } from "react";

/** A choice the reader can turn on: quiet when off, raised and ringed in
    ink when on. */
export function ChoiceChip({ chosen, onClick, children }: { chosen: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      aria-pressed={chosen}
      onClick={onClick}
      className={`flex min-h-11 w-full cursor-pointer items-start gap-2.5 rounded-xl px-4 py-2.5 text-left transition-[background-color,box-shadow,scale] duration-150 ease-out-soft active:scale-[0.96] ${
        chosen ? "bg-surface text-ink shadow-lifted ring-2 ring-ink" : "bg-fill text-ink-soft hover:bg-sea"
      }`}
    >
      {children}
    </button>
  );
}
