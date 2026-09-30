const WIDTH = 600;
const HEIGHT = 60;

/** A flat processor trace scrolling slowly past: what Activity Monitor
    shows for Gasp while you read. The small bumps are keystrokes. */
function tracePoints(): string {
  const bumps = new Map([
    [9, 7],
    [10, 4],
    [31, 5],
    [46, 8],
    [47, 3],
  ]);
  return Array.from({ length: 61 }, (_, step) => {
    const lift = bumps.get(step) ?? 0;
    return `${step * 10},${HEIGHT - 2 - lift}`;
  }).join(" ");
}

const POINTS = tracePoints();

export function IdleTrace() {
  return (
    <div className="flex items-center gap-6">
      <div className="relative h-15 min-w-0 flex-1 overflow-hidden" aria-hidden>
        <div className="absolute inset-y-0 left-0 flex w-[200%] motion-safe:animate-[trace_24s_linear_infinite]">
          {[0, 1].map((copy) => (
            <svg key={copy} viewBox={`0 0 ${WIDTH} ${HEIGHT}`} preserveAspectRatio="none" className="h-full w-1/2">
              <polyline
                points={POINTS}
                fill="none"
                stroke="currentColor"
                strokeWidth="2"
                strokeLinejoin="round"
                vectorEffect="non-scaling-stroke"
                className="text-ink"
              />
            </svg>
          ))}
        </div>
        <div className="absolute inset-x-0 top-0 h-px bg-rule" />
      </div>
      <p className="shrink-0 text-right">
        <span className="figure block text-2xl font-bold sm:text-3xl">~0% CPU</span>
        <span className="small text-ink-muted">while you read</span>
      </p>
    </div>
  );
}
