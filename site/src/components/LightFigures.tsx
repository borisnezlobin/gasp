/** From PLAN.md's performance tables, measured on an M2 Pro. */
const KEYSTROKE_MS = 2.2;
const FRAME_MS = 1000 / 120;

const share = (ms: number) => `${(ms / FRAME_MS) * 100}%`;

/** One frame at 120 Hz, with the part a keystroke in the note above uses. */
export function KeystrokeFrame() {
  return (
    <div>
      <div
        role="img"
        aria-label={`A keystroke takes ${KEYSTROKE_MS} ms of an 8.3 ms frame.`}
        className="relative h-4 overflow-hidden rounded-full bg-fill"
      >
        <div className="absolute inset-y-0 left-0 rounded-full bg-caret" style={{ width: share(KEYSTROKE_MS) }} />
      </div>
      <div className="mt-3 flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1">
        <p>
          <span className="figure text-2xl font-bold sm:text-3xl">{KEYSTROKE_MS} ms</span>{" "}
          <span className="text-ink-soft">to show each keystroke in it</span>
        </p>
        <p className="small figure text-ink-muted">One frame at 120 Hz is 8.3 ms</p>
      </div>
    </div>
  );
}

type Figure = { value: string; label: string };

const IDLE: Figure[] = [
  { value: "About 60 MB", label: "of memory with a vault open" },
  { value: "Under 1%", label: "CPU while you read" },
];

export function IdleFigures() {
  return (
    <dl className="grid gap-8 sm:grid-cols-2">
      {IDLE.map((figure) => (
        <div key={figure.value} className="flex flex-col-reverse">
          <dt className="mt-1 text-ink-soft">{figure.label}</dt>
          <dd className="figure text-2xl font-bold sm:text-3xl">{figure.value}</dd>
        </div>
      ))}
    </dl>
  );
}
