import { versionAnswer } from "@/lib/versionAnswer";

export const dynamic = "force-static";

/** As for the Mac's, ten minutes. */
export const revalidate = 600;

/** The newest stable release the Linux app's update check offers: its
    tarball, which a copy installed from the tarball replaces itself
    from. Nothing about the request is logged or counted. */
export async function GET(): Promise<Response> {
  return versionAnswer("linux", revalidate);
}
