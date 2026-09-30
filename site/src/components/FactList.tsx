import type { ReactNode } from "react";

export type Fact = { term: ReactNode; detail: ReactNode };

/** Facts as a list of name and value, one per row. */
export function FactList({ facts, className = "" }: { facts: Fact[]; className?: string }) {
  return (
    <dl className={`divide-y divide-rule ${className}`}>
      {facts.map((fact, index) => (
        <div key={index} className="py-4 first:pt-0">
          <dt className="subheading figure">{fact.term}</dt>
          <dd className="body mt-1 text-ink-soft">{fact.detail}</dd>
        </div>
      ))}
    </dl>
  );
}
