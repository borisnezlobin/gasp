import Link from "next/link";
import { CREDITS_PATH, PRIVACY_PATH, REPO_URL } from "@/lib/site";
import { RippleLines } from "./RippleLines";

const linkStyle = "underline decoration-rule underline-offset-4 hover:decoration-ink";

/** The page closes on the lines of a note again, the water the hero's
    whale came out of: a ring spreads once they come into view, and the
    pointer sets off more. */
export function SiteFooter() {
  return (
    <footer className="mx-auto max-w-7xl px-4 pt-16 pb-10 sm:px-8">
      <RippleLines lengths={[1, 0.86, 0.94, 0.58]} playful dropWhenSeen={0.62} />
      <nav aria-label="More" className="small mt-10 flex flex-wrap gap-x-6 gap-y-2 text-ink-soft">
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
