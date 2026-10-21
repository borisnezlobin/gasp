import { LockKeyOpen, LockKey } from "@phosphor-icons/react/dist/ssr";
import type { ReactNode } from "react";
import Link from "next/link";
import { BulletList } from "@/components/BulletList";
import { Faq } from "@/components/Faq";
import { FactList, type Fact } from "@/components/FactList";
import { Chapter, CodeBlock, PageShell, PageTitle } from "@/components/PageShell";
import { Steps } from "@/components/Steps";
import { proseLinkStyle } from "@/components/styles";
import { TryGasp } from "@/components/TryGasp";
import { pageMetadata } from "@/lib/metadata";
import { FEATURES_PATH, PRIVACY_PATH } from "@/lib/site";

const PATH = `${FEATURES_PATH}/locked-notes`;

export const metadata = pageMetadata({
  path: PATH,
  title: "Encrypted Markdown notes, locked with a password | Gasp",
  description:
    "Lock a note in Gasp and its text is scrambled on your computer, in iCloud and on GitHub. Only your password or Touch ID opens it. It's free.",
});

/** What a locked note's file holds, in the format crates/crypt writes. The
    ciphertext here is made up. */
const ON_DISK = `<!-- Encrypted with Gasp. Open it in Gasp to read it. -->
gasp-encrypted: 1
key: vault
claude: never
name: shown
wrapped: UvImZaYMEtKJGF2VDuiBNgkWb2sRPReNbA/TkB/yOaG

glfIPk5VlDPk4C47bIkprJIoekk6P0K4uGpSSozBfGIy2EJAPnjR/rohtxlB3lex
0XEw/yy6yxz4Uk0yGfuBXunJJm/oSHoNrKsFXJu59awr2qxPDjpLK4NFQV7FZmH+
UzHQR1xfxRXmyqhAPu7NPpZP+rtJySLdi46tYBfB2WiucHX4PN8RJIb0/ZWTq338`;

function Side({ icon, title, children }: { icon: ReactNode; title: string; children: ReactNode }) {
  return (
    <figure className="min-w-0">
      <figcaption className="flex items-center gap-2 font-bold">
        {icon}
        {title}
      </figcaption>
      <div className="mt-3">{children}</div>
    </figure>
  );
}

/** The same note twice: the file every disk and cloud sees, and what Gasp
    shows once you unlock it. */
function TwoViews() {
  return (
    <div className="grid gap-8 md:grid-cols-2 md:gap-6">
      <Side icon={<LockKey size={20} weight="bold" aria-hidden />} title="The note's file, as your computer, iCloud and GitHub store it">
        <div className="text-sm">
          <CodeBlock wrap>{ON_DISK}</CodeBlock>
        </div>
      </Side>
      <Side icon={<LockKeyOpen size={20} weight="bold" aria-hidden />} title="The same note, unlocked in Gasp">
        <div className="rounded-xl bg-surface p-5 shadow-lifted">
          <p className="subheading">Passport numbers</p>
          <ul className="body mt-3 space-y-1 text-ink-soft">
            <li>Mine: renews in March 2029</li>
            <li>Sam&apos;s: renews in June 2031</li>
          </ul>
          <p className="small mt-4 text-ink-muted">Locks again after two minutes in the background.</p>
        </div>
      </Side>
    </div>
  );
}

const HOW: Fact[] = [
  {
    term: "Scrambling",
    detail:
      "Gasp scrambles the note's text with a cipher called XChaCha20-Poly1305. A cipher is a method for turning text into nonsense that only the right key can turn back.",
  },
  {
    term: "Keys",
    detail:
      "Each locked note gets its own random key. Your password unlocks that key. You can use one password for all your locked notes, or give a note its own password.",
  },
  {
    term: "Your password",
    detail:
      "Gasp never saves your password. It turns your password into a key with a method called Argon2id, which is slow on purpose. That makes it very slow for anyone to guess your password.",
  },
  { term: "Unlocking", detail: "Type your password. On a Mac, you can also turn on Touch ID in settings." },
  {
    term: "Locking again",
    detail: "An unlocked note locks itself after two minutes in the background. It also blurs while you're using another app.",
  },
  {
    term: "AI assistants",
    detail: "By default, AI assistants can't read locked notes. You can let an assistant read a note while it's unlocked, or at any time.",
  },
  {
    term: "File names",
    detail: "You can hide a locked note's name too. Gasp then gives the file a made-up name. The note's real title won't show up in iCloud or on GitHub.",
  },
];

const LIMITS = [
  "Pictures and other attachments in a locked note aren't scrambled. Only the note's text is.",
  "Search finds a locked note by its name. It can't search the text inside it.",
  "If you forget your password, nobody can open the note, including us.",
];

export default function LockedNotes() {
  return (
    <PageShell width="medium">
      <PageTitle title="Locked notes">
        <p>
          When you lock a note, Gasp scrambles its text on your computer. The note stays scrambled in iCloud and on
          GitHub. Only you, with your password, can open it again.
        </p>
      </PageTitle>

      <Chapter id="views" title="What everyone else sees">
        <TwoViews />
      </Chapter>

      <Chapter id="how" title="How it works">
        <FactList facts={HOW} />
      </Chapter>

      <Chapter id="lock" title="Locking a note">
        <Steps
          steps={[
            "Open the note you want to lock.",
            "Press ⌘P (Ctrl+P on Linux) to open the list of commands. Then choose **Encrypt note**.",
            "Choose a password. The first time, you set one password for all your locked notes. You can also give this note a password of its own.",
          ]}
        />
      </Chapter>

      <Chapter id="limits" title="What it doesn't cover">
        <BulletList items={LIMITS} className="body measure text-ink-soft" />
      </Chapter>

      <Chapter id="privacy" title="The rest of your notes">
        <p className="body measure text-ink-soft">
          Gasp doesn&apos;t need an account. Your notes never pass through our servers. Once a day, Gasp tells us its
          version number and which kind of computer it runs on. It sends no name and no ID. We use this only to count
          how many people use Gasp. The{" "}
          <Link href={PRIVACY_PATH} className={proseLinkStyle}>
            privacy page
          </Link>{" "}
          shows exactly what Gasp sends and how to turn it off.
        </p>
      </Chapter>

      <Faq
        questions={[
          {
            question: "How does Gasp lock notes?",
            answer:
              "Gasp gives each locked note its own random key. It scrambles the note's text with that key, using a cipher called XChaCha20-Poly1305. Your password unlocks the key. Gasp never saves your password anywhere.",
          },
          {
            question: "Can iCloud or GitHub read my locked notes?",
            answer:
              "No. Gasp scrambles the note on your computer before it's saved. iCloud and GitHub only ever store the scrambled file. You can also hide the note's name.",
          },
          {
            question: "What happens if I forget my password?",
            answer:
              "Then nobody can open the note, including us. There's no copy of your password and no way around it. If you turned on Touch ID, the Mac where you turned it on can still unlock the note.",
          },
          {
            question: "Can my AI assistant read locked notes?",
            answer:
              "Only if you allow it for that note. For each note, you choose whether AI assistants can never read it, can read it while it's unlocked, or can always read it.",
          },
          {
            question: "Is note encryption free?",
            answer: "Yes. Locked notes are part of Gasp. Gasp is free on Mac and Linux.",
          },
        ]}
      />
      <TryGasp />
    </PageShell>
  );
}
