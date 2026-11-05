import { BulletList } from "@/components/BulletList";
import { Faq } from "@/components/Faq";
import { FactList, type Fact } from "@/components/FactList";
import { LaunchRace } from "@/components/LaunchRace";
import { IdleTrace, KeystrokeSpeed } from "@/components/LightFigures";
import { LineField } from "@/components/LineField";
import { Chapter, PageShell, PageTitle } from "@/components/PageShell";
import { TryGasp } from "@/components/TryGasp";
import { pageMetadata } from "@/lib/metadata";
import { FEATURES_PATH } from "@/lib/site";

const PATH = `${FEATURES_PATH}/speed`;

export const metadata = pageMetadata({
  path: PATH,
  title: "A Markdown editor ready in 0.42 seconds | Gasp",
  description:
    "Gasp shows a note you can type in 0.42 seconds after you open it. It stays instant in notes 48,000 lines long and uses about 60 MB of memory.",
});

/** From PLAN.md's verification tables, measured on a MacBook Pro with M2 Pro. */
const FIGURES: Fact[] = [
  {
    term: "About 0.15 seconds",
    detail: "The time from opening Gasp to its window appearing. It's the same with 1 note or with 204 notes.",
  },
  {
    term: "0.3 milliseconds",
    detail:
      "The time between pressing a key and seeing the letter, in a note 5,000 lines long. A millisecond is a thousandth of a second. In a note 48,000 lines long, it takes 2.2 milliseconds.",
  },
  { term: "7 milliseconds", detail: "The time to open a note or switch to another tab." },
  {
    term: "56 to 69 megabytes",
    detail:
      "The memory Gasp uses once it has started. The low number is with one short note open. The high number is with a note full of large screenshots.",
  },
  {
    term: "0.9%",
    detail:
      "How much of the processor Gasp uses while a note sits open on a Mac. Obsidian uses 1.7% in the same test. When Gasp's window is hidden, it uses none.",
  },
  {
    term: "0.13 milliseconds",
    detail: "The time to search every note in a test vault, after each letter you type into the search box.",
  },
];

const REASONS = [
  "Gasp is a native app, written in the Rust programming language. Many note apps, like Obsidian, are built on Electron, which runs the app inside a hidden web browser. Gasp has no web browser inside it.",
  "When you type, Gasp only re-reads the paragraph you're changing. It doesn't re-read the whole note.",
  "Gasp only lays out the lines you can see on screen. That's why a note 48,000 lines long scrolls as smoothly as a short one.",
  "We measure Gasp's speed after every change to its code. If a change makes Gasp slower than our limits, the change is rejected.",
];

export default function Speed() {
  return (
    <PageShell width="medium">
      <PageTitle title="Speed">
        <p>
          Gasp shows a note you can type in less than half a second after you open it. It stays just as quick in notes
          that are tens of thousands of lines long.
        </p>
      </PageTitle>

      <Chapter id="launch" title="Ready in 0.42 seconds">
        <LaunchRace />
      </Chapter>

      <Chapter id="long-notes" title="Quick in huge notes">
        <LineField />
        <div className="mt-8">
          <KeystrokeSpeed />
        </div>
        <div className="mt-16">
          <IdleTrace />
        </div>
      </Chapter>

      <Chapter id="figures" title="Every number we measured">
        <FactList facts={FIGURES} />
      </Chapter>

      <Chapter id="why" title="Why it's fast">
        <BulletList items={REASONS} className="body measure text-ink-soft" />
      </Chapter>

      <Faq
        questions={[
          {
            question: "How fast is Gasp compared with other editors?",
            answer:
              "We recorded three apps opening on a MacBook Pro with an M2 Pro chip. Gasp had a note ready to type in 0.42 seconds. Apple Notes took 1.53 seconds. Obsidian took 4.66 seconds.",
          },
          {
            question: "Does Gasp slow down with large vaults or long notes?",
            answer:
              "No. Gasp starts in about the same time with 1 note or with 204 notes. In a note 48,000 lines long, a letter still appears 2.2 milliseconds after you press the key. That's faster than your screen redraws itself.",
          },
          {
            question: "How much memory does Gasp use?",
            answer:
              "About 56 to 69 megabytes once it has started. The spelling and grammar checker runs separately. It uses about 125 megabytes while it checks your writing. Then it closes itself after a minute with nothing to check.",
          },
          {
            question: "Is Gasp built on Electron?",
            answer: "No. Gasp is a native app written in Rust. It draws its window with your computer's graphics chip. There's no web browser inside it.",
          },
        ]}
      />
      <TryGasp />
    </PageShell>
  );
}
