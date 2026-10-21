import { AppleLogo, Cloud, GithubLogo, LinuxLogo } from "@phosphor-icons/react/dist/ssr";
import type { Icon } from "@phosphor-icons/react";
import type { ReactNode } from "react";
import { BulletList } from "@/components/BulletList";
import { Faq } from "@/components/Faq";
import { FactList } from "@/components/FactList";
import { Chapter, PageShell, PageTitle } from "@/components/PageShell";
import { TryGasp } from "@/components/TryGasp";
import { TEAM_QUESTION } from "@/lib/answers";
import { pageMetadata } from "@/lib/metadata";
import { FEATURES_PATH } from "@/lib/site";

const PATH = `${FEATURES_PATH}/sync`;

export const metadata = pageMetadata({
  path: PATH,
  title: "Free Markdown sync through iCloud or GitHub | Gasp",
  description:
    "Gasp keeps your notes the same on every computer through your own iCloud Drive or GitHub account. Both are free. Gasp never stores your notes.",
});

type Route = { name: string; icon: Icon; devices: { icon: Icon; name: string }[]; points: string[] };

const ROUTES: Route[] = [
  {
    name: "iCloud Drive",
    icon: Cloud,
    devices: [{ icon: AppleLogo, name: "Mac" }],
    points: [
      "iCloud Drive is Apple's online storage. It comes with every Apple account.",
      "In Gasp's settings, choose Sync with iCloud. Gasp copies your notes into a folder called Gasp in your iCloud Drive. Your original folder stays where it was.",
      "Your notes stay ordinary files that you can see in Finder.",
      "If you change the same note on two computers before iCloud catches up, iCloud keeps both copies. Gasp shows them side by side so you can compare them.",
    ],
  },
  {
    name: "GitHub",
    icon: GithubLogo,
    devices: [
      { icon: AppleLogo, name: "Mac" },
      { icon: LinuxLogo, name: "Linux" },
    ],
    points: [
      "GitHub is a website that stores files and keeps every earlier version of them. A free account is enough.",
      "In Gasp's settings, choose Sign in with GitHub. Gasp makes a private folder for your notes that only you can see.",
      "About a minute after you stop typing, Gasp sends your changes to GitHub. It also brings in the changes from your other computers.",
    ],
  },
];

function RouteCard({ route }: { route: Route }) {
  const RouteIcon = route.icon;
  return (
    <section aria-labelledby={`${route.name}-title`} className="rounded-2xl bg-surface p-6 shadow-lifted">
      <div className="flex items-center gap-3">
        <RouteIcon size={28} aria-hidden />
        <h3 id={`${route.name}-title`} className="subheading text-2xl">
          {route.name}
        </h3>
      </div>
      <p className="mt-3 flex flex-wrap gap-2">
        {route.devices.map((device) => (
          <span key={device.name} className="small inline-flex items-center gap-1.5 rounded-full bg-fill px-3 pt-1.5 pb-0.5 text-ink-soft">
            <device.icon size={16} weight="fill" aria-hidden />
            {device.name}
          </span>
        ))}
      </p>
      <BulletList items={route.points} className="body mt-5 text-ink-soft" />
    </section>
  );
}

/** One line of a note, as each device left it. */
function NoteLine({ children, tone = "plain" }: { children: ReactNode; tone?: "plain" | "added" }) {
  const tones = { plain: "text-ink-soft", added: "bg-highlight text-ink" };
  return <li className={`rounded-md px-2 pt-1 ${tones[tone]}`}>{children}</li>;
}

function NoteCard({ title, children }: { title: string; children: ReactNode }) {
  return (
    <figure className="rounded-xl bg-surface p-4 shadow-lifted">
      <figcaption className="small font-bold text-ink">{title}</figcaption>
      <ul className="small mt-3 space-y-1">{children}</ul>
    </figure>
  );
}

/** Two devices adding to the same list merge with both lines kept; only a
    line both changed needs you. */
function MergeExample() {
  return (
    <div className="grid gap-4 sm:grid-cols-3">
      <NoteCard title="On your Mac">
        <NoteLine>Groceries</NoteLine>
        <NoteLine>Oat milk</NoteLine>
        <NoteLine tone="added">Coffee beans</NoteLine>
      </NoteCard>
      <NoteCard title="On your laptop">
        <NoteLine>Groceries</NoteLine>
        <NoteLine>Oat milk</NoteLine>
        <NoteLine tone="added">Lemons</NoteLine>
      </NoteCard>
      <NoteCard title="After sync">
        <NoteLine>Groceries</NoteLine>
        <NoteLine>Oat milk</NoteLine>
        <NoteLine tone="added">Coffee beans</NoteLine>
        <NoteLine tone="added">Lemons</NoteLine>
      </NoteCard>
    </div>
  );
}

export default function Sync() {
  return (
    <PageShell width="medium">
      <PageTitle title="Free sync">
        <p>
          Sync keeps the same notes on all your computers. Gasp syncs through your own iCloud Drive or your own GitHub
          account. Both are free. Gasp never keeps a copy of your notes on its own servers.
        </p>
      </PageTitle>

      <Chapter id="routes" title="Two ways to sync">
        <div className="grid gap-5 md:grid-cols-2">
          {ROUTES.map((route) => (
            <RouteCard key={route.name} route={route} />
          ))}
        </div>
      </Chapter>

      <Chapter
        id="merging"
        title="When two computers change one note"
        intro="With GitHub sync, Gasp combines the changes from each computer line by line. If each computer added different lines, Gasp keeps all of them."
      >
        <MergeExample />
        <p className="body mt-8 measure text-ink-soft">
          Sometimes two computers change the very same line. Then Gasp shows you both versions side by side. You can
          pick one, keep both or write your own. You can keep working on the note while you decide. All your other
          notes keep syncing as usual.
        </p>
      </Chapter>

      <Chapter id="team" title="Sharing notes with a team">
        <div className="body measure space-y-4 text-ink-soft">
          <p>
            A team can share one folder of notes through GitHub. One person makes a private folder for the notes on
            GitHub and invites the others. Then each person connects Gasp to that folder in the sync settings.
          </p>
          <p>
            Everyone&apos;s changes reach the others within a few minutes. You won&apos;t see other people type in real
            time, the way you do in Google Docs. Instead, Gasp brings in their changes each time it syncs.
          </p>
        </div>
      </Chapter>

      <Chapter id="cost" title="What it costs">
        <FactList
          facts={[
            { term: "Gasp", detail: "Free. Sync is part of the app." },
            { term: "iCloud Drive", detail: "Free up to 5 GB, which comes with every Apple account. Text notes take up very little of that space." },
            { term: "GitHub", detail: "Free. A free GitHub account can have as many private folders as you like." },
          ]}
        />
      </Chapter>

      <Faq
        questions={[
          {
            question: "Is Gasp's sync really free?",
            answer:
              "Yes. Gasp doesn't run a sync service of its own. Your notes travel through your own iCloud Drive or GitHub account. Both are free.",
          },
          {
            question: "Can Gasp sync with Linux?",
            answer:
              "Yes, through GitHub. iCloud Drive only works on Apple devices. If you use both a Mac and a Linux computer, sync through GitHub.",
          },
          {
            question: "Do I need to know how GitHub works?",
            answer:
              "No. After you sign in with GitHub, Gasp does everything for you. A small icon shows whether your notes are synced.",
          },
          {
            question: "Are synced notes encrypted?",
            answer:
              "Notes you lock in Gasp are scrambled on your computer before they're uploaded. iCloud and GitHub can't read them. Notes you haven't locked are stored like any other file in your iCloud Drive or on GitHub.",
          },
          {
            question: "Can I use a GitHub folder I already have?",
            answer:
              "Yes. In Gasp's sync settings, choose Use a repository address and token. Enter the folder's address on GitHub and an access token, which works like a password for that one folder. If your notes already sync with git, Gasp keeps using it.",
          },
          TEAM_QUESTION,
        ]}
      />
      <TryGasp />
    </PageShell>
  );
}
