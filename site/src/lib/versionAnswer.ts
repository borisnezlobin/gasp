import { gitHubReleases, newestStableVersion, type Installer } from "./releases";

/** The newest stable release with `installer`, as the app's update check
    reads it. Nothing about the request is logged or counted. */
export async function versionAnswer(installer: Installer, cacheSeconds: number): Promise<Response> {
  const releases = await gitHubReleases(cacheSeconds);
  if (releases === null) {
    return Response.json({ error: "GitHub couldn't be reached." }, { status: 503 });
  }
  const newest = newestStableVersion(releases, installer);
  if (newest === null) {
    return Response.json({ error: "No release is published yet." }, { status: 404 });
  }
  return Response.json(newest);
}
