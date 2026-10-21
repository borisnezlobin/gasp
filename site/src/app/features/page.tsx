import { HubList } from "@/components/HubList";
import { Chapter, PageShell, PageTitle } from "@/components/PageShell";
import { TryGasp } from "@/components/TryGasp";
import { pageMetadata } from "@/lib/metadata";
import { FEATURE_LINKS } from "@/lib/pages";
import { FEATURES_PATH } from "@/lib/site";

export const metadata = pageMetadata({
  path: FEATURES_PATH,
  title: "Features of Gasp, a fast Markdown editor",
  description:
    "Gasp opens Obsidian vaults, starts in 0.42 seconds, syncs for free, locks private notes with a password, shows math as you type and works with AI.",
});

const EVERYTHING_ELSE: { name: string; detail: string }[] = [
  { name: "Formatting as you type", detail: "Gasp hides Markdown symbols as you type. Your note looks finished while you write it. The symbols come back where your cursor is." },
  { name: "Tables", detail: "You edit tables like a spreadsheet. You can drag rows and columns around and sort them." },
  { name: "Search", detail: "Gasp searches every note as you type. You can also replace a word in all your notes at once." },
  { name: "Command list", detail: "Press ⌘P and type a few letters to find any command. Commands you used recently come first." },
  { name: "Spelling and grammar", detail: "Gasp checks your writing without the internet. It learns the names and words you use often." },
  { name: "Footnotes", detail: "Gasp numbers your footnotes for you. It warns you about a footnote that's missing or never used." },
  { name: "Saving as PDF or a web page", detail: "Any note can become a PDF that looks like a printed page, or a web page for your website." },
  { name: "Tabs and side-by-side notes", detail: "Open notes in tabs. Drag a tab to the side to see two notes at once." },
  { name: "Links", detail: "The sidebar shows which notes link to the one you have open, and the headings inside it." },
  { name: "Daily notes and templates", detail: "One command opens today's note. It can start from a template with the date filled in." },
  { name: "File recovery", detail: "Gasp saves a backup copy of your notes every five minutes and keeps it for a week." },
  { name: "Writing time", detail: "Gasp tracks how long you've spent writing each note. It also shows the word count and reading time." },
];

export default function FeaturesPage() {
  return (
    <PageShell width="medium">
      <PageTitle title="Features">
        <p>
          Gasp is a free app for writing notes on a Mac or a Linux computer. It keeps your notes as ordinary text files
          in a folder you choose. Here is what it can do.
        </p>
      </PageTitle>
      <HubList links={FEATURE_LINKS} />

      <Chapter id="more" title="Everything else">
        <dl className="grid gap-x-10 gap-y-8 sm:grid-cols-2">
          {EVERYTHING_ELSE.map((feature) => (
            <div key={feature.name}>
              <dt className="subheading">{feature.name}</dt>
              <dd className="body mt-1 text-ink-soft">{feature.detail}</dd>
            </div>
          ))}
        </dl>
      </Chapter>
      <TryGasp />
    </PageShell>
  );
}
