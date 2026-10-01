import type { LookChanges } from "./lookChanges";

export type Suggestion = { request: string; changes: LookChanges };

/** Example requests, each with the changes the route returns for a model
    that reads it well. The page shows these when the live demo can't
    answer, and says they're saved examples. */
export const SUGGESTIONS: Suggestion[] = [
  {
    request: "Make it dark with a green accent",
    changes: { appearance: "dark", accent: "green" },
  },
  {
    request: "Float a formatting bar over the top",
    changes: {
      toolbar: {
        place: "editor-top",
        surface: "overlay",
        items: ["format.bold", "format.italic", "format.highlight", "format.link"],
      },
    },
  },
  {
    request: "Set my notes in Avenir, a bit bigger",
    changes: { font: "Avenir Next", fontSize: 14 },
  },
  {
    request: "Show only the word count at the bottom",
    changes: { statusWidgets: ["word-count"] },
  },
];

export function suggestionFor(request: string): Suggestion | undefined {
  return SUGGESTIONS.find((suggestion) => suggestion.request === request.trim());
}
