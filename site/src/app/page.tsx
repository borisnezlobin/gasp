import { BreachHero } from "@/components/BreachHero";
import { ConfigFiles } from "@/components/ConfigFiles";
import { DownloadButton } from "@/components/DownloadButton";
import { FeatureGrid } from "@/components/FeatureGrid";
import { IdleTrace } from "@/components/IdleTrace";
import { LaunchDemo } from "@/components/LaunchDemo";
import { LiveEditor } from "@/components/LiveEditor";
import { MemoryField } from "@/components/MemoryField";
import { ScreenshotTabs } from "@/components/ScreenshotTabs";
import { ScrollSwimmer } from "@/components/ScrollSwimmer";
import { Section } from "@/components/Section";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteHeader } from "@/components/SiteHeader";
import { SwimmingWhale } from "@/components/SwimmingWhale";

const REQUIREMENTS = "Free, for macOS 12 or later on Apple silicon and Intel.";

function DownloadRow() {
  return (
    <div className="flex flex-col items-start gap-3 sm:flex-row sm:items-center sm:gap-5">
      <DownloadButton size="large" />
      <p className="small text-ink-muted">{REQUIREMENTS}</p>
    </div>
  );
}

function FriendQuote() {
  return (
    <figure className="mx-auto max-w-7xl px-4 py-24 sm:px-8 lg:py-36">
      <blockquote>
        <p className="text-[clamp(3rem,11vw,9rem)] leading-[0.95] font-bold">
          “It’s crazy fast.”
          <span aria-hidden className="ml-[0.06em] inline-block h-[0.8em] w-[0.06em] translate-y-[0.08em] rounded-full bg-caret" />
        </p>
        <p className="lede mt-8 text-ink-soft">“Works very well, very smooth, very cool.”</p>
      </blockquote>
      <figcaption className="small mt-4 text-ink-muted">A friend who tried it</figcaption>
    </figure>
  );
}

export default function Home() {
  return (
    <>
      <SiteHeader />
      <main>
        <BreachHero>
          <DownloadRow />
        </BreachHero>

        <Section id="speed" title="Opens in 300 ms">
          <LaunchDemo />
        </Section>

        <Section id="light" title="Stays light">
          <MemoryField />
          <div className="mt-14">
            <IdleTrace />
          </div>
        </Section>

        <Section
          id="formatting"
          title="Formats as you type"
          layout="side"
          intro="Every note stays a plain .md file, so any other app can still open it."
        >
          <LiveEditor />
        </Section>

        <Section id="screens" title="The whole app">
          <ScreenshotTabs />
        </Section>

        <Section
          id="customization"
          title="Yours to change"
          layout="side"
          intro="Everything Gasp does is set in files in your vault. Ask Claude Code for a new shortcut or toolbar, and the app picks up the change the moment it's saved."
        >
          <ConfigFiles />
        </Section>

        <Section id="features" title="Everything else">
          <FeatureGrid />
        </Section>

        <FriendQuote />

        <section aria-labelledby="try-title" className="mx-auto max-w-7xl px-4 pt-12 pb-8 sm:px-8">
          <h2 id="try-title" className="heading">
            Try Gasp
          </h2>
          <div className="mt-8">
            <DownloadRow />
          </div>
          <div className="mt-12 md:hidden">
            <SwimmingWhale />
          </div>
        </section>
      </main>
      <SiteFooter />
      <ScrollSwimmer />
    </>
  );
}
