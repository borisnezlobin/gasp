"use client";

import type { CSSProperties } from "react";
import { COMMANDS, TOOLBARS } from "@/lib/gaspSchema";
import { pixels, realPixels, token, type ResolvedToolbar } from "@/lib/mockConfig";
import { AppIcon, itemIcon } from "./AppIcon";

const WIDGET_TEXT: Record<string, string> = {
  "word-count": "152 words",
  "character-count": "873 characters",
  "reading-time": "1 min read",
  "edit-time": "4 min",
  "cursor-position": "14:38",
};

const UI_FONT: CSSProperties = { fontFamily: token("font.ui"), fontSize: realPixels(13) };

type BarShape = { density: string; style: string };

const buttonSide = ({ density }: BarShape) => pixels(`toolbar.${density}-button`);
const iconSide = ({ density }: BarShape) => pixels(`toolbar.${density}-icon`);

function itemTitle(item: string): string {
  const menu = item.startsWith("menu:") ? TOOLBARS.menus[item.slice(5)] : undefined;
  return menu?.title ?? COMMANDS.get(item)?.title ?? item;
}

function menuIcon(item: string): string {
  return TOOLBARS.menus[item.slice(5)]?.icon ?? "dots-three";
}

function CommandButton({ item, bar }: { item: string; bar: BarShape }) {
  const showsLabel = bar.style !== "icons";
  const showsIcon = bar.style !== "labels";
  const isMenu = item.startsWith("menu:");
  return (
    <span
      title={itemTitle(item)}
      className="flex shrink-0 items-center justify-center text-(--g-color-icon) transition-colors duration-150 hover:bg-(--g-color-fill-strong) hover:text-(--g-color-icon-strong)"
      style={{
        minWidth: buttonSide(bar),
        height: buttonSide(bar),
        borderRadius: pixels("toolbar.button-radius"),
        gap: pixels("toolbar.label-gap"),
        paddingInline: showsLabel ? pixels("toolbar.padding", 2) : undefined,
      }}
    >
      {showsIcon && <AppIcon name={isMenu ? menuIcon(item) : itemIcon(item)} size={iconSide(bar)} />}
      {showsLabel && (
        <span className="whitespace-nowrap text-(--g-color-text)" style={UI_FONT}>
          {itemTitle(item)}
        </span>
      )}
      {isMenu && <AppIcon name="caret-down" size={realPixels(10)} />}
    </span>
  );
}

function Separator({ vertical }: { vertical: boolean }) {
  const width = pixels("toolbar.separator-width");
  const size = vertical ? { height: width, width: realPixels(16) } : { width, height: realPixels(16) };
  return <span className="shrink-0 bg-(--g-color-divider)" style={{ ...size, marginInline: vertical ? undefined : realPixels(4) }} />;
}

function WidgetText({ item }: { item: string }) {
  if (item === "sync") return <AppIcon name="cloud-check" size={realPixels(14)} className="text-(--g-color-syncing)" />;
  return <span className="whitespace-nowrap">{WIDGET_TEXT[item]}</span>;
}

function ToolbarItem({ item, bar, vertical }: { item: string; bar: BarShape; vertical: boolean }) {
  if (item === "separator") return <Separator vertical={vertical} />;
  if (item === "spacer") return <span className="flex-1" />;
  if (Object.hasOwn(WIDGET_TEXT, item) || item === "sync") return <WidgetText item={item} />;
  return <CommandButton item={item} bar={bar} />;
}

export function ToolbarItems({ toolbar, vertical = false }: { toolbar: ResolvedToolbar; vertical?: boolean }) {
  return toolbar.items.map((item, index) => <ToolbarItem key={`${item}-${index}`} item={item} bar={toolbar} vertical={vertical} />);
}

/** A pill floating over the note: the popover surface with its ring and
    shadow, its corners the button radius plus the padding. */
export const PILL_STYLE: CSSProperties = {
  padding: pixels("toolbar.padding"),
  borderRadius: `calc((${token("toolbar.button-radius")} + ${token("toolbar.padding")}) * var(--px))`,
  boxShadow: `0 0 0 1px ${token("color.popover-ring")}, 0 ${realPixels(4)} ${realPixels(16)} ${token("color.popover-shadow")}`,
};

const gapOf = (toolbar: ResolvedToolbar) => pixels(`toolbar.${toolbar.density}-gap`);

const APPEAR = "transition-[opacity,translate] duration-300 ease-out-soft starting:opacity-0 motion-safe:starting:translate-y-1";

/** On-hover bars float over their edge rather than taking room, and show
    while the pointer is on the note, as Gasp reveals them near the edge. */
const hoverClass = (toolbar: ResolvedToolbar) =>
  toolbar.behaviour === "on-hover" ? "opacity-0 transition-opacity duration-150 group-hover/note:opacity-100" : APPEAR;

export function FloatingBar({ toolbar, className = "" }: { toolbar: ResolvedToolbar; className?: string }) {
  return (
    <span
      aria-hidden
      className={`z-20 inline-flex items-center bg-(--g-color-popover) ${APPEAR} ${className}`}
      style={{ ...PILL_STYLE, gap: gapOf(toolbar) }}
    >
      <ToolbarItems toolbar={toolbar} />
    </span>
  );
}

type Edge = "top" | "bottom" | "left" | "right";

const OVERLAY_POSITIONS: Record<Edge, string> = {
  top: "absolute left-1/2 -translate-x-1/2 flex-row",
  bottom: "absolute left-1/2 -translate-x-1/2 flex-row",
  left: "absolute top-1/2 -translate-y-1/2 flex-col",
  right: "absolute top-1/2 -translate-y-1/2 flex-col",
};

const STRIP_SHAPES: Record<Edge, string> = {
  top: "flex-row shadow-[0_1px_0_var(--g-color-divider)]",
  bottom: "flex-row shadow-[0_-1px_0_var(--g-color-divider)]",
  left: "flex-col",
  right: "flex-col",
};

const insetFor = (edge: Edge): CSSProperties => ({ [edge]: pixels("space.md") });

/** A bar docked on an edge: a strip of its own, or a pill over the note. */
export function DockedBar({ toolbar, edge }: { toolbar: ResolvedToolbar; edge: Edge }) {
  const vertical = edge === "left" || edge === "right";
  const overlay = toolbar.surface === "overlay" || toolbar.behaviour === "on-hover";
  const shape = overlay ? `${OVERLAY_POSITIONS[edge]} z-20 bg-(--g-color-popover)` : `${STRIP_SHAPES[edge]} relative`;
  const style: CSSProperties = overlay
    ? { ...PILL_STYLE, ...insetFor(edge), gap: gapOf(toolbar) }
    : { padding: pixels("toolbar.padding", 2), gap: gapOf(toolbar) };
  return (
    <div aria-hidden className={`flex shrink-0 items-center ${shape} ${hoverClass(toolbar)}`} style={style}>
      <ToolbarItems toolbar={toolbar} vertical={vertical} />
    </div>
  );
}
