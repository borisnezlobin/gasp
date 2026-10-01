import { answerFrom, type ChangeAnswer } from "./changeAnswer";

export type Suggestion = { request: string; answer: ChangeAnswer };

/** Example requests, each with JSON a model that reads it well returns,
    passed through the same validation as a live answer. The page shows
    these when the live demo can't answer, and says they're saved examples. */
const SAVED: { request: string; modelJson: unknown }[] = [
  {
    request: "Make it cosy, like a paper notebook",
    modelJson: {
      theme: {
        "color.background": "#fbf6ec",
        "color.app-background": "#f1e9da",
        "color.sidebar": "#efe6d4",
        "color.gray-800": "#3b3128",
        "color.accent": "#9a5b1e",
        "color.code-background": "#f3ead9",
        "dark.color.background": "#221d17",
        "dark.color.app-background": "#1a1612",
        "dark.color.gray-800": "#eadfcd",
        "dark.color.accent": "#e2ab77",
        "font.text": "Iowan Old Style",
        "font.line-height.body": 1.75,
      },
    },
  },
  {
    request: "Make it look like a terminal",
    modelJson: {
      settings: { "appearance.theme": "dark" },
      theme: {
        "font.text": "Menlo",
        "font.ui": "Menlo",
        "font.code": "Menlo",
        "dark.color.background": "#0c0f0c",
        "dark.color.app-background": "#070907",
        "dark.color.gray-800": "#7ee787",
        "dark.color.text-strong": "#a8f0ae",
        "dark.color.accent": "#7ee787",
        "radius.sm": 0,
        "radius.md": 0,
        "radius.lg": 0,
      },
    },
  },
  {
    request: "Put a formatting bar down the left side",
    modelJson: {
      toolbars: {
        formatting: {
          place: "window-left",
          items: ["format.bold", "format.italic", "format.highlight", "separator", "format.link", "edit.toggle-task"],
        },
      },
    },
  },
  {
    request: "Make ⌘D duplicate the line",
    modelJson: { keys: [{ keys: "Mod+D", do: "edit.duplicate-line" }] },
  },
  {
    request: "Make headings collapsible",
    modelJson: { builtin: "fold" },
  },
  {
    request: "Highlight my long sentences",
    modelJson: { settings: { "prose.sentence-length.enabled": true } },
  },
];

export const SUGGESTIONS: Suggestion[] = SAVED.map(({ request, modelJson }) => ({ request, answer: answerFrom(modelJson) }));

export function suggestionFor(request: string): Suggestion | undefined {
  return SUGGESTIONS.find((suggestion) => suggestion.request === request.trim());
}
