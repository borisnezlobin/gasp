import { createHash } from "node:crypto";
import type { Redis } from "@upstash/redis";
import { redisFromEnv } from "./redis";

type Window = { name: string; seconds: number };

const MINUTE: Window = { name: "minute", seconds: 60 };
const DAY: Window = { name: "day", seconds: 60 * 60 * 24 };

type Limit = { scope: "visitor" | "everyone"; window: Window; most: number };

const DEFAULT_DAILY_LIMIT = 900;

/** OpenRouter lets a free model take 20 requests a minute, and 1,000 a
    day once the account has bought credits, so everyone together stays
    under both. One visitor gets a handful a minute and a few dozen a day. */
export function demoLimits(dailyLimit = dailyLimitFromEnv()): Limit[] {
  return [
    { scope: "visitor", window: MINUTE, most: 6 },
    { scope: "visitor", window: DAY, most: 40 },
    { scope: "everyone", window: MINUTE, most: 18 },
    { scope: "everyone", window: DAY, most: dailyLimit },
  ];
}

function dailyLimitFromEnv(): number {
  const configured = Number(process.env.DEMO_AI_DAILY_LIMIT);
  return Number.isInteger(configured) && configured > 0 ? configured : DEFAULT_DAILY_LIMIT;
}


/** A visitor's address, hashed so no address is ever stored. */
export function visitorId(request: Request): string {
  const forwarded = request.headers.get("x-forwarded-for")?.split(",")[0]?.trim();
  const address = forwarded || request.headers.get("x-real-ip") || "unknown";
  return createHash("sha256").update(`gasp-demo:${address}`).digest("hex").slice(0, 24);
}

function counterKey(limit: Limit, visitor: string, now: number): string {
  const bucket = Math.floor(now / 1000 / limit.window.seconds);
  const who = limit.scope === "visitor" ? visitor : "all";
  return `change:${who}:${limit.window.name}:${bucket}`;
}

export type Counter = (keys: { key: string; seconds: number }[]) => Promise<number[]>;

function redisCounter(redis: Redis): Counter {
  return async (keys) => {
    const pipeline = redis.pipeline();
    for (const { key, seconds } of keys) pipeline.incr(key).expire(key, seconds);
    const results = await pipeline.exec();
    return keys.map((_, index) => Number(results[index * 2]) || 0);
  };
}

type MemoryCount = { count: number; expires: number };
const memoryCounts = new Map<string, MemoryCount>();

function forgetExpired(now: number) {
  for (const [key, entry] of memoryCounts) if (entry.expires <= now) memoryCounts.delete(key);
}

/** Best effort without Redis: each server instance counts on its own. */
export function memoryCounter(clock: () => number = Date.now): Counter {
  return async (keys) => {
    const now = clock();
    forgetExpired(now);
    return keys.map(({ key, seconds }) => {
      const entry = memoryCounts.get(key) ?? { count: 0, expires: now + seconds * 1000 };
      entry.count += 1;
      memoryCounts.set(key, entry);
      return entry.count;
    });
  };
}

export function resetMemoryCounts() {
  memoryCounts.clear();
}

function defaultCounter(): Counter {
  const redis = redisFromEnv();
  return redis ? redisCounter(redis) : memoryCounter();
}

/** Counts this request against every limit, and says whether any is over. */
export async function overLimit(
  visitor: string,
  { counter = defaultCounter(), limits = demoLimits(), now = Date.now() } = {},
): Promise<boolean> {
  const keys = limits.map((limit) => ({ key: counterKey(limit, visitor, now), seconds: limit.window.seconds * 2 }));
  const counts = await counter(keys);
  return counts.some((count, index) => count > limits[index].most);
}
