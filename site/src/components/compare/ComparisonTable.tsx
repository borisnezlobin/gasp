import type { Cell, Row } from "@/lib/comparison";
import { COVERAGE_TONE, CoverageMark } from "../CoverageMark";

function Mark({ cell }: { cell: Cell }) {
  return (
    <div role="cell" className={`flex gap-2.5 ${COVERAGE_TONE[cell.coverage]}`}>
      <CoverageMark coverage={cell.coverage} />
      <span>{cell.text}</span>
    </div>
  );
}

const ROW_GRID = "grid grid-cols-2 gap-x-6 gap-y-2 sm:grid-cols-[minmax(0,11rem)_1fr_1fr]";

/** Both apps row by row, each answer marked with a check, a half circle or
    a dash, so the table reads at a glance before a word of it. On a phone
    each topic sits over its two answers. */
export function ComparisonTable({ rival, rows }: { rival: string; rows: Row[] }) {
  return (
    <div role="table" aria-label={`Gasp and ${rival}, side by side`} className="body">
      <div role="rowgroup">
        <div role="row" className={`${ROW_GRID} pb-3`}>
          <span role="columnheader" className="max-sm:hidden">
            <span className="sr-only">Topic</span>
          </span>
          <span role="columnheader" className="subheading">
            Gasp
          </span>
          <span role="columnheader" className="subheading">
            {rival}
          </span>
        </div>
      </div>
      <div role="rowgroup" className="divide-y divide-rule border-t border-rule">
        {rows.map((row) => (
          <div role="row" key={row.topic} className={`${ROW_GRID} py-4`}>
            <span role="rowheader" className="font-bold max-sm:col-span-2">
              {row.topic}
            </span>
            <Mark cell={row.gasp} />
            <Mark cell={row.rival} />
          </div>
        ))}
      </div>
    </div>
  );
}
