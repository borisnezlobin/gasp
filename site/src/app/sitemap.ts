import type { MetadataRoute } from "next";
import { CHANGELOG_PATH, CREDITS_PATH, PRIVACY_PATH, SITE_URL } from "@/lib/site";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: SITE_URL },
    { url: `${SITE_URL}${PRIVACY_PATH}` },
    { url: `${SITE_URL}${CREDITS_PATH}` },
    { url: `${SITE_URL}${CHANGELOG_PATH}` },
  ];
}
