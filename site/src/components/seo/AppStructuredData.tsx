import { DOWNLOAD_PATH, SITE_URL } from "@/lib/site";
import { JsonLd } from "./JsonLd";

/** What search engines read about the app itself. */
export function AppStructuredData({ description, version = null }: { description: string; version?: string | null }) {
  return (
    <JsonLd
      data={{
        "@type": "SoftwareApplication",
        name: "Gasp",
        description,
        url: SITE_URL,
        downloadUrl: `${SITE_URL}${DOWNLOAD_PATH}`,
        image: `${SITE_URL}/opengraph-image.png`,
        applicationCategory: "UtilitiesApplication",
        applicationSubCategory: "Markdown editor",
        operatingSystem: "macOS 12 or later, Linux",
        ...(version ? { softwareVersion: version } : {}),
        offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
      }}
    />
  );
}
