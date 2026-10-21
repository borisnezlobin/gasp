import { AgentDemo } from "@/components/AgentDemo";
import { Faq } from "@/components/Faq";
import { Chapter, CodeBlock, PageShell, PageTitle } from "@/components/PageShell";
import { Steps } from "@/components/Steps";
import { TryGasp } from "@/components/TryGasp";
import { pageMetadata } from "@/lib/metadata";
import { FEATURES_PATH } from "@/lib/site";

const PATH = `${FEATURES_PATH}/ai-agents`;

export const metadata = pageMetadata({
  path: PATH,
  title: "A Markdown editor for AI agents, with MCP built in | Gasp",
  description:
    "Connect Claude, Claude Code, Cursor or Codex to your notes in one click. Your AI assistant can read and edit notes, and change how Gasp looks.",
});

/** The tools in crates/mcp/src/tools, grouped as the server lists them. */
const TOOLS: { group: string; detail: string; names: string[] }[] = [
  {
    group: "Notes",
    detail: "Find, read, write and move notes. Deleting a note moves it to the trash, where you can get it back.",
    names: ["list_notes", "read_note", "search", "create_note", "write_note", "patch_note", "move_note", "delete_note"],
  },
  {
    group: "Attachments",
    detail: "Pictures and other files in your notes folder.",
    names: ["list_attachments", "read_attachment", "write_attachment", "move_attachment", "delete_attachment"],
  },
  {
    group: "Links",
    detail: "Which notes link to which, and every tag you use.",
    names: ["backlinks", "outgoing_links", "tags"],
  },
  {
    group: "Gasp itself",
    detail: "Settings, colors, fonts, keyboard shortcuts, toolbars and typing shortcuts.",
    names: [
      "get_settings",
      "set_setting",
      "get_theme",
      "set_theme_token",
      "get_rules",
      "set_rules",
      "get_toolbars",
      "set_toolbars",
      "get_snippets",
      "set_snippets",
      "get_replacements",
      "set_replacements",
      "list_commands",
    ],
  },
  {
    group: "The app",
    detail: "Which notes are open, where your cursor is and any command in Gasp's menus. It can also preview a page of a note as it would look printed.",
    names: ["editor_state", "run_command", "open_note", "render_note"],
  },
];

function ToolGroups() {
  return (
    <dl className="divide-y divide-rule">
      {TOOLS.map((tools) => (
        <div key={tools.group} className="grid gap-2 py-5 first:pt-0 sm:grid-cols-[12rem_1fr] sm:gap-6">
          <dt className="subheading">{tools.group}</dt>
          <dd>
            <p className="body text-ink-soft">{tools.detail}</p>
            <p className="mt-3 flex flex-wrap gap-1.5">
              {tools.names.map((name) => (
                <code key={name} className="code rounded-md bg-fill px-2 py-0.5 text-ink-soft">
                  {name}
                </code>
              ))}
            </p>
          </dd>
        </div>
      ))}
    </dl>
  );
}

export default function AiAgents() {
  return (
    <PageShell width="medium">
      <PageTitle title="AI assistants">
        <p>
          AI assistants like Claude, Cursor and Codex can work inside your notes in Gasp. They can find notes, write
          and edit them, and even change how Gasp looks. You keep using the AI app you already have. Gasp connects to
          it through MCP, a standard way for AI apps to use other apps.
        </p>
      </PageTitle>

      <Chapter
        id="demo"
        title="Things you can ask"
        intro="Pick a request. You'll see the steps the assistant takes inside Gasp, and then its answer."
      >
        <AgentDemo />
      </Chapter>

      <Chapter id="connect" title="Connecting your assistant">
        <Steps
          steps={[
            "Open Gasp's settings and go to the General page.",
            "Under **Agent access**, click the AI app you use: Claude, Claude Code, Cursor or Codex. Gasp connects itself to that app.",
            "If the AI app was already open, restart it. Then ask it about your notes.",
          ]}
        />
        <p className="body mt-8 measure text-ink-soft">
          Other AI apps that support MCP can connect too. Tell the app to run this command, with the path to your notes
          folder at the end:
        </p>
        <div className="mt-4">
          <CodeBlock>gasp mcp ~/Documents/Notes</CodeBlock>
        </div>
      </Chapter>

      <Chapter
        id="tools"
        title="What an assistant can do"
        intro="Gasp gives an assistant 33 tools. Each tool is one thing it can do, like reading a note or changing a color."
      >
        <ToolGroups />
      </Chapter>

      <Chapter id="limits" title="What an assistant can't do">
        <p className="body measure text-ink-soft">
          An assistant can only reach files inside your notes folder. It can&apos;t open the hidden folders where git
          and the trash keep their files. It can change Gasp&apos;s settings only through Gasp&apos;s own tools. Locked
          notes stay closed to assistants, unless you open a note to them yourself.
        </p>
      </Chapter>

      <Faq
        questions={[
          {
            question: "Which AI apps work with Gasp?",
            answer:
              "Gasp connects to Claude, Claude Code, Cursor and Codex with one click. Any other AI app that supports MCP can connect with a short command.",
          },
          {
            question: "What is MCP?",
            answer:
              "MCP stands for Model Context Protocol. It's a standard way for AI apps to use other apps. Gasp uses it to let your AI assistant read your notes and change Gasp's settings.",
          },
          {
            question: "Can an assistant see changes I haven't saved yet?",
            answer:
              "Yes, while Gasp is open. The assistant sees what's in the editor right now. When it makes an edit, you can undo the whole edit in one step.",
          },
          {
            question: "Does Gasp send my notes to an AI company?",
            answer:
              "No. Gasp has no AI of its own. Your notes only reach an AI company when you connect an assistant and ask it to read them.",
          },
          {
            question: "Can an assistant delete my notes?",
            answer:
              "An assistant can move a note to the trash, just like you can. It can't delete anything for good. You can get the note back from the trash.",
          },
        ]}
      />
      <TryGasp />
    </PageShell>
  );
}
