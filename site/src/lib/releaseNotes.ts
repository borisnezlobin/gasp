/** The small part of Markdown that release notes on GitHub use. */

export type NoteSpan = { kind: "text" | "strong" | "code"; text: string } | { kind: "link"; text: string; href: string };

export type NoteBlock =
  | { kind: "heading"; spans: NoteSpan[] }
  | { kind: "list"; items: NoteSpan[][] }
  | { kind: "paragraph"; spans: NoteSpan[] };

const SPAN_PATTERN = /(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)\s]+\))/g;
const LINK_PATTERN = /^\[([^\]]+)\]\(([^)\s]+)\)$/;
const HEADING_PATTERN = /^#{1,6}\s+/;
const BULLET_PATTERN = /^\s*[-*]\s+/;

function spanOf(token: string): NoteSpan {
  const link = LINK_PATTERN.exec(token);
  if (link) return { kind: "link", text: link[1], href: link[2] };
  if (token.startsWith("**")) return { kind: "strong", text: token.slice(2, -2) };
  if (token.startsWith("`")) return { kind: "code", text: token.slice(1, -1) };
  return { kind: "text", text: token };
}

/** Splitting on a capturing pattern alternates plain text and marked-up
    runs, so odd places hold the runs. */
export function parseSpans(text: string): NoteSpan[] {
  return text
    .split(SPAN_PATTERN)
    .map((part, index) => (index % 2 === 1 ? spanOf(part) : { kind: "text" as const, text: part }))
    .filter((span) => span.text !== "");
}

type Section = { heading: string | null; lines: string[] };

function sectionsOf(lines: string[]): Section[] {
  const sections: Section[] = [{ heading: null, lines: [] }];
  for (const line of lines) {
    if (HEADING_PATTERN.test(line)) sections.push({ heading: line.replace(HEADING_PATTERN, ""), lines: [] });
    else sections[sections.length - 1].lines.push(line);
  }
  return sections;
}

/** Install steps point at files "below" on GitHub, which this page doesn't
    have, and the download button already covers them. */
function isInstallStep(section: Section): boolean {
  return section.heading?.trim().toLowerCase() === "install";
}

function isDownloadSentence(paragraph: string): boolean {
  return /^download\b/i.test(paragraph);
}

function blocksOfLines(lines: string[]): NoteBlock[] {
  const blocks: NoteBlock[] = [];
  let paragraph: string[] = [];
  const closeParagraph = () => {
    const text = paragraph.join(" ").trim();
    if (text && !isDownloadSentence(text)) blocks.push({ kind: "paragraph", spans: parseSpans(text) });
    paragraph = [];
  };
  for (const line of lines) {
    if (BULLET_PATTERN.test(line)) {
      closeParagraph();
      const item = parseSpans(line.replace(BULLET_PATTERN, ""));
      const last = blocks[blocks.length - 1];
      if (last?.kind === "list") last.items.push(item);
      else blocks.push({ kind: "list", items: [item] });
    } else if (line.trim() === "") {
      closeParagraph();
    } else {
      paragraph.push(line.trim());
    }
  }
  closeParagraph();
  return blocks;
}

export function parseReleaseNotes(body: string | null): NoteBlock[] {
  const lines = (body ?? "").replace(/\r\n?/g, "\n").split("\n");
  return sectionsOf(lines)
    .filter((section) => !isInstallStep(section))
    .flatMap((section) => {
      const blocks = blocksOfLines(section.lines);
      if (section.heading === null || blocks.length === 0) return blocks;
      return [{ kind: "heading" as const, spans: parseSpans(section.heading) }, ...blocks];
    });
}
