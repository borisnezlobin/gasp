import { timingSafeEqual } from "node:crypto";
import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { ColumnChart } from "@/components/stats/ColumnChart";
import { publishedReleases, type Release } from "@/lib/releases";
import { ARCH_NAMES, PLATFORM_NAMES, breakdown, installsOn, type Breakdown } from "@/lib/stats";
import { recentRequests, type LoggedRequest } from "@/lib/requestLog";
import { lastDays, readTallies, type DayTally } from "@/lib/tally";

export const dynamic = "force-dynamic";
export const metadata: Metadata = { title: "Stats", robots: { index: false, follow: false } };

const DAYS = 30;

function tokenMatches(given: string | string[] | undefined): boolean {
  const expected = process.env.STATS_TOKEN;
  if (!expected || typeof given !== "string") return false;
  const a = Buffer.from(given);
  const b = Buffer.from(expected);
  return a.length === b.length && timingSafeEqual(a, b);
}

function Tile({ label, value, detail }: { label: string; value: number; detail: string }) {
  return (
    <div className="rounded-2xl bg-surface p-5 shadow-lifted">
      <p className="small text-ink-soft">{label}</p>
      <p className="figure mt-1 text-4xl font-bold">{value.toLocaleString("en")}</p>
      <p className="small mt-1 text-ink-muted">{detail}</p>
    </div>
  );
}

function BreakdownTable({ title, rows, latestLabel }: { title: string; rows: Breakdown[]; latestLabel: string }) {
  const most = Math.max(1, ...rows.map((row) => row.total));
  return (
    <section className="rounded-2xl bg-surface p-5 shadow-lifted sm:p-6">
      <h2 className="subheading">{title}</h2>
      <table className="small figure mt-4 w-full text-left">
        <thead className="text-ink-muted">
          <tr>
            <th className="py-1.5 font-normal">Name</th>
            <th className="py-1.5 text-right font-normal">{latestLabel}</th>
            <th className="py-1.5 pl-4 text-right font-normal">Pings, {DAYS} days</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((row) => (
            <tr key={row.name} className="border-t border-rule">
              <td className="py-2">{row.name}</td>
              <td className="py-2 text-right">{row.latest.toLocaleString("en")}</td>
              <td className="py-2 pl-4">
                <div className="flex items-center justify-end gap-3">
                  <span className="h-2 rounded-r-[4px] bg-(--series-1)" style={{ width: `${(row.total / most) * 6}rem` }} />
                  <span className="w-12 text-right">{row.total.toLocaleString("en")}</span>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && <p className="small mt-3 text-ink-muted">No pings yet.</p>}
    </section>
  );
}

function ReleaseTable({ releases }: { releases: Release[] }) {
  return (
    <section className="rounded-2xl bg-surface p-5 shadow-lifted sm:p-6">
      <h2 className="subheading">GitHub downloads by release</h2>
      <table className="small figure mt-4 w-full text-left">
        <tbody>
          {releases.map((release) => (
            <tr key={release.tag} className="border-t border-rule first:border-t-0">
              <td className="py-2">{release.tag}</td>
              <td className="py-2 text-right">{release.downloads.toLocaleString("en")}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <p className="small mt-3 text-ink-muted">
        GitHub&apos;s own count of DMG downloads, including ones that didn&apos;t come through the site.
      </p>
    </section>
  );
}

function NoStore() {
  return (
    <p className="body rounded-2xl bg-surface p-6 text-ink-soft shadow-lifted">
      No store is connected, so nothing is being counted. Connect Upstash Redis to this project in Vercel&apos;s
      Storage tab, then redeploy.
    </p>
  );
}

function Tallies({ tallies, days }: { tallies: DayTally[]; days: string[] }) {
  const mac = tallies.map((tally) => installsOn(tally, "mac"));
  const ios = tallies.map((tally) => installsOn(tally, "ios"));
  const downloads = tallies.map((tally) => tally.downloads);
  const today = tallies.length - 1;
  return (
    <>
      <div className="grid gap-4 sm:grid-cols-2">
        <Tile label="Active installs today" value={mac[today] + ios[today]} detail={`${mac[today]} Mac, ${ios[today]} iPhone`} />
        <Tile
          label={`Downloads from the site, ${DAYS} days`}
          value={downloads.reduce((sum, count) => sum + count, 0)}
          detail={`${downloads[today]} today`}
        />
      </div>
      <ColumnChart
        title="Active installs per day"
        days={days}
        series={[
          { name: PLATFORM_NAMES.mac, color: "var(--series-1)", values: mac },
          { name: PLATFORM_NAMES.ios, color: "var(--series-2)", values: ios },
        ]}
      />
      <ColumnChart title="Downloads from the site per day" days={days} series={[{ name: "Downloads", color: "var(--series-1)", values: downloads }]} />
      <div className="grid gap-4 lg:grid-cols-2">
        <BreakdownTable title="Versions" latestLabel="Today" rows={breakdown(tallies, (ping) => `${PLATFORM_NAMES[ping.platform]} ${ping.version}`)} />
        <BreakdownTable title="Chips" latestLabel="Today" rows={breakdown(tallies, (ping) => ARCH_NAMES[ping.arch])} />
      </div>
    </>
  );
}

const REQUESTS_SHOWN = 300;

function RequestLog({ requests }: { requests: LoggedRequest[] }) {
  return (
    <section className="rounded-2xl bg-surface p-5 shadow-lifted sm:p-6">
      <h2 className="subheading">Change anything requests</h2>
      {requests.length === 0 ? (
        <p className="small mt-3 text-ink-muted">Nobody has asked for anything yet.</p>
      ) : (
        <ol className="mt-4 divide-y divide-rule">
          {requests.map((entry, index) => (
            <li key={`${entry.at}-${index}`} className="grid gap-1 py-3 sm:grid-cols-[9rem_minmax(0,1fr)] sm:gap-4">
              <time className="small figure text-ink-muted" dateTime={entry.at}>
                {entry.at.slice(5, 16).replace("T", " ")}
              </time>
              <div className="min-w-0">
                <p className="font-bold break-words">{entry.request}</p>
                <p className="small mt-0.5 break-words text-ink-muted">
                  {entry.outcome} <span className="figure">({(entry.ms / 1000).toFixed(1)} s)</span>
                </p>
              </div>
            </li>
          ))}
        </ol>
      )}
    </section>
  );
}

export default async function Stats({ searchParams }: { searchParams: Promise<Record<string, string | string[] | undefined>> }) {
  const { token } = await searchParams;
  if (!tokenMatches(token)) notFound();
  const days = lastDays(DAYS);
  const [tallies, releases, requests] = await Promise.all([
    readTallies(days),
    publishedReleases(),
    recentRequests(REQUESTS_SHOWN),
  ]);
  const githubTotal = releases.reduce((sum, release) => sum + release.downloads, 0);

  return (
    <main className="mx-auto flex max-w-5xl flex-col gap-6 px-4 py-12 sm:px-8">
      <div>
        <h1 className="heading">Gasp stats</h1>
        <p className="small mt-2 text-ink-muted">Days are UTC. An active install is one app that pinged that day.</p>
      </div>
      {tallies ? <Tallies tallies={tallies} days={days} /> : <NoStore />}
      <div className="grid gap-4 lg:grid-cols-2">
        <Tile label="GitHub downloads, all time" value={githubTotal} detail="Every DMG on every release" />
        <ReleaseTable releases={releases} />
      </div>
      {requests && <RequestLog requests={requests} />}
    </main>
  );
}
