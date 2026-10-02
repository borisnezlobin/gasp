import type { Metadata } from "next";
import Link from "next/link";
import settingDark from "@/assets/setting-usage-dark.png";
import settingLight from "@/assets/setting-usage-light.png";
import { FactList, type Fact } from "@/components/FactList";
import { InkImage } from "@/components/InkImage";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteHeader } from "@/components/SiteHeader";
import { REPO_URL } from "@/lib/site";
import { LiveText } from "@/components/glyphs/LiveText";

export const metadata: Metadata = {
  title: "Privacy",
  description: "Exactly what Gasp sends, which is four small facts once a day, and how to turn it off.",
};

const PAYLOAD = `{
  "version": "0.1.0",
  "platform": "mac",
  "os": "15.1",
  "arch": "arm64"
}`;

const VERSION_ANSWER = `{
  "version": "0.2.0",
  "url": "https://github.com/borisnezlobin/gasp/releases/download/v0.2.0/Gasp-0.2.0.dmg",
  "notes": "https://github.com/borisnezlobin/gasp/releases/tag/v0.2.0",
  "published": "2026-10-01T09:00:00Z",
  "size": 96746643,
  "sha256": "51ca523e…"
}`;

const FIELDS: Fact[] = [
  { term: <span className="code">version</span>, detail: "Which version of Gasp you have." },
  { term: <span className="code">platform</span>, detail: "Mac or iPhone." },
  { term: <span className="code">os</span>, detail: "Your macOS or iOS version number." },
  { term: <span className="code">arch</span>, detail: "Apple silicon or Intel." },
];

const NEVER = [
  "Anything you've written, or the names of your notes, files and folders.",
  "Your name, email, GitHub account or anything else about you.",
  "An ID. Nothing links one day's ping to the next, so there's no way to follow one Mac over time.",
];

function Part({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-16">
      <h2 className="subheading text-2xl">
        <LiveText text={title} />
      </h2>
      <div className="body mt-4 space-y-4 text-ink-soft">{children}</div>
    </section>
  );
}

export default function Privacy() {
  return (
    <>
      <SiteHeader />
      <main className="mx-auto max-w-3xl px-4 pt-12 pb-20 sm:px-8 lg:pt-20">
        <h1 className="heading">
          <LiveText text="Privacy" />
        </h1>
        <p className="lede mt-6 text-ink-soft">
          Your notes live on your devices, and in your iCloud Drive or your own GitHub repository if you sync. We never see them. Gasp
          sends us one tiny message, at most once a day, so we can count how many people use it, and the Mac app asks us
          once a day whether a newer version is out. This page shows all of it.
        </p>

        <Part title="What the app sends">
          <p>Once a day, a little after Gasp opens, it sends exactly this to gaspmd.com:</p>
          <pre className="code overflow-x-auto rounded-xl bg-fill p-5 leading-relaxed text-ink">{PAYLOAD}</pre>
          <FactList facts={FIELDS} className="pt-2" />
        </Part>

        <Part title="What it never sends">
          <ul className="list-disc space-y-2 pl-5 marker:text-ink-muted">
            {NEVER.map((item) => (
              <li key={item}>{item}</li>
            ))}
          </ul>
        </Part>

        <Part title="What we keep">
          <p>
            Each ping adds one to a count for that day and those four values, such as twelve Macs on version 0.1.0,
            macOS 15.1 and Apple silicon. That count is all that&apos;s stored. We don&apos;t keep your IP address.
            Vercel, which runs this site, has to see it to answer any request, as every web host does, and we
            don&apos;t look it up or save it.
          </p>
          <p>
            The download button counts downloads the same way, as one number per day. The site has no cookies, no ads
            and no analytics scripts.
          </p>
        </Part>

        <Part title="Turning it off">
          <p>
            Open Settings in Gasp. On the Mac the switch is on the General page, under Agent access. On the iPhone
            it&apos;s in the Telemetry section.
          </p>
          <div className="overflow-hidden rounded-xl shadow-lifted">
            <InkImage
              light={settingLight}
              dark={settingDark}
              alt="The Send anonymous usage data switch in Gasp's settings, turned on."
              sizes="(min-width: 768px) 44rem, 100vw"
              className="h-auto w-full"
            />
          </div>
          <p>
            The switch is saved in your vault as <span className="code">telemetry.enabled</span>, so turning it off
            on one device turns it off on every device that syncs that vault.
          </p>
        </Part>

        <Part title="Checking for updates">
          <p>
            A little after the Mac app opens, and then once a day while it runs, it asks gaspmd.com/api/version for
            the newest version. It also asks when you choose Check for updates in the Gasp menu. The request
            carries nothing about you or your Mac: no version, no ID and nothing from your notes. The answer is the
            same for everyone:
          </p>
          <pre className="code overflow-x-auto rounded-xl bg-fill p-5 leading-relaxed text-ink">{VERSION_ANSWER}</pre>
          <p>
            Nothing about these requests is logged or counted. When you choose Update, Gasp downloads the new version
            from GitHub, and it installs it only after macOS confirms that Gasp&apos;s developer signed it and Apple
            notarized it.
          </p>
          <p>
            To stop the daily check, turn off Check for updates on the General page of Settings. It&apos;s saved in
            your vault as <span className="code">updates.check</span>. The menu item still checks when you ask it to.
          </p>
        </Part>

        <Part title="Everything else the app connects to">
          <p>
            Sync goes through your own iCloud Drive, or straight to GitHub with your own account, and the writing check runs on your device. When a note
            shows a link card, Gasp fetches that page&apos;s title and picture from the site the link points to.
          </p>
          <p>
            Gasp&apos;s source is public, so you can <a href={REPO_URL} className="underline underline-offset-4">read the code</a>{" "}
            that sends the ping. If something here is unclear, open an issue on GitHub and we&apos;ll answer it.
          </p>
          <p>
            <Link href="/" className="underline underline-offset-4">
              Back to Gasp
            </Link>
          </p>
        </Part>
      </main>
      <SiteFooter />
    </>
  );
}
