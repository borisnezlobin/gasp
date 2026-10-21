import { Fragment } from "react";

/** A step's text with the app's own labels, written **like this**, in bold. */
function withLabels(step: string) {
  return step.split(/\*\*(.+?)\*\*/).map((part, index) =>
    index % 2 === 1 ? (
      <strong key={index} className="font-bold text-ink">
        {part}
      </strong>
    ) : (
      <Fragment key={index}>{part}</Fragment>
    ),
  );
}

/** Things to do in order, each numbered so you can keep your place. */
export function Steps({ steps }: { steps: string[] }) {
  return (
    <ol className="measure space-y-4">
      {steps.map((step, index) => (
        <li key={step} className="grid grid-cols-[2rem_1fr] gap-3">
          <span aria-hidden className="figure subheading text-ink-muted">
            {index + 1}
          </span>
          <span className="body text-ink-soft">{withLabels(step)}</span>
        </li>
      ))}
    </ol>
  );
}
