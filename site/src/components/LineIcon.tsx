import type { ReactNode } from "react";

/** Holds an icon beside text so it sits on the text's first line however
    the text wraps. The class in globals.css explains the optical drop. */
export function LineIcon({ children }: { children: ReactNode }) {
  return <span className="line-icon">{children}</span>;
}
