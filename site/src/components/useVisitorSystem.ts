"use client";

import { useSyncExternalStore } from "react";
import { visitorSystem, type VisitorSystem } from "@/lib/visitorSystem";

type NavigatorWithHints = Navigator & { userAgentData?: { platform?: string } };

function fromBrowser(): VisitorSystem {
  const browser = navigator as NavigatorWithHints;
  return visitorSystem({
    platform: browser.userAgentData?.platform,
    userAgent: browser.userAgent,
    maxTouchPoints: browser.maxTouchPoints,
  });
}

const noChanges = () => () => {};

/** The visitor's system once the page runs in their browser. The page as
    the server sends it, and as crawlers read it, names no system. */
export function useVisitorSystem(): VisitorSystem | null {
  return useSyncExternalStore<VisitorSystem | null>(noChanges, fromBrowser, () => null);
}
