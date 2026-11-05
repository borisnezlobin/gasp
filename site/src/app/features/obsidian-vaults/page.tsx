import Image, { type StaticImageData } from "next/image";
import gaspIcon from "@/app/icon.png";
import obsidianIcon from "@/assets/icon-obsidian.png";
import { Faq } from "@/components/Faq";
import { Chapter, PageShell, PageTitle } from "@/components/PageShell";
import { Steps } from "@/components/Steps";
import { SupportList, type Support } from "@/components/SupportList";
import { TryGasp } from "@/components/TryGasp";
import { pageMetadata } from "@/lib/metadata";
import { FEATURES_PATH } from "@/lib/site";

const PATH = `${FEATURES_PATH}/obsidian-vaults`;

export const metadata = pageMetadata({
  path: PATH,
  title: "An Obsidian-compatible Markdown editor | Gasp",
  description:
    "Gasp opens your Obsidian vault as it is. Links, embedded notes, callouts, math and footnotes all show up. You can keep using Obsidian on the same folder.",
});

type Owner = "gasp" | "obsidian" | "both";

const OWNER_ICONS: Record<Owner, { icons: StaticImageData[]; label: string }> = {
  obsidian: { icons: [obsidianIcon], label: "Obsidian's" },
  gasp: { icons: [gaspIcon], label: "Gasp's" },
  both: { icons: [obsidianIcon, gaspIcon], label: "Both apps'" },
};

const VAULT: { path: string; depth: number; owner: Owner; detail: string }[] = [
  { path: "Notes/", depth: 0, owner: "both", detail: "Your vault. Both apps open this same folder." },
  { path: ".obsidian/", depth: 1, owner: "obsidian", detail: "Obsidian's settings. Gasp reads them but never changes them." },
  { path: ".gasp/", depth: 1, owner: "gasp", detail: "Gasp's own settings, colors and keyboard shortcuts." },
  { path: "Daily/2026-10-21.md", depth: 1, owner: "both", detail: "Your notes. Both apps can read and edit them." },
  { path: "images/", depth: 1, owner: "both", detail: "Your pictures and other attachments." },
];

/** The vault as a folder tree, each entry marked with the app that owns it. */
function VaultTree() {
  return (
    <ul className="overflow-hidden rounded-2xl bg-surface shadow-lifted">
      {VAULT.map((entry) => {
        const owner = OWNER_ICONS[entry.owner];
        return (
          <li key={entry.path} className="flex items-center gap-4 border-b border-rule px-5 py-4 last:border-b-0">
            <span className="flex w-12 shrink-0 -space-x-2" aria-label={owner.label} role="img">
              {owner.icons.map((icon, index) => (
                <Image key={index} src={icon} alt="" width={28} height={28} className="size-7 rounded-md" />
              ))}
            </span>
            <div className="min-w-0 sm:flex sm:flex-1 sm:items-baseline sm:gap-6">
              <code className="code block text-base text-ink sm:w-60 sm:shrink-0" style={{ paddingInlineStart: `${entry.depth * 1.5}rem` }}>
                {entry.path}
              </code>
              <span className="small block text-ink-muted">{entry.detail}</span>
            </div>
          </li>
        );
      })}
    </ul>
  );
}

const RENDERS: Support[] = [
  { sample: "[[Lemma 2#Proof|the proof]]", name: "Links to other notes, including links to a heading", coverage: "yes" },
  { sample: "![[Fourier series#Parseval]]", name: "A note, or part of one, shown inside another note", coverage: "yes" },
  { sample: "![[photo.png|300]]", name: "Pictures, at the size you set", coverage: "yes" },
  { sample: "> [!tip]- Folded", name: "Callouts, the colored boxes for tips and warnings", coverage: "yes" },
  { sample: "^key-result", name: "Links to a single paragraph", coverage: "yes" },
  { sample: "==highlight==", name: "Highlights", coverage: "yes" },
  { sample: "%% hidden %%", name: "Hidden comments", coverage: "yes" },
  { sample: "#reading/books", name: "Tags, listed in a sidebar", coverage: "yes" },
  { sample: "$e^{i\\pi} + 1 = 0$", name: "Math, shown as finished equations", coverage: "yes" },
  { sample: "[^1]", name: "Footnotes", coverage: "yes" },
  { sample: "- [ ] Reply to Sam", name: "Checklists", coverage: "yes" },
  { sample: "| Site | Hz |", name: "Tables, which you edit like a spreadsheet", coverage: "yes" },
  { sample: "tags: [physics]", name: "Note properties at the top, shown as plain text", coverage: "partly" },
  { sample: "```dataview", name: "Dataview queries, which need an Obsidian plugin", coverage: "no" },
  { sample: "```mermaid", name: "Mermaid diagrams, which show as code", coverage: "no" },
  { sample: "^[inline note]", name: "Footnotes written inside the sentence", coverage: "no" },
];

export default function ObsidianVaults() {
  return (
    <PageShell width="medium">
      <PageTitle title="Obsidian-compatible">
        <p>
          Gasp opens your Obsidian vault as it is. You don&apos;t need to import or convert anything. Your notes stay in
          the same folder. You can keep using Obsidian on them too.
        </p>
      </PageTitle>

      <Chapter id="folder" title="One folder, two apps">
        <VaultTree />
      </Chapter>

      <Chapter
        id="syntax"
        title="What Gasp draws"
        intro="Everything you write in Obsidian looks the same in Gasp. The only exception is anything that needs an Obsidian plugin. Gasp can't run plugins. That text shows exactly as you typed it."
      >
        <SupportList items={RENDERS} />
      </Chapter>

      <Chapter id="settings" title="Your Obsidian settings">
        <p className="body measure text-ink-soft">
          The first time you open your vault, Gasp offers to copy your Obsidian settings. It brings over your keyboard
          shortcuts, fonts, accent color and the folder where your attachments go. Afterwards, Gasp shows you a list
          of every setting it copied.
        </p>
      </Chapter>

      <Chapter id="open" title="Opening your vault">
        <Steps
          steps={[
            "Download Gasp and open it.",
            "Choose **Open a folder** and pick your Obsidian vault. Your vault is the folder with a hidden .obsidian folder inside it. If Gasp already has a vault open, use **Open another vault** instead.",
            "Gasp offers to **Import settings from Obsidian**. Say yes to bring over your keyboard shortcuts, fonts and colors.",
            "You can keep Obsidian installed. When you save a note in one app, the other app shows the change.",
          ]}
        />
      </Chapter>

      <Faq
        questions={[
          {
            question: "Will Gasp change my Obsidian vault?",
            answer:
              "Gasp only changes the notes you edit. It keeps its own settings in a separate .gasp folder. It never changes Obsidian's .obsidian folder. Obsidian opens your vault exactly as before.",
          },
          {
            question: "Can I use Gasp and Obsidian on the same vault at the same time?",
            answer:
              "Yes. Both apps notice when a note changes. When you save a note in one app, the other app shows the new version. You don't need to close either app.",
          },
          {
            question: "Do Obsidian plugins work in Gasp?",
            answer:
              "No. Instead, Gasp has many popular plugin features built in. These include math shortcuts, a table editor, footnote tools and a timer that tracks how long you spend writing each note. Text that needs a plugin, like a Dataview query, shows as you typed it.",
          },
          {
            question: "Does Gasp work with Obsidian's wikilinks?",
            answer:
              "Yes. Links written in [[double brackets]] work, including links to a heading or a single paragraph. When you rename a note in Gasp, every link to it is updated, just like in Obsidian.",
          },
          {
            question: "How do I sync an Obsidian vault with Gasp?",
            answer:
              "Turn on sync in Gasp's settings. Gasp can sync your vault through your own iCloud Drive, or through a private folder on GitHub that Gasp sets up for you. Both are free. If your vault already syncs with git, Gasp keeps using git.",
          },
        ]}
      />
      <TryGasp />
    </PageShell>
  );
}
