import { latestRelease } from "@/lib/releases";
import { RELEASES_URL } from "@/lib/site";
import { countDownload } from "@/lib/tally";

export const dynamic = "force-dynamic";

async function countQuietly(): Promise<void> {
  try {
    await countDownload();
  } catch {
    // A download never waits on, or fails with, the counter.
  }
}

/** Counts the download, then sends the browser to the newest DMG, or to
    the releases page when GitHub can't say which that is. */
export async function GET(): Promise<Response> {
  const [release] = await Promise.all([latestRelease(), countQuietly()]);
  return Response.redirect(release?.dmgUrl ?? RELEASES_URL, 302);
}
