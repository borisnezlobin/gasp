import type { LookChanges } from "./lookChanges";

type Describers = { [Key in keyof LookChanges]-?: (value: NonNullable<LookChanges[Key]>) => string };

const PLACES = { "editor-top": "at the top", "editor-bottom": "at the bottom" };

const DESCRIBERS: Describers = {
  accent: (colour) => `${colour} accent`,
  link: (colour) => `${colour} links`,
  highlight: (colour) => `${colour} highlights`,
  font: (family) => `text in ${family}`,
  fontSize: (size) => `${size} point text`,
  appearance: (appearance) => (appearance === "match-system" ? "appearance that follows the system" : `${appearance} appearance`),
  toolbar: (toolbar) => `a formatting bar ${PLACES[toolbar.place]}`,
  statusWidgets: (widgets) => `status bar showing ${widgets.length ? widgets.join(", ").replaceAll("-", " ") : "nothing"}`,
  statusBar: (state) => `status bar ${state}`,
  foldHeadings: () => "a heading folded",
};

/** The changes in words, for screen readers, such as "dark appearance,
    green accent". */
export function describeChanges(changes: LookChanges): string {
  return Object.entries(changes)
    .map(([key, value]) => {
      if (value === null) return key === "toolbar" ? "no formatting bar" : "";
      const describe = DESCRIBERS[key as keyof LookChanges] as (input: unknown) => string;
      return describe(value);
    })
    .filter(Boolean)
    .join(", ");
}
