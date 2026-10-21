import type { Measure } from "@/lib/comparison";

function figure(value: number, unit: string) {
  return `${value.toLocaleString("en-US", { maximumFractionDigits: 2 })} ${unit}`;
}

function Bar({ name, value, most, unit, ours }: { name: string; value: number; most: number; unit: string; ours: boolean }) {
  return (
    <div className="grid grid-cols-[6.5rem_1fr] items-center gap-4 sm:grid-cols-[8rem_1fr]">
      <span className={ours ? "font-bold" : "text-ink-soft"}>{name}</span>
      <div className="flex items-center gap-3">
        <div
          aria-hidden
          className={`h-3 rounded-full ${ours ? "bg-ink" : "bg-ink-muted/50"}`}
          style={{ width: `max(0.75rem, ${(value / most) * 70}%)` }}
        />
        <span className={`figure shrink-0 font-bold ${ours ? "text-ink" : "text-ink-soft"}`}>{figure(value, unit)}</span>
      </div>
    </div>
  );
}

/** Figures measured on both apps, drawn as bars on one scale per figure. */
export function MeasuredBars({ rival, measures }: { rival: string; measures: Measure[] }) {
  return (
    <dl className="space-y-10">
      {measures.map((measure) => {
        const most = Math.max(measure.gasp, measure.rival);
        return (
          <div key={measure.name}>
            <dt className="subheading">{measure.name}</dt>
            <dd className="mt-4 space-y-3">
              <Bar name="Gasp" value={measure.gasp} most={most} unit={measure.unit} ours />
              <Bar name={rival} value={measure.rival} most={most} unit={measure.unit} ours={false} />
              <p className="body measure text-ink-muted">{measure.method}</p>
            </dd>
          </div>
        );
      })}
    </dl>
  );
}
