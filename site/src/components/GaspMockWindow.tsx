"use client";

import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import type { BuiltinDemo } from "@/lib/builtins";
import type { ConfigPatch } from "@/lib/configPatch";
import { mockStyle, pixels, realPixels, showsInText, token } from "@/lib/mockConfig";
import { AppIcon } from "./mock/AppIcon";
import { MockContext, mockState, useMock, useToolbarsAt } from "./mock/mockContext";
import { MockNote } from "./mock/MockNote";
import { DemoOverlay, Notices } from "./mock/MockOverlays";
import { DockedBar, ToolbarItems } from "./mock/MockToolbar";

const COLOUR_EASE = "transition-colors duration-300 ease-out-soft";
const UI_TEXT: CSSProperties = { fontFamily: token("font.ui"), fontSize: realPixels(13) };

function IconButton({ name, label, onClick }: { name: string; label?: string; onClick?: () => void }) {
  const className = "grid shrink-0 place-items-center rounded-md text-(--g-color-icon) hover:bg-(--g-color-fill-strong)";
  const style = { width: realPixels(28), height: realPixels(28) };
  if (!onClick) {
    return (
      <span aria-hidden className={className} style={style}>
        <AppIcon name={name} size={realPixels(16)} />
      </span>
    );
  }
  return (
    <button type="button" onClick={onClick} aria-label={label} className={`${className} cursor-pointer`} style={style}>
      <AppIcon name={name} size={realPixels(16)} />
    </button>
  );
}

function TabBar({ corner, onToggleSidebar }: { corner?: ReactNode; onToggleSidebar: () => void }) {
  return (
    <div className="flex shrink-0 items-center gap-[calc(8*var(--px))] px-[calc(14*var(--px))]" style={{ height: realPixels(44), ...UI_TEXT }}>
      {["bg-[#ff5f57]", "bg-[#febc2e]", "bg-[#28c840]"].map((light) => (
        <span key={light} aria-hidden className={`size-[calc(12*var(--px))] shrink-0 rounded-full ${light}`} />
      ))}
      <span className="w-[calc(12*var(--px))]" />
      <IconButton name="sidebar-simple" label="Show or hide the file sidebar" onClick={onToggleSidebar} />
      <span
        className={`flex min-w-0 items-center justify-between bg-(--g-color-background) text-(--g-color-text) ${COLOUR_EASE}`}
        style={{
          width: realPixels(200),
          height: realPixels(32),
          paddingInline: realPixels(12),
          borderRadius: pixels("radius.md"),
          boxShadow: `0 1px 2px ${token("color.tab-shadow")}, 0 0 0 1px ${token("color.ring")}`,
        }}
      >
        <span className="truncate">Trip to Lisbon</span>
        <AppIcon name="x" size={realPixels(12)} className="text-(--g-color-icon)" />
      </span>
      <span className="figure ml-auto truncate text-(--g-color-text-detail)">{corner}</span>
      <IconButton name="plus" />
      <IconButton name="caret-down" />
      <IconButton name="sidebar-simple-right" />
    </div>
  );
}

const FILES = [
  { name: "Travel", folder: true, depth: 0 },
  { name: "Food", depth: 1 },
  { name: "Packing", depth: 1 },
  { name: "Trip to Lisbon", depth: 1, active: true },
  { name: "Field notes", folder: true, depth: 0 },
  { name: "Recipes", folder: true, depth: 0 },
];

function FileTree() {
  return (
    <ul className="grid" style={{ padding: pixels("space.md"), gap: realPixels(2), ...UI_TEXT }}>
      {FILES.map(({ name, folder, depth, active }) => (
        <li
          key={name}
          className={`flex items-center gap-[0.5em] truncate ${active ? "bg-(--g-color-fill-strong) text-(--g-color-text-strong)" : "text-(--g-color-text)"}`}
          style={{ paddingLeft: `calc((${depth} * 14 + 8) * var(--px))`, height: realPixels(28), borderRadius: pixels("radius.md") }}
        >
          <AppIcon name={folder ? "folder-simple" : "file-text"} size={realPixels(15)} className="text-(--g-color-icon)" />
          {name}
        </li>
      ))}
    </ul>
  );
}

type SidebarProps = { open: boolean; mode: string; onLeave: () => void };

/** The file sidebar. Always-shown and pushing sidebars take room; an
    overlay one slides over the note with a shadow. */
function FileSidebar({ open, mode, onLeave }: SidebarProps) {
  const overlay = mode === "overlay";
  const width = realPixels(210);
  const place = overlay ? "absolute inset-y-0 left-0 z-30 shadow-[2px_0_12px_var(--g-color-shadow)]" : "relative shrink-0";
  return (
    <div
      onPointerLeave={onLeave}
      className={`${place} overflow-hidden bg-(--g-color-sidebar) transition-[width,translate,opacity] duration-200 ease-out-soft ${COLOUR_EASE}`}
      style={{ width: open || overlay ? width : 0, translate: overlay && !open ? "-100% 0" : undefined, opacity: overlay && !open ? 0 : 1 }}
    >
      <div style={{ width }}>
        <FileTree />
      </div>
    </div>
  );
}

const HIDE_DELAY_MS = 300;

/** The sidebar's reveal setting, as the built-in hover rules run it: the
    window's left edge shows it and leaving it hides it after 300ms. */
function useSidebar() {
  const { setting } = useMock();
  const reveal = String(setting("sidebar.files.reveal"));
  const [toggled, setToggled] = useState(false);
  const [hovered, setHovered] = useState(false);
  const hideTimer = useRef<number | undefined>(undefined);
  const open = reveal === "always" ? !toggled : toggled || hovered;
  const showOnHover = () => {
    window.clearTimeout(hideTimer.current);
    if (reveal === "hover") setHovered(true);
  };
  const hideAfterLeaving = () => {
    hideTimer.current = window.setTimeout(() => setHovered(false), HIDE_DELAY_MS);
  };
  return { open, mode: reveal === "always" ? "push" : String(setting("sidebar.files.mode")), showOnHover, hideAfterLeaving, toggle: () => setToggled((current) => !current) };
}

function StatusBar() {
  const bars = useToolbarsAt("status-bar");
  return (
    <div
      aria-hidden
      className={`flex shrink-0 items-center gap-[calc(12*var(--px))] px-[calc(16*var(--px))] text-(--g-color-text-faint) ${COLOUR_EASE}`}
      style={{ height: realPixels(30), fontFamily: token("font.ui"), fontSize: realPixels(12) }}
    >
      {bars.map((toolbar) => (
        <span key={toolbar.id} className="contents">
          <ToolbarItems toolbar={toolbar} />
        </span>
      ))}
    </div>
  );
}

function Breadcrumbs() {
  return (
    <div className="flex shrink-0 items-center" style={{ height: realPixels(40), paddingInline: realPixels(12), ...UI_TEXT }}>
      <span className="flex text-(--g-color-icon-disabled)">
        <AppIcon name="arrow-left" size={realPixels(16)} />
        <AppIcon name="arrow-right" size={realPixels(16)} className="ml-[calc(12*var(--px))]" />
      </span>
      <span className="flex-1 truncate text-center">
        <span className="text-(--g-color-text-detail)">Travel</span>
        <span className="text-(--g-color-text-faint)"> / </span>
        <span className="text-(--g-color-text-strong)">Trip to Lisbon</span>
      </span>
      <span className="flex gap-[calc(12*var(--px))] text-(--g-color-icon)">
        <AppIcon name="book-open" size={realPixels(16)} />
        <AppIcon name="dots-three" size={realPixels(16)} />
      </span>
    </div>
  );
}

const DOCK_EDGES = { "editor-top": "top", "editor-bottom": "bottom", "window-left": "left", "window-right": "right" } as const;
type DockPlace = keyof typeof DOCK_EDGES;

function useDocked(place: DockPlace, floating: boolean) {
  return useToolbarsAt(place).filter((toolbar) => {
    const floats = toolbar.surface === "overlay" || toolbar.behaviour === "on-hover";
    return showsInText(toolbar) && floats === floating;
  });
}

function Strips({ place }: { place: DockPlace }) {
  return useDocked(place, false).map((toolbar) => <DockedBar key={toolbar.id} toolbar={toolbar} edge={DOCK_EDGES[place]} />);
}

function FloatingDocks() {
  const places = Object.keys(DOCK_EDGES) as DockPlace[];
  return places.map((place) => <FloatingDock key={place} place={place} />);
}

function FloatingDock({ place }: { place: DockPlace }) {
  return useDocked(place, true).map((toolbar) => <DockedBar key={toolbar.id} toolbar={toolbar} edge={DOCK_EDGES[place]} />);
}

/** Fades the note back in each time the look changes, so the change reads
    as one event even when it's only a colour. */
function usePulseOn(version: number) {
  const target = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = target.current;
    if (!element || version === 0) return;
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;
    element.animate([{ opacity: 0.35, filter: "blur(3px)" }, { opacity: 1, filter: "blur(0)" }], {
      duration: 360,
      easing: "cubic-bezier(0.22, 1, 0.36, 1)",
    });
  }, [version]);
  return target;
}

function NoteCard({ version, notices }: { version: number; notices: ReactNode[] }) {
  const { demo } = useMock();
  const note = usePulseOn(version);
  return (
    <div
      className={`group/note relative flex min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-(--g-color-background) ${COLOUR_EASE}`}
      style={{ borderRadius: pixels("radius.lg"), boxShadow: `0 0 0 1px ${token("color.ring")}` }}
    >
      <Strips place="editor-top" />
      <Breadcrumbs />
      <div ref={note} className="min-h-0 flex-1 overflow-x-hidden overflow-y-auto overscroll-contain [scrollbar-width:thin]">
        <div
          className="mx-auto"
          style={{ maxWidth: `calc(${token("size.editor-max-width")} * var(--px) + 2 * ${pixels("space.xxl")})`, padding: `${pixels("space.md")} ${pixels("space.xxl")} ${pixels("space.xxl", 3)}` }}
        >
          <MockNote key={version} />
        </div>
      </div>
      <Strips place="editor-bottom" />
      <FloatingDocks />
      <DemoOverlay key={`demo-${version}`} demo={demo} />
      <Notices key={`notices-${version}`} notices={notices} />
    </div>
  );
}

type Sidebar = ReturnType<typeof useSidebar>;

function WindowBody({ sidebar, version, notices }: { sidebar: Sidebar; version: number; notices: ReactNode[] }) {
  return (
    <div className="relative flex min-h-0 flex-1" style={{ paddingInline: realPixels(10), gap: realPixels(6) }}>
      <span aria-hidden className="absolute inset-y-0 left-0 z-20 w-[calc(10*var(--px))]" onPointerEnter={sidebar.showOnHover} />
      <Strips place="window-left" />
      <FileSidebar open={sidebar.open} mode={sidebar.mode} onLeave={sidebar.hideAfterLeaving} />
      <NoteCard version={version} notices={notices} />
      <Strips place="window-right" />
    </div>
  );
}

function SyncPulse() {
  if (useMock().demo !== "sync") return null;
  return (
    <span
      aria-hidden
      className="absolute right-[calc(14*var(--px))] bottom-[calc(4*var(--px))] size-[calc(22*var(--px))] rounded-full ring-2 ring-(--g-color-accent) motion-safe:animate-ping"
    />
  );
}

type FrameProps = { version: number; notices: ReactNode[]; corner?: ReactNode };

function WindowFrame({ version, notices, corner }: FrameProps) {
  const sidebar = useSidebar();
  return (
    <>
      <TabBar corner={corner} onToggleSidebar={sidebar.toggle} />
      <WindowBody sidebar={sidebar} version={version} notices={notices} />
      <div className="relative">
        <SyncPulse />
        <StatusBar />
      </div>
    </>
  );
}

type MockWindowProps = Omit<FrameProps, "notices"> & { patch: ConfigPatch; demo?: BuiltinDemo; notices?: ReactNode[] };

/** A small Gasp window drawn from the same keys the app reads: every theme
    token is a CSS variable here, and settings and toolbars resolve as in
    `crates/config`. Its size never changes. */
export function GaspMockWindow({ patch, demo, notices = [], version, corner }: MockWindowProps) {
  return (
    <MockContext value={mockState(patch, demo)}>
      <div
        style={mockStyle(patch)}
        className={`relative flex h-[30rem] flex-col overflow-hidden rounded-2xl bg-(--g-color-app-background) text-(--g-color-text) shadow-lifted [--px:0.62px] sm:h-[34rem] sm:[--px:0.78px] ${COLOUR_EASE}`}
      >
        <WindowFrame version={version} notices={notices} corner={corner} />
      </div>
    </MockContext>
  );
}
