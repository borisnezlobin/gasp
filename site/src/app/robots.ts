import type { MetadataRoute } from "next";
import { SITE_URL } from "@/lib/site";

/** Crawlers read the download page but stay off the files it counts
    (/download/mac and the rest), the stats and the API. */
export default function robots(): MetadataRoute.Robots {
  return {
    rules: { userAgent: "*", allow: "/", disallow: ["/download/", "/stats", "/api/"] },
    sitemap: `${SITE_URL}/sitemap.xml`,
  };
}
