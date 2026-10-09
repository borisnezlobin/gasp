/** The desktop system a visitor is on, as far as the browser says. Phones,
    tablets and Chromebooks are `other`: Gasp has nothing they can install
    from here. */
export type VisitorSystem = "mac" | "linux" | "windows" | "other";

/** What the browser says about its system: Chromium's client hint when
    there is one (`macOS`, `Linux`, `Windows`, `Android`, `Chrome OS`),
    the user agent, and how many touch points the screen has, which tells
    an iPad, whose Safari says it's a Mac, from a Mac. */
export type SystemHints = {
  platform?: string;
  userAgent: string;
  maxTouchPoints?: number;
};

export function visitorSystem({ platform, userAgent, maxTouchPoints = 0 }: SystemHints): VisitorSystem {
  const said = `${platform ?? ""} ${userAgent}`.toLowerCase();
  if (/android|iphone|ipad|ipod|cros|chrome os|mobile/.test(said)) return "other";
  if (/mac/.test(said)) return maxTouchPoints > 1 ? "other" : "mac";
  if (/win/.test(said)) return "windows";
  if (/linux|x11|ubuntu|fedora|debian/.test(said)) return "linux";
  return "other";
}
