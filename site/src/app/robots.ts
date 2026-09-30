import type { MetadataRoute } from "next";
import { SITE_URL } from "@/lib/site";

/** Crawlers stay off the download counter, the stats and the ping. */
export default function robots(): MetadataRoute.Robots {
  return {
    rules: { userAgent: "*", allow: "/", disallow: ["/download", "/stats", "/api/"] },
    sitemap: `${SITE_URL}/sitemap.xml`,
  };
}
