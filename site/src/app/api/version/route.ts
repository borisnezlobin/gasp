import { gitHubReleases, newestStableVersion } from "@/lib/releases";

export const dynamic = "force-static";

/** Ten minutes, so a new release reaches the app soon after it's published
    while GitHub sees at most a few requests an hour. */
export const revalidate = 600;

/** The newest stable release the Mac app's update check offers. Nothing
    about the request is logged or counted. */
export async function GET(): Promise<Response> {
  const releases = await gitHubReleases(revalidate);
  if (releases === null) {
    return Response.json({ error: "GitHub couldn't be reached." }, { status: 503 });
  }
  const newest = newestStableVersion(releases);
  if (newest === null) {
    return Response.json({ error: "No release is published yet." }, { status: 404 });
  }
  return Response.json(newest);
}
