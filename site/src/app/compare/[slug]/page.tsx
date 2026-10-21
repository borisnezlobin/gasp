import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { Chooser } from "@/components/compare/Chooser";
import { ComparisonTable } from "@/components/compare/ComparisonTable";
import { MeasuredBars } from "@/components/compare/MeasuredBars";
import { Faq } from "@/components/Faq";
import { Chapter, PageShell, PageTitle } from "@/components/PageShell";
import { Steps } from "@/components/Steps";
import { TryGasp } from "@/components/TryGasp";
import { comparisonOf, COMPARISONS } from "@/lib/comparisons";
import { pageMetadata } from "@/lib/metadata";
import { COMPARE_PATH } from "@/lib/site";

type Params = { params: Promise<{ slug: string }> };

export const dynamicParams = false;

export function generateStaticParams() {
  return COMPARISONS.map(({ slug }) => ({ slug }));
}

export async function generateMetadata({ params }: Params): Promise<Metadata> {
  const comparison = comparisonOf((await params).slug);
  if (!comparison) return {};
  const { title, description } = comparison;
  return pageMetadata({ path: `${COMPARE_PATH}/${comparison.slug}`, title, description });
}

export default async function ComparisonPage({ params }: Params) {
  const comparison = comparisonOf((await params).slug);
  if (!comparison) notFound();
  const { rival } = comparison;
  return (
    <PageShell width="medium">
      <PageTitle title={`Gasp vs ${rival}`}>
        <p>{comparison.summary}</p>
      </PageTitle>

      <Chapter id="fit" title="Which one fits you">
        <Chooser rival={rival} reasons={comparison.reasons} />
      </Chapter>

      {comparison.measures && (
        <Chapter id="measured" title="Measured side by side">
          <MeasuredBars rival={rival} measures={comparison.measures} />
        </Chapter>
      )}

      <Chapter id="table" title="Feature by feature">
        <ComparisonTable rival={rival} rows={comparison.rows} />
      </Chapter>

      <Chapter id="switching" title={comparison.switching.title}>
        <Steps steps={comparison.switching.steps} />
      </Chapter>

      <Faq questions={comparison.questions} />
      <TryGasp />
    </PageShell>
  );
}
