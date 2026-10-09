import { versionAnswer } from "@/lib/versionAnswer";

export const dynamic = "force-static";

/** Ten minutes, so a new release reaches the app soon after it's published
    while GitHub sees at most a few requests an hour. */
export const revalidate = 600;

/** The newest stable release the Mac app's update check offers: its disk
    image. Nothing about the request is logged or counted. */
export async function GET(): Promise<Response> {
  return versionAnswer("mac", revalidate);
}
