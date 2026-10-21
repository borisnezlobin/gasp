import type { Icon } from "@phosphor-icons/react";
import { ArrowsClockwise, FolderOpen, Lightning, LockKey, Robot, Sigma } from "@phosphor-icons/react/dist/ssr";
import { COMPARISONS } from "./comparisons";
import { COMPARE_PATH, FEATURES_PATH } from "./site";

export type PageLink = { path: string; name: string; summary: string };

export type FeatureLink = PageLink & { icon: Icon };

/** Every feature page, in the order the hub and the footer list them. */
export const FEATURE_LINKS: FeatureLink[] = [
  {
    path: `${FEATURES_PATH}/obsidian-vaults`,
    name: "Works with Obsidian",
    summary: "Open your Obsidian notes in Gasp without converting anything. You can keep using Obsidian too.",
    icon: FolderOpen,
  },
  {
    path: `${FEATURES_PATH}/speed`,
    name: "Speed",
    summary: "A note is ready to type in less than half a second. Typing stays instant, even in very long notes.",
    icon: Lightning,
  },
  {
    path: `${FEATURES_PATH}/sync`,
    name: "Free sync",
    summary: "Keep your notes the same on all your computers through iCloud Drive or GitHub. It costs nothing.",
    icon: ArrowsClockwise,
  },
  {
    path: `${FEATURES_PATH}/locked-notes`,
    name: "Locked notes",
    summary: "Lock private notes with a password. Nobody can read them without it, even in the cloud.",
    icon: LockKey,
  },
  {
    path: `${FEATURES_PATH}/ai-agents`,
    name: "AI assistants",
    summary: "Let Claude and other AI assistants read and edit your notes, and even change how Gasp looks.",
    icon: Robot,
  },
  {
    path: `${FEATURES_PATH}/math`,
    name: "Math",
    summary: "Write equations in LaTeX and see them as finished math while you type.",
    icon: Sigma,
  },
];

/** Every comparison page, named the way people search for it. */
export const COMPARISON_LINKS: PageLink[] = COMPARISONS.map((comparison) => ({
  path: `${COMPARE_PATH}/${comparison.slug}`,
  name: `Gasp vs ${comparison.rival}`,
  summary: comparison.description,
}));
