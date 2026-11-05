import { TEAM_QUESTION } from "./answers";
import type { Cell, Comparison, Measure } from "./comparison";

export type { Comparison } from "./comparison";

/** From the launch race on the home page (components/LaunchRace.tsx). */
const LAUNCH_METHOD =
  "We recorded the screen of a MacBook Pro with an M2 Pro chip. We quit each app first, then timed it from the moment we opened it until a note appeared.";

function launch(rival: number): Measure {
  return { name: "Time from opening the app to a note you can type in", unit: "s", gasp: 0.42, rival, method: LAUNCH_METHOD };
}

const FILES: Cell = { coverage: "yes", text: "Ordinary text files (Markdown) in a folder on your computer." };

const PLATFORMS: Cell = { coverage: "partly", text: "Mac and Linux today. Windows and iPhone apps are coming." };

const FREE_SYNC: Cell = { coverage: "yes", text: "Free. Gasp syncs through your own iCloud Drive or GitHub account." };

const LOCKED_NOTES: Cell = { coverage: "yes", text: "You can lock any note with a password. Nobody can read it without that password." };

const AI_ASSISTANTS: Cell = {
  coverage: "yes",
  text: "AI assistants like Claude, Cursor and Codex can read and edit your notes. You connect one with a click in Gasp's settings.",
};

/** How a team shares notes in Gasp: everyone syncs the same GitHub folder. */
const SHARED_NOTES: Cell = {
  coverage: "partly",
  text: "A team can share one folder of notes through GitHub. Each person's changes reach the others within a few minutes.",
};

const STEPS = {
  download: "Download Gasp and open it.",
  sync: "If you want your notes on more than one computer, turn on sync in Gasp's settings. On a Mac you can use iCloud Drive. On any computer you can use a free, private folder on GitHub, which Gasp sets up for you.",
};

const obsidian: Comparison = {
  slug: "obsidian",
  rival: "Obsidian",
  title: "Gasp vs Obsidian: a faster, private Obsidian alternative",
  description:
    "Gasp opens your Obsidian vault as it is. Obsidian has more plugins and a graph view. Gasp opens faster, syncs for free, locks notes and works with AI.",
  summary:
    "Obsidian and Gasp are both apps for writing notes. Both keep your notes as ordinary text files in a folder on your computer, which Obsidian calls a vault. Because of that, you can open your Obsidian vault in Gasp today and keep using Obsidian too. Obsidian has thousands of add-ons and a map of how your notes link together. Gasp opens much faster, syncs your notes for free, can lock notes with a password and lets AI assistants work in your notes.",
  reasons: {
    rival: [
      "Want to see your notes as a map of links (the graph view)",
      "Rely on add-ons that other people have written (there are over 9,000)",
      "Plan on a whiteboard (Canvas) or in database-style tables (Bases)",
      "Need an app for Windows or Android today",
      "Want to share a vault with other people through Obsidian's paid sync",
    ],
    gasp: [
      "Want your note on screen less than half a second after you open the app",
      "Write very long notes and want typing to stay instant",
      "Want to sync your notes without paying every month",
      "Want to lock private notes with a password",
      "Want an AI assistant like Claude to read and edit your notes",
    ],
  },
  measures: [
    launch(4.66),
    {
      name: "How much of the processor the app uses while a note is open",
      unit: "%",
      gasp: 0.9,
      rival: 1.7,
      method: "Measured on a Mac with one note open and nobody typing. Lower is better, because it leaves more of your computer free for other things.",
    },
  ],
  rows: [
    { topic: "Price", gasp: { coverage: "yes", text: "Free." }, rival: { coverage: "yes", text: "Free." } },
    { topic: "Your notes", gasp: FILES, rival: FILES },
    {
      topic: "Sync",
      gasp: FREE_SYNC,
      rival: {
        coverage: "partly",
        text: "Obsidian Sync costs $4 a month or more. You can also set up iCloud or another service yourself.",
      },
    },
    {
      topic: "Locked notes",
      gasp: LOCKED_NOTES,
      rival: {
        coverage: "partly",
        text: "Obsidian can't lock a note. Obsidian Sync scrambles your notes before they leave your computer, but the notes on your computer stay readable.",
      },
    },
    {
      topic: "AI assistants",
      gasp: AI_ASSISTANTS,
      rival: { coverage: "partly", text: "Possible through add-ons that other people have written." },
    },
    {
      topic: "Math",
      gasp: {
        coverage: "yes",
        text: "Equations written in LaTeX show as finished math while you type. Shortcuts for typing math are built in.",
      },
      rival: {
        coverage: "yes",
        text: "Equations written in LaTeX show as finished math. Shortcuts for typing math need an add-on.",
      },
    },
    {
      topic: "Add-ons",
      gasp: {
        coverage: "partly",
        text: "Gasp has no add-ons yet. You change how it works in its settings, or you ask an AI assistant to change them for you.",
      },
      rival: { coverage: "yes", text: "Over 9,000 add-ons and 900 themes, made by other people who use Obsidian." },
    },
    {
      topic: "Graph view, Canvas and Bases",
      gasp: {
        coverage: "no",
        text: "Gasp has none of these. Instead, the sidebar lists every note that links to the one you have open.",
      },
      rival: { coverage: "yes", text: "Obsidian has all three built in." },
    },
    {
      topic: "Computers and phones",
      gasp: PLATFORMS,
      rival: { coverage: "yes", text: "Mac, Windows, Linux, iPhone, iPad and Android." },
    },
    {
      topic: "Working with others",
      gasp: SHARED_NOTES,
      rival: { coverage: "partly", text: "Obsidian Sync, which is paid, can share a vault with other people." },
    },
  ],
  switching: {
    title: "Moving from Obsidian",
    steps: [
      STEPS.download,
      "Choose **Open a folder** and pick your Obsidian vault. Your vault is the folder with a hidden .obsidian folder inside it. If Gasp already has a vault open, use **Open another vault** instead.",
      "Gasp offers to **Import settings from Obsidian**. Say yes to bring over your keyboard shortcuts, fonts, accent color and attachment folder.",
      "You can keep Obsidian installed. When you save a note in one app, the other app shows the change.",
    ],
  },
  questions: [
    {
      question: "Can Gasp open my Obsidian vault?",
      answer:
        "Yes. Gasp opens the vault folder as it is. Links between notes, embedded notes, callouts, highlights, tags, math, footnotes and checklists all look the way they do in Obsidian. Gasp keeps its own settings in a separate .gasp folder. It never changes Obsidian's .obsidian folder.",
    },
    {
      question: "Do Obsidian plugins work in Gasp?",
      answer:
        "No. Instead, Gasp has many popular plugin features built in. These include math shortcuts, a table editor, footnote tools and a timer that tracks how long you spend writing each note. You can change how Gasp works in its settings, or ask an AI assistant to change the settings for you.",
    },
    {
      question: "Is Gasp faster than Obsidian?",
      answer:
        "Yes, in our test. We recorded both apps on a MacBook Pro with an M2 Pro chip. Gasp had a note ready to type in 0.42 seconds after we opened it. Obsidian took 4.66 seconds. One reason is that Gasp is a native app written in Rust. Obsidian is built on Electron, which runs the app inside a hidden web browser.",
    },
    {
      question: "How do I sync my vault without paying for Obsidian Sync?",
      answer:
        "Turn on sync in Gasp's settings. Gasp can sync your vault through your own iCloud Drive, or through a private folder on GitHub that Gasp sets up for you. Both are free. If you change the same line on two computers, Gasp shows both versions side by side. You pick the one to keep.",
    },
    {
      question: "Does Gasp have a graph view?",
      answer:
        "No. Instead, the sidebar shows the notes that link to the note you have open, the notes it links to, its headings and its tags.",
    },
  ],
};

const bear: Comparison = {
  slug: "bear",
  rival: "Bear",
  title: "Gasp vs Bear: Markdown files or a notes database",
  description:
    "Bear keeps notes in its own database on Apple devices. Gasp keeps them as ordinary text files on Mac and Linux, with free sync and free locked notes.",
  summary:
    "Bear is a notes app for the Mac, iPhone and iPad. It keeps your notes inside its own database, which only Bear can open. Gasp keeps every note as an ordinary text file in a folder. That means any other app can open your notes too. Gasp syncs notes and locks them with a password for free. Bear needs a paid Pro subscription to do both.",
  reasons: {
    rival: [
      "Need an iPhone and iPad app today",
      "Like to organize notes with tags inside tags",
      "Want to export notes as Word documents or images",
      "Love Bear's look, themes and typography",
    ],
    gasp: [
      "Want your notes as ordinary files that any app can open",
      "Use a Linux computer",
      "Want sync and locked notes without a subscription",
      "Already keep your notes in Obsidian",
      "Want an AI assistant to change how the app looks, as well as edit your notes",
    ],
  },
  rows: [
    {
      topic: "Price",
      gasp: { coverage: "yes", text: "Free." },
      rival: { coverage: "partly", text: "The basic app is free. Bear Pro costs $2.99 a month or $29.99 a year." },
    },
    {
      topic: "Your notes",
      gasp: FILES,
      rival: { coverage: "partly", text: "Kept in Bear's own database. You can export them as Markdown files." },
    },
    {
      topic: "Sync",
      gasp: FREE_SYNC,
      rival: { coverage: "partly", text: "Through iCloud, if you pay for Bear Pro." },
    },
    {
      topic: "Locked notes",
      gasp: {
        coverage: "yes",
        text: "Free. You can use one password for all your locked notes, or give a note its own password.",
      },
      rival: { coverage: "partly", text: "Only with Bear Pro. Each locked note has its own password." },
    },
    {
      topic: "AI assistants",
      gasp: {
        coverage: "yes",
        text: "AI assistants like Claude can edit your notes. They can also change Gasp's settings, colors and keyboard shortcuts.",
      },
      rival: { coverage: "yes", text: "AI assistants like Claude can read and edit your notes on a Mac." },
    },
    {
      topic: "Tags",
      gasp: { coverage: "yes", text: "Write #tags anywhere in a note. A sidebar lists every tag." },
      rival: { coverage: "yes", text: "Tags, including tags inside tags, are the main way to organize notes." },
    },
    {
      topic: "Export",
      gasp: { coverage: "yes", text: "PDF files and web pages." },
      rival: { coverage: "yes", text: "PDF files, web pages, Word documents and images, with Bear Pro." },
    },
    {
      topic: "Computers and phones",
      gasp: PLATFORMS,
      rival: { coverage: "partly", text: "Mac, iPhone and iPad." },
    },
  ],
  switching: {
    title: "Moving from Bear",
    steps: [
      "In Bear, select all your notes. Export them as Markdown files into a new folder.",
      STEPS.download,
      "In Gasp, choose **Open a folder** and pick the folder you exported to. Your Bear tags come along as #tags in the text. Gasp lists them all in its sidebar.",
      STEPS.sync,
    ],
  },
  questions: [
    {
      question: "Does Bear store notes as files?",
      answer:
        "No. Bear keeps notes in a database on each device. It makes Markdown files only when you export your notes. Gasp works with the files directly. That means any app that reads Markdown can open your Gasp notes.",
    },
    {
      question: "Are locked notes free in Gasp?",
      answer:
        "Yes. You can lock any note with a password. A locked note stays scrambled on your computer and in iCloud or GitHub. On a Mac, you can unlock it with Touch ID.",
    },
    {
      question: "Is there a Gasp app for iPhone?",
      answer: "Not yet. The iPhone app is being tested and isn't in the App Store. Today, Gasp runs on Mac and Linux.",
    },
    {
      question: "Will my Bear tags still work?",
      answer:
        "Yes. When you export from Bear, your tags stay in your notes as #tags. Gasp lists every tag in its sidebar. You can also search for a tag by typing tag: and the tag's name.",
    },
  ],
};

const typora: Comparison = {
  slug: "typora",
  rival: "Typora",
  title: "Gasp vs Typora: free Markdown editors compared",
  description:
    "Typora and Gasp both hide Markdown symbols as you type. Typora exports to Word and more. Gasp adds linked notes, free sync, locked notes and AI.",
  summary:
    "Typora and Gasp are both apps for writing in Markdown. Markdown is plain text with a few simple symbols for formatting, like # in front of a heading. Both apps hide those symbols as you type, so your writing looks finished while you write it. Typora is built for writing one document at a time and exporting it to many formats. Gasp is built for a whole folder of notes that link to each other. Gasp also syncs your notes, searches all of them and can lock private ones. Gasp is free. Typora costs $14.99.",
  reasons: {
    rival: [
      "Need a Windows app today",
      "Export documents to Word, LaTeX or EPUB e-books",
      "Write one document at a time, without a folder of notes",
      "Want to restyle the app with your own CSS",
    ],
    gasp: [
      "Keep many notes that link to each other",
      "Want sync built into the app, for free",
      "Want to lock private notes with a password",
      "Want to search every note as you type",
      "Want an AI assistant to work in your notes",
    ],
  },
  rows: [
    {
      topic: "Price",
      gasp: { coverage: "yes", text: "Free." },
      rival: { coverage: "partly", text: "$14.99, paid once, for up to three computers." },
    },
    { topic: "Your notes", gasp: FILES, rival: FILES },
    {
      topic: "Formatting as you type",
      gasp: {
        coverage: "yes",
        text: "Gasp hides the Markdown symbols everywhere except near your cursor. You can also show them all the time, or hide them all the time.",
      },
      rival: { coverage: "yes", text: "Typora hides the Markdown symbols as you type." },
    },
    {
      topic: "Links between notes",
      gasp: {
        coverage: "yes",
        text: "You link notes with [[double brackets]]. Each note shows which other notes link to it.",
      },
      rival: {
        coverage: "partly",
        text: "Ordinary web-style links work. Typora doesn't show which notes link to a note.",
      },
    },
    {
      topic: "Sync",
      gasp: { coverage: "yes", text: "Built in, through iCloud Drive or GitHub." },
      rival: {
        coverage: "partly",
        text: "Typora has no sync of its own. You can keep your files in iCloud Drive, Dropbox or OneDrive yourself.",
      },
    },
    { topic: "Locked notes", gasp: LOCKED_NOTES, rival: { coverage: "no", text: "Typora can't lock notes." } },
    { topic: "AI assistants", gasp: AI_ASSISTANTS, rival: { coverage: "no", text: "Typora doesn't connect to AI assistants." } },
    {
      topic: "Export",
      gasp: { coverage: "partly", text: "PDF files and web pages." },
      rival: { coverage: "yes", text: "PDF files, web pages, Word documents, LaTeX, EPUB e-books and more." },
    },
    {
      topic: "Computers and phones",
      gasp: { coverage: "partly", text: "Mac and Linux today. A Windows app is coming." },
      rival: { coverage: "yes", text: "Mac, Windows and Linux." },
    },
  ],
  switching: {
    title: "Moving from Typora",
    steps: [
      STEPS.download,
      "Your Typora files are already Markdown. In Gasp, choose **Open a folder** and pick the folder that holds them.",
      STEPS.sync,
    ],
  },
  questions: [
    {
      question: "Is Gasp a free alternative to Typora?",
      answer:
        "Yes. Gasp is free on Mac and Linux. You don't need an account to use it. Like Typora, Gasp formats Markdown as you type and saves ordinary .md files.",
    },
    {
      question: "Can Gasp export to Word?",
      answer:
        "Not yet. Gasp exports PDF files and web pages. If you often need to send Word documents, Typora is the better choice for that.",
    },
    {
      question: "Does Gasp hide Markdown symbols like Typora does?",
      answer:
        "Yes. By default, Gasp shows the symbols only where your cursor is. You can also show them all the time, or hide them everywhere. You can make this choice separately for each kind of formatting.",
    },
  ],
};

const appleNotes: Comparison = {
  slug: "apple-notes",
  rival: "Apple Notes",
  title: "Gasp vs Apple Notes: quick notes or Markdown files",
  description:
    "Apple Notes is free and great for sketches and sharing. Gasp keeps notes as ordinary text files, opens in 0.42 s, runs on Linux and works with AI.",
  summary:
    "Apple Notes comes free on every iPhone, iPad and Mac. It's great for quick notes, drawings and lists you share with family. Gasp is for writing you want to keep as ordinary text files that any app can open. Gasp opens faster, runs on Linux and lets AI assistants like Claude work in your notes.",
  reasons: {
    rival: [
      "Draw or handwrite with an Apple Pencil",
      "Share notes and edit them with other people at the same moment",
      "Need your notes on an iPhone, an iPad and the web today",
      "Want to jot down lists without thinking about files",
    ],
    gasp: [
      "Want your notes as files that you own and can move to any app",
      "Write longer documents with math, tables and footnotes",
      "Use a Linux computer",
      "Want an AI assistant like Claude to read and edit your notes",
      "Want a note ready less than half a second after you open the app",
    ],
  },
  measures: [launch(1.53)],
  rows: [
    {
      topic: "Price",
      gasp: { coverage: "yes", text: "Free." },
      rival: { coverage: "yes", text: "Free. It comes with every Apple device." },
    },
    {
      topic: "Your notes",
      gasp: FILES,
      rival: { coverage: "partly", text: "Kept inside the Notes app. You can copy a note out as Markdown." },
    },
    {
      topic: "Markdown",
      gasp: { coverage: "yes", text: "You write in Markdown. Gasp saves your notes as Markdown." },
      rival: { coverage: "partly", text: "When you import a Markdown file, Notes turns it into its own format." },
    },
    {
      topic: "Sync",
      gasp: { coverage: "yes", text: "Through iCloud Drive or GitHub." },
      rival: { coverage: "yes", text: "Through iCloud." },
    },
    {
      topic: "Locked notes",
      gasp: { coverage: "yes", text: "Lock a note with a password. On a Mac, you can unlock it with Touch ID." },
      rival: { coverage: "yes", text: "Lock a note with your device's password or a separate password." },
    },
    {
      topic: "Working with others",
      gasp: SHARED_NOTES,
      rival: { coverage: "yes", text: "You can share a note with other people. Everyone can edit it at the same moment." },
    },
    {
      topic: "Handwriting",
      gasp: { coverage: "no", text: "Gasp is for typing. It doesn't support drawing or handwriting." },
      rival: { coverage: "yes", text: "You can draw and handwrite with an Apple Pencil." },
    },
    {
      topic: "AI assistants",
      gasp: AI_ASSISTANTS,
      rival: {
        coverage: "partly",
        text: "Apple's Writing Tools can rewrite and summarize text. There's no official way for other AI assistants to reach your notes.",
      },
    },
    {
      topic: "Computers and phones",
      gasp: { coverage: "partly", text: "Mac and Linux today. An iPhone app is coming." },
      rival: { coverage: "yes", text: "iPhone, iPad, Mac, and any web browser through iCloud.com." },
    },
  ],
  switching: {
    title: "Moving from Apple Notes",
    steps: [
      STEPS.download,
      "In Gasp, make a new vault, which is a folder for your notes. You can also open a folder where you already keep notes.",
      "On a Mac with macOS Tahoe or later, open a note in Apple Notes. Choose Edit › **Copy as Markdown**.",
      "Make a new note in Gasp and paste. Do this for each note you want to bring over.",
    ],
  },
  questions: [
    {
      question: "Is Gasp faster than Apple Notes?",
      answer:
        "Yes, in our test. We recorded both apps on a MacBook Pro with an M2 Pro chip. Gasp had a note ready to type in 0.42 seconds after we opened it. Apple Notes took 1.53 seconds.",
    },
    {
      question: "Can I keep my Gasp notes in iCloud?",
      answer:
        "Yes. In Gasp's settings, choose Sync with iCloud. Gasp copies your notes into a folder called Gasp in your iCloud Drive. Your notes stay ordinary files that you can see in Finder.",
    },
    {
      question: "Can Gasp lock notes like Apple Notes does?",
      answer:
        "Yes. When you lock a note in Gasp, it's scrambled on your computer and in iCloud. You open it with your password. On a Mac, you can also turn on Touch ID to open it.",
    },
  ],
};

const logseq: Comparison = {
  slug: "logseq",
  rival: "Logseq",
  title: "Gasp vs Logseq: an outliner or a Markdown editor",
  description:
    "Logseq is an outliner where everything is a bullet point. Gasp is a fast writing app for documents, with free sync, locked notes and AI.",
  summary:
    "Logseq is an outliner. In Logseq, you write everything as bullet points. You can link to any bullet, move it around and search for it on its own. Gasp works like a regular writing app. You write paragraphs and headings, the way you would in a document. The right choice depends on how you like to write.",
  reasons: {
    rival: [
      "Like to think and write in nested bullet points",
      "Keep a daily journal of small, linked notes",
      "Want to mark up PDFs next to your notes",
      "Need a Windows app or a web app",
      "Want to sort and search notes by fields you define",
    ],
    gasp: [
      "Write in paragraphs and headings",
      "Want a finished, stable app that saves ordinary text files",
      "Want free sync today",
      "Want to lock private notes with a password",
      "Already keep your notes in Obsidian",
    ],
  },
  rows: [
    {
      topic: "Price",
      gasp: { coverage: "yes", text: "Free." },
      rival: { coverage: "yes", text: "Free. Its code is open source." },
    },
    {
      topic: "How you write",
      gasp: { coverage: "yes", text: "Documents with headings, paragraphs and lists." },
      rival: { coverage: "yes", text: "Everything is an outline of bullet points." },
    },
    {
      topic: "Your notes",
      gasp: FILES,
      rival: {
        coverage: "partly",
        text: "The classic version, Logseq OG, saves text files but gets no new features. The new version saves to a database and is still being tested.",
      },
    },
    {
      topic: "Sync",
      gasp: FREE_SYNC,
      rival: { coverage: "partly", text: "Logseq Sync is only offered to supporters right now. It has no public price yet." },
    },
    { topic: "Locked notes", gasp: LOCKED_NOTES, rival: { coverage: "no", text: "Logseq can't lock notes." } },
    {
      topic: "AI assistants",
      gasp: AI_ASSISTANTS,
      rival: { coverage: "partly", text: "The new version, which is still being tested, can connect to AI assistants." },
    },
    {
      topic: "Working with others",
      gasp: SHARED_NOTES,
      rival: { coverage: "partly", text: "Editing with other people at the same moment is in early testing." },
    },
    {
      topic: "Computers and phones",
      gasp: PLATFORMS,
      rival: { coverage: "yes", text: "Mac, Windows, Linux and the web. Phone apps are being tested." },
    },
  ],
  switching: {
    title: "Moving from Logseq",
    steps: [
      STEPS.download,
      "In Logseq OG, your notes (Logseq calls them a graph) are a folder of Markdown files. In Gasp, choose **Open a folder** and pick that folder. Your pages are in a folder called pages. Your journal entries are in a folder called journals.",
      "Your bullet points stay as bullet lists. Links to pages, written [[like this]], keep working. Links to single bullets, written ((like this)), show as plain text in Gasp.",
      STEPS.sync,
    ],
  },
  questions: [
    {
      question: "Can Gasp open my Logseq notes?",
      answer:
        "Yes, if you use Logseq OG, which saves notes as Markdown files. Your pages, journals, lists and page links open as they are. Links to single bullets and Logseq's search queries don't work in Gasp.",
    },
    {
      question: "Is Gasp an outliner?",
      answer:
        "No. Gasp is for writing documents. You can still make bullet lists and nest them with the Tab key. You can also fold any heading to hide the text under it.",
    },
    {
      question: "Is Gasp's sync free?",
      answer:
        "Yes. Gasp syncs your notes through your own iCloud Drive or through a private folder on GitHub. Both are free.",
    },
  ],
};

const notion: Comparison = {
  slug: "notion",
  rival: "Notion",
  title: "Gasp vs Notion: a private, local Notion alternative",
  description:
    "Notion is a shared online workspace for teams. Gasp keeps your notes on your own computer, locks private ones, works offline and is free.",
  summary:
    "Notion is a website and app where teams write pages together. Your pages live on Notion's computers, called servers. Several people can edit the same page at the same moment. Gasp is an app on your own computer. It keeps your notes as ordinary text files on your device. It can also lock private notes with a password. A small team can also share notes in Gasp through GitHub.",
  reasons: {
    rival: [
      "Edit pages with your team at the same moment",
      "Build databases, boards and forms",
      "Need apps for Windows and Android, or a web version, today",
      "Want an AI assistant built into the app itself",
    ],
    gasp: [
      "Want your notes stored on your own computer",
      "Want private notes locked so that only you can read them",
      "Want to write offline, with nothing to download first",
      "Want to share notes with a small team through GitHub",
      "Write long documents with math and footnotes",
      "Use a Linux computer",
    ],
  },
  rows: [
    {
      topic: "Price",
      gasp: { coverage: "yes", text: "Free." },
      rival: { coverage: "partly", text: "There's a free plan. The Plus plan costs $10 per person per month, paid yearly." },
    },
    {
      topic: "Where your notes live",
      gasp: { coverage: "yes", text: "On your own computer, as ordinary text files (Markdown)." },
      rival: { coverage: "partly", text: "On Notion's servers. You can export your pages as Markdown files." },
    },
    {
      topic: "Working offline",
      gasp: { coverage: "yes", text: "Everything works offline, because every note is a file on your computer." },
      rival: {
        coverage: "partly",
        text: "Only in the desktop and phone apps. You have to save each page for offline use before you go offline.",
      },
    },
    {
      topic: "Locked notes",
      gasp: {
        coverage: "yes",
        text: "When you lock a note, Gasp scrambles it on your computer before it syncs. Nobody without your password can read it.",
      },
      rival: {
        coverage: "partly",
        text: "Notion encrypts pages on its servers. Notion holds the keys to that encryption, not you.",
      },
    },
    {
      topic: "Account",
      gasp: { coverage: "yes", text: "You don't need an account." },
      rival: { coverage: "no", text: "You need a Notion account." },
    },
    {
      topic: "AI assistants",
      gasp: {
        coverage: "yes",
        text: "You can use the AI assistant you already have. Claude, Cursor or Codex can read and edit your notes.",
      },
      rival: {
        coverage: "yes",
        text: "Notion AI is built in on paid plans. Other AI assistants can connect through Notion's own connector.",
      },
    },
    {
      topic: "Working with others",
      gasp: SHARED_NOTES,
      rival: {
        coverage: "yes",
        text: "Several people can edit a page at the same moment. They can also leave comments and share pages.",
      },
    },
    {
      topic: "Databases",
      gasp: {
        coverage: "partly",
        text: "Gasp has tables that you edit like a spreadsheet. It has no databases, boards or calendars.",
      },
      rival: { coverage: "yes", text: "Tables, boards, calendars and forms that you can sort and filter." },
    },
    {
      topic: "Computers and phones",
      gasp: PLATFORMS,
      rival: { coverage: "yes", text: "Mac, Windows, iPhone, Android and any web browser." },
    },
  ],
  switching: {
    title: "Moving from Notion",
    steps: [
      "In Notion, open Settings and choose to export all workspace content. Pick the Markdown & CSV format. Notion gives you a .zip file. Open it to get a folder.",
      STEPS.download,
      "In Gasp, choose **Open a folder** and pick that folder. Your pages open as notes. Your databases become spreadsheet files (CSV). They stay in the same folder.",
      STEPS.sync,
    ],
  },
  questions: [
    {
      question: "Is Gasp a good Notion alternative?",
      answer:
        "It is if you want your notes on your own computer and your private notes locked with a password. Notion is the better choice if your team edits the same pages at the same moment, or if you rely on databases.",
    },
    TEAM_QUESTION,
    {
      question: "Can Notion read my notes?",
      answer:
        "Notion encrypts your pages on its servers. However, Notion holds the keys to that encryption, not you. Gasp never sees your notes at all. Your notes sync through your own iCloud or GitHub account. Notes you lock in Gasp are scrambled on your computer before they leave it.",
    },
    {
      question: "Does Gasp work offline?",
      answer:
        "Yes. Every note is a file on your computer. You can write anywhere, even without internet. When you're back online, Gasp syncs your changes.",
    },
  ],
};

const iaWriter: Comparison = {
  slug: "ia-writer",
  rival: "iA Writer",
  title: "Gasp vs iA Writer: two Markdown editors compared",
  description:
    "iA Writer is a focused writing app. Gasp is free, runs on Linux and adds linked notes, math, locked notes and AI assistants.",
  summary:
    "iA Writer and Gasp are both apps for writing in Markdown. Both save your writing as ordinary text files in folders you choose. iA Writer is built for focused writing. It can dim everything except the sentence you're writing. It also points out filler words. Gasp is built for a whole folder of notes that link to each other. It adds math, sync and locked notes. Gasp is free. iA Writer costs $49.99 on a Mac.",
  reasons: {
    rival: [
      "Need a Windows, iPhone or iPad app today",
      "Want the app to dim everything except the sentence you're writing",
      "Want the app to point out filler words and clichés",
      "Want text pasted from AI tools shown in a different color",
    ],
    gasp: [
      "Want notes that link to each other, like in Obsidian",
      "Write math",
      "Want to lock private notes with a password",
      "Use a Linux computer",
      "Don't want to pay for a writing app",
    ],
  },
  rows: [
    {
      topic: "Price",
      gasp: { coverage: "yes", text: "Free." },
      rival: { coverage: "partly", text: "$49.99, paid once, on a Mac. $29.99, paid once, on Windows." },
    },
    {
      topic: "Your notes",
      gasp: { coverage: "yes", text: "Ordinary text files (Markdown) in a folder you choose." },
      rival: { coverage: "yes", text: "Ordinary text files (Markdown) in folders you choose." },
    },
    {
      topic: "Sync",
      gasp: { coverage: "yes", text: "Built in, through iCloud Drive or GitHub." },
      rival: { coverage: "yes", text: "Through iCloud, Dropbox, Google Drive or OneDrive." },
    },
    { topic: "Locked notes", gasp: LOCKED_NOTES, rival: { coverage: "no", text: "iA Writer can't lock notes." } },
    {
      topic: "Writing help",
      gasp: {
        coverage: "partly",
        text: "Gasp checks spelling and grammar without the internet. It can also color sentences by how long they are.",
      },
      rival: {
        coverage: "yes",
        text: "Focus Mode dims everything except your current sentence. Style Check points out filler words and clichés.",
      },
    },
    {
      topic: "AI assistants",
      gasp: AI_ASSISTANTS,
      rival: {
        coverage: "no",
        text: "iA Writer doesn't connect to AI assistants. It marks text that you paste from AI tools.",
      },
    },
    {
      topic: "Computers and phones",
      gasp: PLATFORMS,
      rival: { coverage: "partly", text: "Mac, Windows, iPhone and iPad." },
    },
  ],
  switching: {
    title: "Moving from iA Writer",
    steps: [
      STEPS.download,
      "Your iA Writer files are already Markdown. In Gasp, choose **Open a folder** and pick the folder where iA Writer saves your writing.",
      "If that folder is in iCloud Drive or Dropbox, it keeps syncing the same way. Gasp can also sync it through GitHub instead.",
    ],
  },
  questions: [
    {
      question: "Is Gasp a free alternative to iA Writer?",
      answer:
        "Yes, on Mac and Linux. Both apps save ordinary Markdown files. You can open the same folder in either app.",
    },
    {
      question: "Does Gasp have a focus mode?",
      answer:
        "Not like iA Writer's. Gasp can color each sentence by how long it is. Long sentences show in one color and short ones in another. Gasp also checks your spelling and grammar without the internet.",
    },
  ],
};

export const COMPARISONS: Comparison[] = [obsidian, bear, notion, appleNotes, typora, iaWriter, logseq];

export function comparisonOf(slug: string) {
  return COMPARISONS.find((comparison) => comparison.slug === slug);
}
