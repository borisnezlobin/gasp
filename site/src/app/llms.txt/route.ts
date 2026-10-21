import { COMPARISON_LINKS, FEATURE_LINKS, type PageLink } from "@/lib/pages";
import { DOWNLOAD_PATH, PRIVACY_PATH, SITE_URL } from "@/lib/site";

export const dynamic = "force-static";

function list(links: PageLink[]) {
  return links.map((link) => `- [${link.name}](${SITE_URL}${link.path}): ${link.summary}`).join("\n");
}

/** A plain summary of Gasp for AI answer engines, as llmstxt.org lays it out. */
export function GET() {
  const text = `# Gasp

> Gasp is a free app for writing notes on a Mac or a Linux computer. It keeps notes as ordinary Markdown text files in a folder. It opens Obsidian vaults as they are. It syncs notes for free through the user's own iCloud Drive or GitHub account. It can lock private notes with a password. AI assistants like Claude can read and edit notes through its built-in MCP server. It needs no account.

- Price: free, with no paid tier.
- Platforms: macOS 12 or later (Apple silicon and Intel) and 64-bit Linux. Windows and iPhone are on the way.
- Notes: plain Markdown files in a folder you choose.

## Features

${list(FEATURE_LINKS)}

## Comparisons

${list(COMPARISON_LINKS)}

## More

- [Download](${SITE_URL}${DOWNLOAD_PATH}): the Mac disk image and the Linux .deb and tarball.
- [Privacy](${SITE_URL}${PRIVACY_PATH}): exactly what the app sends, and how to turn it off.
`;
  return new Response(text, { headers: { "Content-Type": "text/plain; charset=utf-8" } });
}
