import type { Metadata } from "next";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteHeader } from "@/components/SiteHeader";
import { WHALE_CREDIT } from "@/lib/site";
import { LiveText } from "@/components/glyphs/LiveText";

export const metadata: Metadata = {
  title: "Credits",
  description: "Where the humpback whale in Gasp's icon and on this site comes from.",
};

const linkStyle = "underline decoration-ink-muted underline-offset-4 hover:decoration-ink";

export default function Credits() {
  return (
    <>
      <SiteHeader />
      <main className="mx-auto max-w-3xl px-4 pt-12 pb-20 sm:px-8 lg:pt-20">
        <h1 className="heading">
          <LiveText text="Credits" />
        </h1>
        <section className="mt-12">
          <h2 className="subheading text-2xl">The humpback</h2>
          <div className="body mt-4 space-y-4 text-ink-soft">
            <p>
              The whale is drawn from a{" "}
              <a href={WHALE_CREDIT.modelUrl} className={linkStyle}>
                3D model of a humpback
              </a>{" "}
              by {WHALE_CREDIT.authors}. They made it for their{" "}
              <a href={WHALE_CREDIT.paperUrl} className={linkStyle}>
                study of drag in large swimmers
              </a>{" "}
              and shared it under{" "}
              <a href={WHALE_CREDIT.licenseUrl} className={linkStyle}>
                CC BY 4.0
              </a>
              .
            </p>
            <p>
              For Gasp the model was rigged, posed and rendered in ink. The{" "}
              <a href={WHALE_CREDIT.noticesPath} className={linkStyle}>
                notices
              </a>{" "}
              list every change, and no endorsement by the authors is implied.
            </p>
          </div>
        </section>
        <section className="mt-12">
          <h2 className="subheading text-2xl">Type and icons</h2>
          <div className="body mt-4 space-y-4 text-ink-soft">
            <p>
              Text is set in Charter, which comes with every Mac and iPhone. Elsewhere the page falls back to Charis
              SIL from SIL International, under the SIL Open Font License. Icons are from{" "}
              <a href="https://phosphoricons.com" className={linkStyle}>
                Phosphor
              </a>
              , under the MIT License.
            </p>
          </div>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
