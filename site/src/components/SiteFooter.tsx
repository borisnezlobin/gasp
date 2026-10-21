import Link from "next/link";
import whaleDark from "@/assets/whale-swim-dark.png";
import whaleLight from "@/assets/whale-swim-light.png";
import { COMPARISON_LINKS, FEATURE_LINKS } from "@/lib/pages";
import { CHANGELOG_PATH, COMPARE_PATH, CREDITS_PATH, DOWNLOAD_PATH, FEATURES_PATH, PRIVACY_PATH, REPO_URL } from "@/lib/site";
import { InkImage } from "./InkImage";
import { linkStyle } from "./styles";
import { RippleLines } from "./RippleLines";

/** The page closes on the lines of a note again, with the hero's whale
    swimming beneath them, seen through the same moving water. */
function WhaleUnderTheLines() {
  return (
    <div className="relative">
      <div aria-hidden className="pointer-events-none absolute inset-x-0 -top-4 -bottom-10 overflow-hidden">
        <div className="absolute inset-0 [filter:url(#water)]">
          <div className="absolute top-2 right-[6%] w-[min(70%,30rem)] motion-safe:animate-glide">
            <InkImage light={whaleLight} dark={whaleDark} alt="" sizes="30rem" className="h-auto w-full" />
          </div>
        </div>
        <div className="absolute inset-0 bg-paper/65" />
      </div>
      <div className="relative z-10">
        <RippleLines lengths={[1, 0.86, 0.94, 0.58]} />
      </div>
    </div>
  );
}

type FooterLink = { path: string; name: string };

const ABOUT_LINKS: FooterLink[] = [
  { path: DOWNLOAD_PATH, name: "Download" },
  { path: CHANGELOG_PATH, name: "Changelog" },
  { path: PRIVACY_PATH, name: "Privacy" },
  { path: CREDITS_PATH, name: "Credits" },
  { path: REPO_URL, name: "Source on GitHub" },
];

const GROUPS: { name: string; path: string; links: FooterLink[] }[] = [
  { name: "Features", path: FEATURES_PATH, links: FEATURE_LINKS },
  { name: "Compare", path: COMPARE_PATH, links: COMPARISON_LINKS },
  { name: "Gasp", path: "/", links: ABOUT_LINKS },
];

export function SiteFooter() {
  return (
    <footer className="mx-auto max-w-7xl px-4 pt-16 pb-10 sm:px-8">
      <WhaleUnderTheLines />
      <nav aria-label="More" className="small relative z-10 mt-14 grid grid-cols-2 gap-x-6 gap-y-10 text-ink-soft md:grid-cols-3">
        {GROUPS.map((group) => (
          <div key={group.name}>
            <Link href={group.path} className="inline-flex min-h-11 items-center font-bold text-ink hover:underline hover:underline-offset-4">
              {group.name}
            </Link>
            <ul>
              {group.links.map((link) => (
                <li key={link.path}>
                  <Link href={link.path} className={`inline-flex min-h-11 items-center ${linkStyle}`}>
                    {link.name}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </nav>
    </footer>
  );
}
