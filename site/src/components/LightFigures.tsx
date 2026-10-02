import { LiveText } from "./glyphs/LiveText";

/** From PLAN.md's performance tables, measured on an M2 Pro: a keystroke in
    a 48,000-line note reaches the screen in 2.2 ms. */
const KEYSTROKE_MS = 2.2;
const FRAMES_PER_SECOND = Math.round(1000 / KEYSTROKE_MS);

export function KeystrokeSpeed() {
  return (
    <p className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
      <span className="figure text-2xl font-bold sm:text-3xl">
        <LiveText text={`${FRAMES_PER_SECOND} fps`} />
      </span>
      <span className="text-ink-soft">Typing never lags. Gasp keeps up at that rate even in a note this long.</span>
    </p>
  );
}

const GRID_LINES = 12;

/** Gasp's processor use while a note sits open, drawn like a monitor
    sweeping across: the pen keeps moving and the line stays on the floor.
    Measured at 0.9% on a Mac, which is flat at this scale. */
export function IdleTrace() {
  return (
    <div className="flex items-end gap-6">
      <div aria-hidden className="relative h-16 min-w-0 flex-1 overflow-hidden rounded-lg bg-fill">
        <div className="absolute inset-0 flex">
          {Array.from({ length: GRID_LINES }, (_, line) => (
            <span key={line} className="h-full flex-1 border-l border-ink/5 first:border-l-0" />
          ))}
        </div>
        <div className="absolute bottom-2 left-0 h-0.5 w-full rounded-full bg-ink/15" />
        <div className="absolute bottom-2 left-0 h-0.5 w-full rounded-full bg-ink motion-safe:animate-sweep">
          <span className="absolute -top-0.5 right-0 size-1.5 translate-x-1/2 rounded-full bg-ink motion-reduce:hidden" />
        </div>
      </div>
      <p className="shrink-0 pb-1 text-ink-soft">
        <span className="sr-only">Gasp&apos;s </span>CPU while you read
      </p>
    </div>
  );
}
