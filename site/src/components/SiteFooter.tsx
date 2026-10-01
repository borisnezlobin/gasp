import Link from "next/link";
import whaleDark from "@/assets/whale-swim-dark.png";
import whaleLight from "@/assets/whale-swim-light.png";
import { CREDITS_PATH, PRIVACY_PATH, REPO_URL } from "@/lib/site";
import { InkImage } from "./InkImage";
import { RippleLines } from "./RippleLines";

const linkStyle = "underline decoration-rule underline-offset-4 hover:decoration-ink";

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

export function SiteFooter() {
  return (
    <footer className="mx-auto max-w-7xl px-4 pt-16 pb-10 sm:px-8">
      <WhaleUnderTheLines />
      <nav aria-label="More" className="small relative z-10 mt-14 flex flex-wrap gap-x-6 gap-y-2 text-ink-soft">
        <Link href={PRIVACY_PATH} className={linkStyle}>
          Privacy
        </Link>
        <Link href={CREDITS_PATH} className={linkStyle}>
          Credits
        </Link>
        <a href={REPO_URL} className={linkStyle}>
          Source on GitHub
        </a>
      </nav>
    </footer>
  );
}
