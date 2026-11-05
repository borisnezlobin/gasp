import type { Metadata } from "next";
import { SITE_URL } from "./site";

/** A page's title tag, description, canonical address and link previews,
    all from one title and one description. */
export function pageMetadata({ path, title, description }: { path: string; title: string; description: string }): Metadata {
  return {
    title: { absolute: title },
    description,
    alternates: { canonical: path },
    openGraph: { title, description, url: `${SITE_URL}${path}`, siteName: "Gasp", type: "website" },
    twitter: { card: "summary_large_image", title, description },
  };
}
