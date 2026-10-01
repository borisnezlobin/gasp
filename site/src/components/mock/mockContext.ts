"use client";

import { createContext, useContext } from "react";
import type { BuiltinDemo } from "@/lib/builtins";
import type { ConfigPatch } from "@/lib/configPatch";
import type { SettingValue } from "@/lib/gaspSchema";
import { resolvedToolbars, settingValue, type ResolvedToolbar } from "@/lib/mockConfig";

export type MockState = {
  patch: ConfigPatch;
  toolbars: ResolvedToolbar[];
  demo?: BuiltinDemo;
  setting: (key: string) => SettingValue;
};

export function mockState(patch: ConfigPatch, demo?: BuiltinDemo): MockState {
  return { patch, demo, toolbars: resolvedToolbars(patch), setting: (key) => settingValue(patch, key) };
}

export const MockContext = createContext<MockState>(mockState({}));

export const useMock = () => useContext(MockContext);

export function useToolbarsAt(place: string): ResolvedToolbar[] {
  return useMock().toolbars.filter((toolbar) => toolbar.place === place);
}

/** How near the cursor a piece of Markdown syntax sits: in the element the
    cursor is in, on its line, or in its block. */
export type Nearness = "element" | "line" | "block";

const SCOPE_REACH: Record<string, number> = { element: 0, line: 1, block: 2 };

/** Whether a piece of syntax shows, as `markdown.symbols` decides: its
    kind's override or the mode, and for `around-cursor` the reveal scope. */
export function useSymbolShown(syntax: string, near?: Nearness): boolean {
  const { setting } = useMock();
  const override = setting(`markdown.symbols.overrides.${syntax}`);
  const mode = override ?? setting("markdown.symbols.mode");
  if (mode === "always-shown") return true;
  if (mode !== "around-cursor" || !near) return false;
  return SCOPE_REACH[String(setting("markdown.symbols.scope"))] >= SCOPE_REACH[near];
}
