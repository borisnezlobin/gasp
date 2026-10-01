import { AskToChange } from "@/components/AskToChange";
import { BreachHero } from "@/components/BreachHero";
import { DownloadButton } from "@/components/DownloadButton";
import { FeatureGrid } from "@/components/FeatureGrid";
import { LaunchRace } from "@/components/LaunchRace";
import { IdleTrace, KeystrokeSpeed } from "@/components/LightFigures";
import { LineField } from "@/components/LineField";
import { LiveEditor } from "@/components/LiveEditor";
import { ScreenshotTabs } from "@/components/ScreenshotTabs";
import { Section } from "@/components/Section";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteHeader } from "@/components/SiteHeader";

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
    </figure>
  );
}

export default function Home() {
  return (
    <>
      <SiteHeader />
      <main>
        <BreachHero>
          <DownloadButton size="large" />
        </BreachHero>

        <Section id="speed" title="Ready to edit in under half a second">
          <LaunchRace />
        </Section>

        <Section id="light" title="Quick in huge notes">
          <LineField />
          <div className="mt-8">
            <KeystrokeSpeed />
          </div>
          <div className="mt-16">
            <IdleTrace />
          </div>
        </Section>

        <Section
          id="formatting"
          title="Formats as you type"
          layout="side"
          intro="Gasp uses Markdown, an open format. You're never tied to Gasp."
        >
          <LiveEditor />
        </Section>

        <Section id="screens" title="The whole app">
          <ScreenshotTabs />
        </Section>

        <Section
          id="customization"
          title="Change anything"
          intro="Ask in plain words, and a model rewrites Gasp's settings while the window below changes to match."
        >
          <AskToChange />
        </Section>

        <Section id="features" title="Everything else">
          <FeatureGrid />
        </Section>

        <FriendQuote />

        <section aria-labelledby="try-title" className="mx-auto max-w-7xl px-4 pt-12 sm:px-8">
          <h2 id="try-title" className="heading">
            Try Gasp
          </h2>
          <div className="mt-8">
            <DownloadButton size="large" />
          </div>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
