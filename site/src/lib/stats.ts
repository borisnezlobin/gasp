import type { Platform } from "./ping";
import type { DayTally, PingCount } from "./tally";

export const PLATFORM_NAMES: Record<Platform, string> = { mac: "Mac", ios: "iPhone" };
export const ARCH_NAMES: Record<string, string> = { arm64: "Apple silicon", x86_64: "Intel" };

export function installsOn(tally: DayTally, platform: Platform): number {
  return tally.pings.filter((ping) => ping.platform === platform).reduce((sum, ping) => sum + ping.count, 0);
}

export type Breakdown = { name: string; latest: number; total: number };

/** Pings grouped by `keyOf`: on the last day, and over every day given. */
export function breakdown(tallies: DayTally[], keyOf: (ping: PingCount) => string): Breakdown[] {
  const rows = new Map<string, Breakdown>();
  const lastDay = tallies.at(-1)?.day;
  for (const tally of tallies) {
    for (const ping of tally.pings) {
      const name = keyOf(ping);
      const row = rows.get(name) ?? { name, latest: 0, total: 0 };
      row.total += ping.count;
      if (tally.day === lastDay) row.latest += ping.count;
      rows.set(name, row);
    }
  }
  return [...rows.values()].sort((a, b) => b.total - a.total);
}
