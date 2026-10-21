import mathDark from "@/assets/shot-math-dark.png";
import mathLight from "@/assets/shot-math-light.png";
import { BulletList } from "@/components/BulletList";
import { Faq } from "@/components/Faq";
import { InkImage } from "@/components/InkImage";
import { Chapter, PageShell, PageTitle } from "@/components/PageShell";
import { TryGasp } from "@/components/TryGasp";
import { pageMetadata } from "@/lib/metadata";
import { FEATURES_PATH } from "@/lib/site";

const PATH = `${FEATURES_PATH}/math`;

export const metadata = pageMetadata({
  path: PATH,
  title: "A Markdown editor for LaTeX math notes | Gasp",
  description:
    "Gasp shows LaTeX equations as finished math while you type. It has 226 math shortcuts built in and saves notes as PDFs. It's free on Mac and Linux.",
});

/** A few of the built-in snippets, from crates/snippets/defaults/snippets.txt. */
const SNIPPETS: { typed: string; becomes: string; where: string }[] = [
  { typed: "mk", becomes: "$ $", where: "Starts math inside a sentence." },
  { typed: "dm", becomes: "$$ $$", where: "Starts math on its own lines." },
  { typed: "//", becomes: "\\frac{ }{ }", where: "Makes an empty fraction." },
  { typed: "x/", becomes: "\\frac{x}{ }", where: "Makes a fraction with x on top." },
  { typed: "@a", becomes: "\\alpha", where: "Types the Greek letter alpha. @b is beta, and so on." },
  { typed: "beg", becomes: "\\begin{ } … \\end{ }", where: "Starts a block, such as a matrix. You type its name once." },
  { typed: "reals", becomes: "\\mathbb{R}", where: "Types the symbol for the real numbers." },
];

function SnippetTable() {
  return (
    <table className="body w-full text-left">
      <thead>
        <tr className="border-b border-rule">
          <th scope="col" className="pb-3 font-bold">
            You type
          </th>
          <th scope="col" className="pb-3 font-bold">
            Gasp writes
          </th>
          <th scope="col" className="pb-3 font-bold max-sm:sr-only">
            What it&apos;s for
          </th>
        </tr>
      </thead>
      <tbody className="divide-y divide-rule">
        {SNIPPETS.map((snippet) => (
          <tr key={snippet.typed}>
            <td className="py-3 pr-4">
              <code className="code text-ink">{snippet.typed}</code>
            </td>
            <td className="py-3 pr-4">
              <code className="code text-ink">{snippet.becomes}</code>
            </td>
            <td className="py-3 text-ink-soft max-sm:hidden">{snippet.where}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const WHILE_TYPING = [
  "Each equation shows as finished math. When you click into it, you see the LaTeX again so you can edit it.",
  "While you edit an equation, a preview of the finished math sits just above the line.",
  "Each pair of matching brackets has its own color, so you can see which brackets belong together.",
  "Brackets around a sum, an integral or a fraction grow to fit what's inside them.",
  "The Tab key jumps past the next closing bracket. At the end of the math, it jumps out of the math.",
  "Inside a matrix, the Tab key moves to the next column and the Enter key starts the next row.",
];

export default function MathPage() {
  return (
    <PageShell width="medium">
      <PageTitle title="Math">
        <p>
          You write math in Gasp with LaTeX, the standard way of typing equations. Gasp shows each equation as finished
          math while you type. In the file, the equation stays as plain LaTeX between dollar signs. That means any
          other Markdown app can read it too.
        </p>
      </PageTitle>

      <Chapter id="looks" title="Notes full of equations">
        <div className="overflow-hidden rounded-xl shadow-lifted">
          <InkImage
            light={mathLight}
            dark={mathDark}
            alt="Fourier series study notes in Gasp: Parseval's identity worked down to π²/8, the discrete Fourier transform and its inverse, the 4×4 DFT matrix, and a short NumPy check."
            sizes="(min-width: 896px) 56rem, 100vw"
            className="h-auto w-full"
          />
        </div>
      </Chapter>

      <Chapter
        id="snippets"
        title="Shortcuts for typing math"
        intro="Gasp has 226 shortcuts for typing math. You type a few letters, and Gasp replaces them with the LaTeX you need. The shortcuts are the same as the ones in Latex Suite, a popular Obsidian plugin. You can change them or add your own in Gasp's settings."
      >
        <SnippetTable />
      </Chapter>

      <Chapter id="typing" title="While you type">
        <BulletList items={WHILE_TYPING} className="body measure text-ink-soft" />
      </Chapter>

      <Chapter id="export" title="Sharing it">
        <p className="body measure text-ink-soft">
          You can save any note as a PDF. The equations come out as cleanly as in a printed textbook. Footnotes sit
          at the bottom of each page. You can also save a note as a web page. Every modern web browser shows its
          equations without any extra software.
        </p>
      </Chapter>

      <Faq
        questions={[
          {
            question: "Does Gasp support LaTeX math?",
            answer:
              "Yes. Put math inside a sentence between single dollar signs, like $x^2$. Put math on its own lines between double dollar signs. This works the same way as in Obsidian. Gasp shows each equation as finished math. When you click into an equation, Gasp shows its LaTeX again.",
          },
          {
            question: "Does Gasp have Latex Suite's snippets?",
            answer:
              "Yes. Gasp's built-in math shortcuts are the same as Latex Suite's. For example, mk starts math inside a sentence, and // makes a fraction.",
          },
          {
            question: "Can I export math notes to PDF?",
            answer: "Yes. Gasp saves the note as a PDF with its equations and footnotes laid out like a printed page.",
          },
        ]}
      />
      <TryGasp />
    </PageShell>
  );
}
