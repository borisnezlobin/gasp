"use client";

import type { VisitorSystem } from "@/lib/visitorSystem";
import { useVisitorSystem } from "./useVisitorSystem";

/** Says "Your system" on the download that fits the visitor, once the
    page knows what they're on. */
export function YourSystemMark({ system }: { system: VisitorSystem }) {
  if (useVisitorSystem() !== system) return null;
  return <span className="small rounded-full bg-highlight px-2.5 py-0.5 font-bold text-ink">Your system</span>;
}
