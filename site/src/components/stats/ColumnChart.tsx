"use client";

import { useState } from "react";

export type Series = { name: string; color: string; values: number[] };

type ColumnChartProps = {
  title: string;
  days: string[];
  series: Series[];
};

const TICK_COUNT = 4;

function niceMax(value: number): number {
  if (value <= TICK_COUNT) return TICK_COUNT;
  const magnitude = 10 ** Math.floor(Math.log10(value));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * magnitude).find((m) => m * TICK_COUNT >= value) ?? value;
  return step * TICK_COUNT;
}

const shortDay = (day: string) =>
  new Date(`${day}T00:00:00Z`).toLocaleDateString("en", { month: "short", day: "numeric", timeZone: "UTC" });

function Legend({ series }: { series: Series[] }) {
  if (series.length < 2) return null;
  return (
    <ul className="small flex gap-5 text-ink-soft">
      {series.map((each) => (
        <li key={each.name} className="flex items-center gap-2">
          <span className="size-2.5 rounded-full" style={{ background: each.color }} aria-hidden />
          {each.name}
        </li>
      ))}
    </ul>
  );
}

/** Stacked daily columns with a hover tooltip, and the same numbers as a
    table under a disclosure. */
export function ColumnChart({ title, days, series }: ColumnChartProps) {
  const [hovered, setHovered] = useState<number | null>(null);
  const totals = days.map((_, day) => series.reduce((sum, each) => sum + each.values[day], 0));
  const top = niceMax(Math.max(0, ...totals));
  const ticks = Array.from({ length: TICK_COUNT + 1 }, (_, index) => (top / TICK_COUNT) * index).reverse();

  return (
    <figure className="rounded-2xl bg-surface p-5 shadow-lifted sm:p-6">
      <figcaption className="flex flex-wrap items-baseline justify-between gap-3">
        <span className="subheading">{title}</span>
        <Legend series={series} />
      </figcaption>

      <div className="relative mt-6 grid grid-cols-[2.5rem_minmax(0,1fr)]" aria-hidden>
        <div className="small figure relative h-48 text-right text-ink-muted">
          {ticks.map((tick) => (
            <span
              key={tick}
              className="absolute right-0 -translate-y-1/2 pr-2 leading-none"
              style={{ top: `${(1 - tick / top) * 100}%` }}
            >
              {tick.toLocaleString("en")}
            </span>
          ))}
        </div>
        <div className="relative h-48">
          {ticks.map((tick) => (
            <div
              key={tick}
              className="absolute inset-x-0 h-px bg-rule"
              style={{ top: `${(1 - tick / top) * 100}%` }}
            />
          ))}
          <div className="absolute inset-0 flex items-end gap-[2px]" onPointerLeave={() => setHovered(null)}>
            {days.map((day, index) => (
              <div
                key={day}
                className="flex h-full min-w-0 flex-1 cursor-default flex-col justify-end"
                onPointerEnter={() => setHovered(index)}
              >
                <div className="mx-auto flex w-full max-w-6 flex-col-reverse gap-[2px]">
                  {series.filter((each) => each.values[index] > 0).map((each) => (
                    <div
                      key={each.name}
                      className="w-full last:rounded-t-[4px]"
                      style={{
                        height: `${(each.values[index] / top) * 12}rem`,
                        background: each.color,
                        opacity: hovered === null || hovered === index ? 1 : 0.45,
                      }}
                    />
                  ))}
                </div>
              </div>
            ))}
          </div>
          {hovered !== null && (
            <div
              className="small pointer-events-none absolute -top-2 z-10 -translate-x-1/2 -translate-y-full rounded-lg bg-button px-3 py-2 whitespace-nowrap text-on-button shadow-lifted"
              style={{ left: `${((hovered + 0.5) / days.length) * 100}%` }}
            >
              <p className="font-bold">{shortDay(days[hovered])}</p>
              {series.map((each) => (
                <p key={each.name} className="figure">
                  {each.name}: {each.values[hovered].toLocaleString("en")}
                </p>
              ))}
            </div>
          )}
        </div>
        <div />
        <div className="small figure mt-2 flex justify-between text-ink-muted">
          <span>{shortDay(days[0])}</span>
          <span>{shortDay(days[days.length - 1])}</span>
        </div>
      </div>

      <details className="small mt-4 text-ink-soft">
        <summary className="cursor-pointer">Show as a table</summary>
        <table className="figure mt-3 w-full text-left">
          <thead>
            <tr>
              <th className="py-1 font-bold">Day</th>
              {series.map((each) => (
                <th key={each.name} className="py-1 text-right font-bold">
                  {each.name}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {days.map((day, index) => (
              <tr key={day} className="border-t border-rule">
                <td className="py-1">{shortDay(day)}</td>
                {series.map((each) => (
                  <td key={each.name} className="py-1 text-right">
                    {each.values[index].toLocaleString("en")}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
      </details>
    </figure>
  );
}
