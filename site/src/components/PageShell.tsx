import type { ReactNode } from "react";
import { LiveText } from "./glyphs/LiveText";
import { SiteFooter } from "./SiteFooter";
import { SiteHeader } from "./SiteHeader";

const WIDTHS = { prose: "max-w-3xl", medium: "max-w-4xl", wide: "max-w-5xl" };

/** A page other than the home page: the header, its content at a reading
    width, and the footer. */
export function PageShell({ width = "prose", children }: { width?: keyof typeof WIDTHS; children: ReactNode }) {
  return (
    <>
      <SiteHeader />
      <main className={`mx-auto ${WIDTHS[width]} px-4 pt-12 pb-20 sm:px-8 lg:pt-20`}>{children}</main>
      <SiteFooter />
    </>
  );
}

/** The page's name, and the sentence under it that says what it holds. */
export function PageTitle({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <>
      <h1 className="heading">
        <LiveText text={title} />
      </h1>
      {children && <div className="lede measure mt-6 text-ink-soft">{children}</div>}
    </>
  );
}

/** One titled part of a page of reading. */
export function Part({ id, title, children }: { id?: string; title: string; children: ReactNode }) {
  return (
    <section id={id} className="mt-16 scroll-mt-8">
      <h2 className="subheading text-2xl">
        <LiveText text={title} />
      </h2>
      <div className="body mt-4 space-y-4 text-ink-soft">{children}</div>
    </section>
  );
}

/** One of the big parts of a long page, named in a heading as large as
    the home page's, with an optional line that adds a fact. */
export function Chapter({ id, title, intro, children }: { id: string; title: string; intro?: ReactNode; children: ReactNode }) {
  return (
    <section aria-labelledby={`${id}-title`} className="mt-24 scroll-mt-8">
      <h2 id={`${id}-title`} className="heading">
        <LiveText text={title} />
      </h2>
      {intro && <div className="lede measure mt-4 text-ink-soft">{intro}</div>}
      <div className="mt-10">{children}</div>
    </section>
  );
}

/** A command or file to copy, set as code. Long lines scroll, or wrap
    where the exact line breaks don't matter. */
export function CodeBlock({ children, wrap = false }: { children: string; wrap?: boolean }) {
  return (
    <pre className={`code rounded-xl bg-fill p-5 leading-relaxed text-ink ${wrap ? "break-all whitespace-pre-wrap" : "overflow-x-auto"}`}>
      {children}
    </pre>
  );
}
