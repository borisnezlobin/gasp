import type { Coverage } from "@/lib/comparison";
import { COVERAGE_TONE, CoverageMark } from "./CoverageMark";

export type Support = { sample: string; name: string; coverage: Coverage };

/** Syntax as you'd type it, beside what Gasp does with it. */
export function SupportList({ items }: { items: Support[] }) {
  return (
    <ul className="grid gap-x-10 sm:grid-cols-2">
      {items.map((item) => (
        <li key={item.name} className={`flex gap-3 border-b border-rule py-4 ${COVERAGE_TONE[item.coverage]}`}>
          <CoverageMark coverage={item.coverage} />
          <div className="min-w-0">
            <p className="body">{item.name}</p>
            <code className="code mt-1 block truncate text-ink-muted" title={item.sample}>
              {item.sample}
            </code>
          </div>
        </li>
      ))}
    </ul>
  );
}
