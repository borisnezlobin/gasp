import { askModel, ModelError, modelConfigFromEnv, type ModelConfig } from "./changeModel";
import { memoryCounter, overLimit, visitorId, type Counter } from "./demoLimits";
import { hasChanges, parseChanges, parseReply, type LookChanges } from "./lookChanges";
import { settingsFiles, type SettingsFile } from "./lookFiles";

export const MAX_REQUEST_LENGTH = 200;
const MAX_BODY_BYTES = 2000;

export type ChangeErrorCode =
  | "bad-request"
  | "too-long"
  | "no-key"
  | "rate-limited"
  | "provider-busy"
  | "provider-failed"
  | "timeout";

export type ChangeAnswer = { changes: LookChanges; files: SettingsFile[]; reply?: string };

const STATUS: Record<ChangeErrorCode, number> = {
  "bad-request": 400,
  "too-long": 413,
  "no-key": 503,
  "rate-limited": 429,
  "provider-busy": 503,
  "provider-failed": 502,
  timeout: 504,
};

const NOTHING_TO_CHANGE = "Gasp's settings can't change that.";
const HEADINGS_ALREADY_FOLD = "Headings already fold in Gasp: click the arrow beside one, or press ⌘⌥[.";

const failure = (error: ChangeErrorCode) => Response.json({ error }, { status: STATUS[error] });

type Deps = { config?: ModelConfig | null; fetcher?: typeof fetch; counter?: Counter };

type ReadRequest = { wanted: string } | { error: ChangeErrorCode };

function parseJson(text: string): unknown {
  try {
    return JSON.parse(text);
  } catch {
    return null;
  }
}

async function readRequest(request: Request): Promise<ReadRequest> {
  const text = await request.text().catch(() => "");
  if (text.length > MAX_BODY_BYTES) return { error: "too-long" };
  const wanted = (parseJson(text) as { request?: unknown } | null)?.request;
  if (typeof wanted !== "string" || !wanted.trim()) return { error: "bad-request" };
  if (wanted.length > MAX_REQUEST_LENGTH) return { error: "too-long" };
  return { wanted: wanted.trim() };
}

async function limited(request: Request, counter: Counter | undefined): Promise<boolean> {
  const visitor = visitorId(request);
  try {
    return await overLimit(visitor, counter ? { counter } : {});
  } catch {
    return overLimit(visitor, { counter: memoryCounter() });
  }
}

/** Builds the answer from the model's JSON: only what the schema allows,
    and files derived from that alone. */
export function answerFrom(modelJson: unknown): ChangeAnswer {
  const changes = parseChanges(modelJson);
  const reply = parseReply((modelJson as { reply?: unknown })?.reply);
  const answer: ChangeAnswer = { changes, files: settingsFiles(changes) };
  const said = changes.foldHeadings ? HEADINGS_ALREADY_FOLD : (reply ?? (hasChanges(changes) ? undefined : NOTHING_TO_CHANGE));
  return said ? { ...answer, reply: said } : answer;
}

/** Turns a visitor's plain-words request into changes to the demo window.
    The request's text is never logged or stored. */
export async function handleChange(request: Request, deps: Deps = {}): Promise<Response> {
  const read = await readRequest(request);
  if ("error" in read) return failure(read.error);
  const config = deps.config === undefined ? modelConfigFromEnv() : deps.config;
  if (!config) return failure("no-key");
  if (await limited(request, deps.counter)) return failure("rate-limited");
  try {
    const modelJson = await askModel(config, read.wanted, deps.fetcher);
    return Response.json(answerFrom(modelJson));
  } catch (error) {
    return failure(error instanceof ModelError ? error.code : "provider-failed");
  }
}
