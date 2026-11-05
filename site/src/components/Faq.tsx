import { Plus } from "@phosphor-icons/react/dist/ssr";
import { LineIcon } from "./LineIcon";
import { Chapter } from "./PageShell";
import { JsonLd } from "./seo/JsonLd";

export type Question = { question: string; answer: string };

/** Questions people ask before they switch, each answer folded under its
    question, and marked up so search and answer engines can quote them. */
export function Faq({ questions }: { questions: Question[] }) {
  return (
    <Chapter id="faq" title="Questions">
      <div className="divide-y divide-rule">
        {questions.map(({ question, answer }) => (
          <details key={question} className="group py-1">
            <summary className="flex cursor-pointer list-none items-start justify-between gap-6 py-4 [&::-webkit-details-marker]:hidden">
              <h3 className="subheading">{question}</h3>
              <span className="subheading">
                <LineIcon>
                  <Plus
                    size={20}
                    weight="bold"
                    aria-hidden
                    className="text-ink-muted transition-transform duration-150 ease-out-soft group-open:rotate-45"
                  />
                </LineIcon>
              </span>
            </summary>
            <p className="body measure pb-5 text-ink-soft">{answer}</p>
          </details>
        ))}
      </div>
      <JsonLd
        data={{
          "@type": "FAQPage",
          mainEntity: questions.map(({ question, answer }) => ({
            "@type": "Question",
            name: question,
            acceptedAnswer: { "@type": "Answer", text: answer },
          })),
        }}
      />
    </Chapter>
  );
}
