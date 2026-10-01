import {
  ACCENT_COLOURS,
  FONTS,
  FONT_SIZE,
  HIGHLIGHT_COLOURS,
  STATUS_WIDGETS,
  TOOLBAR_ITEMS,
  TOOLBAR_PLACES,
  TOOLBAR_SURFACES,
} from "./lookChanges";

export const DEFAULT_BASE_URL = "https://openrouter.ai/api/v1";
export const DEFAULT_MODEL = "google/gemma-4-26b-a4b-it:free";
/** The same model, paid: OpenRouter turns to it when the free one is
    rate-limited, at about $0.0001 a request. */
export const DEFAULT_FALLBACK_MODEL = "google/gemma-4-26b-a4b-it";
export const ANSWER_TIMEOUT_MS = 8000;

export type ModelConfig = { baseUrl: string; key: string; model: string; fallbackModel: string | null };

export function modelConfigFromEnv(): ModelConfig | null {
  const key = process.env.DEMO_AI_KEY;
  if (!key) return null;
  return {
    baseUrl: (process.env.DEMO_AI_BASE_URL || DEFAULT_BASE_URL).replace(/\/+$/, ""),
    key,
    model: process.env.DEMO_AI_MODEL || DEFAULT_MODEL,
    fallbackModel: fallbackModelFromEnv(),
  };
}

/** `DEMO_AI_FALLBACK_MODEL`, or the paid Gemma on OpenRouter; set it to
    `none` to never spend credits. Other providers have no fallback list. */
function fallbackModelFromEnv(): string | null {
  const configured = process.env.DEMO_AI_FALLBACK_MODEL;
  if (configured === "none") return null;
  if (configured) return configured;
  const onOpenRouter = !process.env.DEMO_AI_BASE_URL || process.env.DEMO_AI_BASE_URL.includes("openrouter.ai");
  return onOpenRouter ? DEFAULT_FALLBACK_MODEL : null;
}

const names = (values: readonly string[]) => values.map((value) => `"${value}"`).join(", ");

export const INSTRUCTIONS = `You turn a request about how the Gasp notes app looks into settings.
Reply with one JSON object and nothing else. Include only the keys the request asks for:
- "accent": the accent colour, which the cursor, checked tasks and links use. One of ${names(Object.keys(ACCENT_COLOURS))}, or a "#rrggbb" hex.
- "link": the colour of links alone. Same values as "accent".
- "highlight": the colour behind highlighted text. One of ${names(Object.keys(HIGHLIGHT_COLOURS))}, or a "#rrggbb" hex.
- "font": the font notes are set in. One of ${names(Object.keys(FONTS))}. Pick the closest: sans-serif is "Helvetica Neue", rounded or geometric is "Avenir Next", monospace is "Menlo", serif is "Charter".
- "fontSize": the base text size in points, a whole number from ${FONT_SIZE.min} to ${FONT_SIZE.max}. The default is ${FONT_SIZE.default}; "bigger" means about 14, "much bigger" about 16.
- "appearance": one of ${names(["light", "dark", "match-system"])}.
- "toolbar": a formatting toolbar, as {"place": one of ${names(TOOLBAR_PLACES)}, "surface": one of ${names(TOOLBAR_SURFACES)}, "items": a list from ${names(TOOLBAR_ITEMS)}}. "overlay" floats over the note; "strip" gives the bar its own row. Use false to remove the toolbar.
- "statusWidgets": what the status bar at the bottom shows, as a list from ${names(STATUS_WIDGETS)}.
- "statusBar": "hidden" or "shown".
- "foldHeadings": true when the request asks for headings or sections that collapse, fold or hide. Gasp already does this, so don't add a "reply" for it.
- "reply": only when part of the request is something these settings can't do, one short plain sentence saying what Gasp can't do, such as "Gasp can't play music." Leave it out otherwise.
Example: "dark mode with a green accent and a toolbar at the top" gives {"appearance":"dark","accent":"green","toolbar":{"place":"editor-top","surface":"overlay","items":["format.bold","format.italic","format.highlight","format.link"]}}.`;

export class ModelError extends Error {
  constructor(readonly code: "provider-busy" | "provider-failed" | "timeout") {
    super(code);
  }
}

function requestBody(config: ModelConfig, request: string): string {
  return JSON.stringify({
    model: config.model,
    ...(config.fallbackModel && { models: [config.model, config.fallbackModel] }),
    temperature: 0,
    max_tokens: 400,
    response_format: { type: "json_object" },
    messages: [
      { role: "system", content: INSTRUCTIONS },
      { role: "user", content: request },
    ],
  });
}

/** The first JSON object in the model's text, which some models wrap in a
    code fence despite being asked not to. */
export function jsonIn(text: string): unknown {
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0 || end <= start) return null;
  try {
    return JSON.parse(text.slice(start, end + 1));
  } catch {
    return null;
  }
}

function answerText(body: unknown): string {
  const choices = (body as { choices?: { message?: { content?: unknown } }[] })?.choices;
  const content = choices?.[0]?.message?.content;
  return typeof content === "string" ? content : "";
}

async function post(config: ModelConfig, request: string, fetcher: typeof fetch): Promise<Response> {
  try {
    return await fetcher(`${config.baseUrl}/chat/completions`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${config.key}`,
        "Content-Type": "application/json",
        "X-Title": "Gasp website demo",
      },
      body: requestBody(config, request),
      signal: AbortSignal.timeout(ANSWER_TIMEOUT_MS),
    });
  } catch (error) {
    const timedOut = error instanceof DOMException && error.name === "TimeoutError";
    throw new ModelError(timedOut ? "timeout" : "provider-failed");
  }
}

/** Asks the model, and returns the JSON it answered with. */
export async function askModel(config: ModelConfig, request: string, fetcher: typeof fetch = fetch): Promise<unknown> {
  const response = await post(config, request, fetcher);
  if (response.status === 429) throw new ModelError("provider-busy");
  if (!response.ok) throw new ModelError("provider-failed");
  const body: unknown = await response.json().catch(() => null);
  const answer = jsonIn(answerText(body));
  if (answer === null) throw new ModelError("provider-failed");
  return answer;
}
