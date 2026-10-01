import type { Redis } from "@upstash/redis";
import type { ChangeAnswer } from "./changeAnswer";
import type { ConfigPatch } from "./configPatch";
import { redisFromEnv } from "./redis";

const LOG_KEY = "demo:requests";
/** The newest requests kept; older ones fall off the end. */
const KEEP = 5000;

/** One request to the Change anything demo: what was typed and what came
    of it, with no address or anything else that identifies who typed it. */
export type LoggedRequest = { at: string; request: string; outcome: string; ms: number };

const PATCH_PARTS: { [Part in keyof ConfigPatch]-?: (value: NonNullable<ConfigPatch[Part]>) => string[] } = {
  theme: (tokens) => Object.keys(tokens),
  settings: (settings) => Object.keys(settings),
  toolbars: (toolbars) => Object.keys(toolbars).map((id) => `toolbar ${id}`),
  timing: (timing) => Object.keys(timing).map((key) => `timing ${key}`),
  keys: (bindings) => bindings.map((binding) => `${binding.keys} ${binding.command}`),
  replacements: (added) => added.map((each) => `${each.from} → ${each.to}`),
};

/** What an answer changed, as a short line such as
    "color.accent, appearance.theme". */
export function outcomeOfAnswer(answer: ChangeAnswer): string {
  const changed = Object.entries(answer.patch).flatMap(([part, value]) =>
    value ? (PATCH_PARTS[part as keyof ConfigPatch] as (input: unknown) => string[])(value) : [],
  );
  const parts = [
    answer.builtin ? `built-in ${answer.builtin}` : "",
    changed.join(", "),
    answer.reply ? `“${answer.reply}”` : "",
  ];
  return parts.filter(Boolean).join(" · ") || "nothing";
}

/** Adds a request to the log, quietly doing nothing without a store. */
export async function logRequest(entry: LoggedRequest, redis: Redis | null = redisFromEnv()): Promise<void> {
  if (!redis) return;
  try {
    await redis.lpush(LOG_KEY, JSON.stringify(entry));
    await redis.ltrim(LOG_KEY, 0, KEEP - 1);
  } catch {
    // A request is answered whether or not the log takes it.
  }
}

function asLoggedRequest(item: unknown): LoggedRequest | null {
  const value = typeof item === "string" ? JSON.parse(item) : item;
  return value && typeof value === "object" && "request" in value ? (value as LoggedRequest) : null;
}

/** The newest `count` requests, newest first, or null without a store. */
export async function recentRequests(count: number, redis: Redis | null = redisFromEnv()): Promise<LoggedRequest[] | null> {
  if (!redis) return null;
  const items = await redis.lrange(LOG_KEY, 0, count - 1);
  return items.map(asLoggedRequest).filter((entry): entry is LoggedRequest => entry !== null);
}
