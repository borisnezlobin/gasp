import type { Metadata } from "next";
import { PageShell, PageTitle, Part } from "@/components/PageShell";
import { proseLinkStyle as linkStyle } from "@/components/styles";
import { WHALE_CREDIT } from "@/lib/site";

export const metadata: Metadata = {
  title: "Credits",
  description: "Where the humpback whale in Gasp's icon and on this site comes from.",
};

export default function Credits() {
  return (
    <PageShell>
      <PageTitle title="Credits" />
      <Part title="The humpback">
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
      </Part>
      <Part title="Type and icons">
        <p>
          Text is set in Charter, which comes with every Mac and iPhone. Elsewhere the page falls back to Charis
          SIL from SIL International, under the SIL Open Font License. Icons are from{" "}
          <a href="https://phosphoricons.com" className={linkStyle}>
            Phosphor
          </a>
          , under the MIT License.
        </p>
      </Part>
    </PageShell>
  );
}
