type Feature = { name: string; detail: string };

const FEATURES: Feature[] = [
  { name: "Search every note", detail: "⌘⇧F looks through the whole vault as you type." },
  { name: "MCP", detail: "Agents can search, read and write notes, and run the app's commands." },
  { name: "Keyboard first", detail: "Every command is in the ⌘P palette, and any shortcut can be rebound." },
  { name: "Math", detail: "LaTeX renders in place, with Latex Suite's snippets as you type." },
  { name: "Writing check", detail: "Spelling and grammar are checked on your device, offline." },
  { name: "HTML export", detail: "Any note becomes a standalone web page, math and all." },
  { name: "Free sync", detail: "Notes travel between devices through a GitHub repository you own." },
  { name: "Obsidian vaults", detail: "Open the folder you already have; its settings can come along." },
];

export function FeatureGrid() {
  return (
    <dl className="grid gap-x-16 sm:grid-cols-2">
      {FEATURES.map(({ name, detail }) => (
        <div key={name} className="border-t border-rule py-5">
          <dt className="subheading">{name}</dt>
          <dd className="body mt-1 text-ink-soft">{detail}</dd>
        </div>
      ))}
    </dl>
  );
}
