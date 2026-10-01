import { createElement, type CSSProperties } from "react";
import iconsJson from "@/lib/gaspIcons.json";
import { COMMANDS } from "@/lib/gaspSchema";

type Shape = [tag: string, attributes: Record<string, string>];

/** The Phosphor icons Gasp's desktop app bundles, by name, read from
    `apps/desktop/assets/icons` by `npm run schema`. */
const ICONS = iconsJson as unknown as Record<string, Shape[]>;

const FALLBACK = "lightning";

const WIDGET_ICONS: Record<string, string> = {
  "word-count": "text-aa",
  "character-count": "text-t",
  "reading-time": "book-open",
  "edit-time": "clock",
  "cursor-position": "cursor-text",
  sync: "cloud-check",
};

/** The icon a toolbar item shows, as the registry gives it. */
export function itemIcon(item: string): string {
  return COMMANDS.get(item)?.icon ?? WIDGET_ICONS[item] ?? FALLBACK;
}

type AppIconProps = { name: string; size: string; className?: string; style?: CSSProperties };

export function AppIcon({ name, size, className, style }: AppIconProps) {
  const shapes = ICONS[name] ?? ICONS[FALLBACK] ?? [];
  return (
    <svg
      viewBox="0 0 256 256"
      fill="currentColor"
      aria-hidden
      className={`shrink-0 ${className ?? ""}`}
      style={{ width: size, height: size, ...style }}
    >
      {shapes.map(([tag, attributes], index) => createElement(tag, { key: index, ...attributes }))}
    </svg>
  );
}
