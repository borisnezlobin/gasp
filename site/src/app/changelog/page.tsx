import type { Metadata } from "next";
import { ArrowUpRight } from "@phosphor-icons/react/dist/ssr";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteHeader } from "@/components/SiteHeader";
import { LiveText } from "@/components/glyphs/LiveText";
import { changelogEntries, gitHubReleases, type ChangelogEntry } from "@/lib/releases";
import { parseReleaseNotes, type NoteBlock, type NoteSpan } from "@/lib/releaseNotes";
import { RELEASES_URL } from "@/lib/site";

export const metadata: Metadata = {
  title: "Changelog",
  description: "What changed in each version of Gasp.",
};

/** Ten minutes, like the update check, so a new release shows up soon
    after it's published. */
export const revalidate = 600;

const linkStyle = "underline decoration-ink-muted underline-offset-4 hover:decoration-ink";

const releaseDate = new Intl.DateTimeFormat("en-US", { dateStyle: "long", timeZone: "UTC" });

function Span({ span }: { span: NoteSpan }) {
  if (span.kind === "link") {
    return (
      <a href={span.href} className={linkStyle}>
        {span.text}
      </a>
    );
  }
  if (span.kind === "strong") return <strong className="text-ink">{span.text}</strong>;
  if (span.kind === "code") return <span className="code">{span.text}</span>;
  return <>{span.text}</>;
}

function Spans({ spans }: { spans: NoteSpan[] }) {
  return spans.map((span, index) => <Span key={index} span={span} />);
}

function Block({ block }: { block: NoteBlock }) {
  if (block.kind === "heading") {
    return (
      <h3 className="subheading pt-4 text-ink">
        <Spans spans={block.spans} />
      </h3>
    );
  }
  if (block.kind === "list") {
    return (
      <ul className="list-disc space-y-2 pl-5 marker:text-ink-muted">
        {block.items.map((item, index) => (
          <li key={index}>
            <Spans spans={item} />
          </li>
        ))}
      </ul>
    );
  }
  return (
    <p>
      <Spans spans={block.spans} />
    </p>
  );
}

function Release({ entry }: { entry: ChangelogEntry }) {
  const blocks = parseReleaseNotes(entry.notes);
  return (
    <li id={`v${entry.version}`} className="grid scroll-mt-8 gap-x-12 gap-y-4 md:grid-cols-[9rem_1fr]">
      <div className="self-start md:sticky md:top-8">
        <h2 className="figure text-3xl leading-tight font-bold">
          <LiveText text={entry.version} />
        </h2>
        <p className="small mt-1 text-ink-muted">
          <time dateTime={entry.publishedAt}>{releaseDate.format(new Date(entry.publishedAt))}</time>
          {entry.prerelease && <span className="block">Early preview</span>}
        </p>
      </div>
      <div className="body space-y-4 text-ink-soft">
        {blocks.length > 0 ? blocks.map((block, index) => <Block key={index} block={block} />) : <p>No notes for this one.</p>}
        <p className="small pt-2">
          <a href={entry.url} className={`inline-flex items-center gap-1 text-ink-muted ${linkStyle}`}>
            Files on GitHub
            <ArrowUpRight size={14} aria-hidden />
          </a>
        </p>
      </div>
    </li>
  );
}

function Unreachable() {
  return (
    <p className="body mt-12 text-ink-soft">
      GitHub didn&apos;t answer just now, so the list couldn&apos;t load. Every version&apos;s notes are also on{" "}
      <a href={RELEASES_URL} className={linkStyle}>
        GitHub&apos;s releases page
      </a>
      .
    </p>
  );
}

export default async function Changelog() {
  const releases = await gitHubReleases(revalidate);
  const entries = releases === null ? [] : changelogEntries(releases);
  return (
    <>
      <SiteHeader />
      <main className="mx-auto max-w-4xl px-4 pt-12 pb-20 sm:px-8 lg:pt-20">
        <h1 className="heading">
          <LiveText text="Changelog" />
        </h1>
        {entries.length === 0 ? (
          <Unreachable />
        ) : (
          <ol className="mt-12 space-y-20 lg:mt-16">
            {entries.map((entry) => (
              <Release key={entry.version} entry={entry} />
            ))}
          </ol>
        )}
      </main>
      <SiteFooter />
    </>
  );
}
