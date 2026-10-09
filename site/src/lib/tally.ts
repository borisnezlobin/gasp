import { redisFromEnv } from "./redis";
import { fieldPing, pingField, type Ping } from "./ping";
import { INSTALLERS, type Installer } from "./releases";

/** Days a day's tally is kept before Redis drops it. */
const KEEP_SECONDS = 60 * 60 * 24 * 400;

const pingsKey = (day: string) => `pings:${day}`;
const downloadsKey = (day: string) => `downloads:${day}`;
/** The same downloads by installer, kept since Linux joined the Mac. */
const downloadsByKey = (day: string) => `downloads-by:${day}`;

const store = redisFromEnv;

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

export async function countDownload(installer: Installer, now: Date = new Date()): Promise<void> {
  const redis = store();
  if (!redis) return;
  const day = dayOf(now);
  const total = downloadsKey(day);
  const by = downloadsByKey(day);
  await redis
    .pipeline()
    .incr(total)
    .expire(total, KEEP_SECONDS)
    .hincrby(by, installer, 1)
    .expire(by, KEEP_SECONDS)
    .exec();
}

export type PingCount = Ping & { count: number };

export type DayTally = {
  day: string;
  pings: PingCount[];
  downloads: number;
  /** Downloads by installer; days before Linux count only in `downloads`. */
  downloadsBy: Record<Installer, number>;
};

function downloadCounts(hash: Record<string, unknown> | null): Record<Installer, number> {
  return Object.fromEntries(INSTALLERS.map((installer) => [installer, Number(hash?.[installer]) || 0])) as Record<
    Installer,
    number
  >;
}

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
    pipeline.hgetall(downloadsByKey(day));
  }
  const results = await pipeline.exec();
  return days.map((day, index) => ({
    day,
    pings: pingCounts(results[index * 3] as Record<string, unknown> | null),
    downloads: Number(results[index * 3 + 1]) || 0,
    downloadsBy: downloadCounts(results[index * 3 + 2] as Record<string, unknown> | null),
  }));
}
