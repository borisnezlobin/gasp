import { Redis } from "@upstash/redis";
import { fieldPing, pingField, type Ping } from "./ping";

/** Days a day's tally is kept before Redis drops it. */
const KEEP_SECONDS = 60 * 60 * 24 * 400;

const pingsKey = (day: string) => `pings:${day}`;
const downloadsKey = (day: string) => `downloads:${day}`;

/** Redis from Vercel's Upstash integration, under either of the names it
    sets, or null when no store is connected. */
function store(): Redis | null {
  const url = process.env.KV_REST_API_URL ?? process.env.UPSTASH_REDIS_REST_URL;
  const token = process.env.KV_REST_API_TOKEN ?? process.env.UPSTASH_REDIS_REST_TOKEN;
  if (!url || !token) return null;
  return new Redis({ url, token });
}

export function hasStore(): boolean {
  return store() !== null;
}

/** The UTC day, such as `2026-09-30`. */
export function dayOf(date: Date): string {
  return date.toISOString().slice(0, 10);
}

export function lastDays(count: number, until: Date = new Date()): string[] {
  const day = 24 * 60 * 60 * 1000;
  return Array.from({ length: count }, (_, index) =>
    dayOf(new Date(until.getTime() - (count - 1 - index) * day)),
  );
}

export async function countPing(ping: Ping, now: Date = new Date()): Promise<void> {
  const redis = store();
  if (!redis) return;
  const key = pingsKey(dayOf(now));
  await redis.pipeline().hincrby(key, pingField(ping), 1).expire(key, KEEP_SECONDS).exec();
}

export async function countDownload(now: Date = new Date()): Promise<void> {
  const redis = store();
  if (!redis) return;
  const key = downloadsKey(dayOf(now));
  await redis.pipeline().incr(key).expire(key, KEEP_SECONDS).exec();
}

export type PingCount = Ping & { count: number };

export type DayTally = {
  day: string;
  pings: PingCount[];
  downloads: number;
};

function pingCounts(hash: Record<string, unknown> | null): PingCount[] {
  return Object.entries(hash ?? {}).flatMap(([field, count]) => {
    const ping = fieldPing(field);
    return ping ? [{ ...ping, count: Number(count) || 0 }] : [];
  });
}

/** Each of `days`, with its pings and downloads, or null with no store. */
export async function readTallies(days: string[]): Promise<DayTally[] | null> {
  const redis = store();
  if (!redis) return null;
  const pipeline = redis.pipeline();
  for (const day of days) {
    pipeline.hgetall(pingsKey(day));
    pipeline.get(downloadsKey(day));
  }
  const results = await pipeline.exec();
  return days.map((day, index) => ({
    day,
    pings: pingCounts(results[index * 2] as Record<string, unknown> | null),
    downloads: Number(results[index * 2 + 1]) || 0,
  }));
}
