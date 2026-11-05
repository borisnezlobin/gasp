import { HubList } from "@/components/HubList";
import { PageShell, PageTitle } from "@/components/PageShell";
import { TryGasp } from "@/components/TryGasp";
import { pageMetadata } from "@/lib/metadata";
import { COMPARISON_LINKS } from "@/lib/pages";
import { COMPARE_PATH } from "@/lib/site";

export const metadata = pageMetadata({
  path: COMPARE_PATH,
  title: "Gasp compared with Obsidian, Bear, Notion and more",
  description:
    "How Gasp compares with Obsidian, Bear, Notion, Apple Notes, Typora, iA Writer and Logseq, and what each app does best.",
});

export default function ComparePage() {
  return (
    <PageShell width="medium">
      <PageTitle title="Gasp compared">
        <p>
          Gasp is a free app for writing notes. Here is how it compares with other popular note apps. Each page
          explains what each app does best and which kind of person it suits. It also shows how to move your notes to
          Gasp.
        </p>
      </PageTitle>
      <HubList links={COMPARISON_LINKS} />
      <TryGasp />
    </PageShell>
  );
}
