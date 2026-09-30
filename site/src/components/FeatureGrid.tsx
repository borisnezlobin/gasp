import {
  ArrowsClockwise,
  Command,
  Export,
  FolderOpen,
  MagnifyingGlass,
  Robot,
  Sigma,
  TextAa,
} from "@phosphor-icons/react/dist/ssr";
import type { Icon } from "@phosphor-icons/react";

type Feature = { icon: Icon; name: string; detail: string };

const FEATURES: Feature[] = [
  { icon: MagnifyingGlass, name: "Search", detail: "Every note, as you type" },
  { icon: Command, name: "Command palette", detail: "⌘P runs anything" },
  { icon: Sigma, name: "Math", detail: "LaTeX renders in place" },
  { icon: TextAa, name: "Spelling and grammar", detail: "Checked offline, on your Mac" },
  { icon: ArrowsClockwise, name: "Sync", detail: "Free, through iCloud or GitHub" },
  { icon: FolderOpen, name: "Obsidian vaults", detail: "Open the folder you have" },
  { icon: Robot, name: "AI agents", detail: "Claude can read and edit notes" },
  { icon: Export, name: "HTML export", detail: "Any note as a web page" },
];

export function FeatureGrid() {
  return (
    <ul className="grid grid-cols-2 gap-x-6 gap-y-10 lg:grid-cols-4">
      {FEATURES.map(({ icon: FeatureIcon, name, detail }) => (
        <li key={name}>
          <FeatureIcon size={28} className="text-ink" aria-hidden />
          <p className="mt-3 font-bold">{name}</p>
          <p className="small mt-0.5 text-ink-muted">{detail}</p>
        </li>
      ))}
    </ul>
  );
}
