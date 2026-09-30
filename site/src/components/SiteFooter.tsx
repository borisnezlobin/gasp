import Link from "next/link";
import { PRIVACY_PATH, REPO_URL, WHALE_CREDIT } from "@/lib/site";

const linkStyle = "underline decoration-rule underline-offset-4 hover:decoration-ink";

export function SiteFooter() {
  return (
    <footer className="mx-auto max-w-7xl px-4 pt-10 pb-12 sm:px-8">
      <div className="flex flex-col gap-8 border-t border-rule pt-8 sm:flex-row sm:justify-between">
        <nav aria-label="More" className="small flex gap-6">
          <Link href={PRIVACY_PATH} className={linkStyle}>
            Privacy
          </Link>
          <a href={REPO_URL} className={linkStyle}>
            Source on GitHub
          </a>
        </nav>
        <p className="small max-w-xl text-ink-muted">
          The humpback is drawn from a{" "}
          <a href={WHALE_CREDIT.modelUrl} className={linkStyle}>
            3D model
          </a>{" "}
          by {WHALE_CREDIT.authors}, made for their{" "}
          <a href={WHALE_CREDIT.paperUrl} className={linkStyle}>
            study of drag in large swimmers
          </a>{" "}
          and shared under{" "}
          <a href={WHALE_CREDIT.licenseUrl} className={linkStyle}>
            CC BY 4.0
          </a>
          . It was rigged, posed and inked for Gasp; the{" "}
          <a href={WHALE_CREDIT.noticesPath} className={linkStyle}>
            notices
          </a>{" "}
          list every change.
        </p>
      </div>
    </footer>
  );
}
