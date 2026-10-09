import { INSTALLERS, latestDownload, type Installer } from "@/lib/releases";
import { DOWNLOAD_PATH } from "@/lib/site";
import { countDownload } from "@/lib/tally";

export const dynamic = "force-dynamic";

function isInstaller(name: string): name is Installer {
  return (INSTALLERS as readonly string[]).includes(name);
}

async function countQuietly(installer: Installer): Promise<void> {
  try {
    await countDownload(installer);
  } catch {
    // A download never waits on, or fails with, the counter.
  }
}

/** `/download/mac`, `/download/linux` (the tarball) and `/download/deb`:
    counts the download, then sends the browser to the newest release's
    file. Anything else, or a file no release has yet, goes to the
    download page. */
export async function GET(request: Request, { params }: { params: Promise<{ installer: string }> }): Promise<Response> {
  const { installer } = await params;
  const page = new URL(DOWNLOAD_PATH, request.url);
  if (!isInstaller(installer)) return Response.redirect(page, 302);
  const found = await latestDownload(installer);
  if (!found) return Response.redirect(page, 302);
  await countQuietly(installer);
  return Response.redirect(found.file.url, 302);
}
