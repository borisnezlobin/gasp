import type { ReactNode } from "react";

type SectionProps = {
  id: string;
  title: string;
  intro?: ReactNode;
  children: ReactNode;
  layout?: "stacked" | "side";
};

const LAYOUTS = {
  stacked: { grid: "", heading: "max-w-2xl", body: "mt-10" },
  side: {
    grid: "lg:grid lg:grid-cols-[minmax(0,22rem)_minmax(0,1fr)] lg:gap-16",
    heading: "",
    body: "mt-10 lg:mt-0",
  },
};

/** One part of a page: a heading naming it, an optional line that adds a
    fact, and its content, beside the heading on wide screens or under it. */
export function Section({ id, title, intro, children, layout = "stacked" }: SectionProps) {
  const style = LAYOUTS[layout];
  return (
    <section aria-labelledby={`${id}-title`} className="mx-auto max-w-7xl px-4 py-20 sm:px-8 lg:py-28">
      <div className={style.grid}>
        <div className={style.heading}>
          <h2 id={`${id}-title`} className="heading">
            {title}
          </h2>
          {intro && <div className="lede mt-4 text-ink-soft">{intro}</div>}
        </div>
        <div className={style.body}>{children}</div>
      </div>
    </section>
  );
}
