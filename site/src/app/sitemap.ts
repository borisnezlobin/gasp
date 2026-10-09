import type { MetadataRoute } from "next";
import { CHANGELOG_PATH, CREDITS_PATH, DOWNLOAD_PATH, PRIVACY_PATH, SITE_URL } from "@/lib/site";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: SITE_URL, priority: 1 },
    { url: `${SITE_URL}${DOWNLOAD_PATH}`, priority: 0.9 },
    { url: `${SITE_URL}${CHANGELOG_PATH}`, priority: 0.5 },
    { url: `${SITE_URL}${PRIVACY_PATH}`, priority: 0.3 },
    { url: `${SITE_URL}${CREDITS_PATH}`, priority: 0.2 },
  ];
}
