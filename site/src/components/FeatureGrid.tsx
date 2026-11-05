import {
  ArrowsClockwise,
  Command,
  FolderOpen,
  LockKey,
  MagnifyingGlass,
  Robot,
  Sigma,
  TextAa,
} from "@phosphor-icons/react/dist/ssr";
import type { Icon } from "@phosphor-icons/react";
import Link from "next/link";
import { FEATURES_PATH } from "@/lib/site";
import { LiveText } from "./glyphs/LiveText";
import { linkStyle } from "./styles";

type Feature = { icon: Icon; name: string; detail: string; href?: string };

const FEATURES: Feature[] = [
  { icon: MagnifyingGlass, name: "Search", detail: "Every note, as you type" },
  { icon: Command, name: "Command palette", detail: "⌘P runs anything" },
  { icon: Sigma, name: "Math", detail: "LaTeX renders in place", href: `${FEATURES_PATH}/math` },
  { icon: TextAa, name: "Spelling and grammar", detail: "Checked offline, on your Mac" },
  { icon: ArrowsClockwise, name: "Sync", detail: "Free, through iCloud or GitHub", href: `${FEATURES_PATH}/sync` },
  { icon: FolderOpen, name: "Obsidian vaults", detail: "Switch with no hassle", href: `${FEATURES_PATH}/obsidian-vaults` },
  { icon: Robot, name: "AI assistants", detail: "Your AI can edit notes, and even Gasp itself", href: `${FEATURES_PATH}/ai-agents` },
  { icon: LockKey, name: "Locked notes", detail: "Encrypted on disk and in sync", href: `${FEATURES_PATH}/locked-notes` },
];

function FeatureName({ feature }: { feature: Feature }) {
  if (!feature.href) return <LiveText text={feature.name} />;
  return (
    <Link href={feature.href} className={linkStyle}>
      <LiveText text={feature.name} />
    </Link>
  );
}

export function FeatureGrid() {
  return (
    <ul className="grid grid-cols-2 gap-x-6 gap-y-10 lg:grid-cols-4">
      {FEATURES.map((feature) => (
        <li key={feature.name}>
          <feature.icon size={28} className="text-ink" aria-hidden />
          <p className="mt-3 font-bold">
            <FeatureName feature={feature} />
          </p>
          <p className="small mt-0.5 text-ink-muted">{feature.detail}</p>
        </li>
      ))}
    </ul>
  );
}
