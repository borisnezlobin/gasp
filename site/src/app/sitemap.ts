import type { MetadataRoute } from "next";
import { COMPARISON_LINKS, FEATURE_LINKS } from "@/lib/pages";
import { CHANGELOG_PATH, COMPARE_PATH, CREDITS_PATH, DOWNLOAD_PATH, FEATURES_PATH, PRIVACY_PATH, SITE_URL } from "@/lib/site";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    { url: SITE_URL, priority: 1 },
    { url: `${SITE_URL}${DOWNLOAD_PATH}`, priority: 0.9 },
    { url: `${SITE_URL}${FEATURES_PATH}`, priority: 0.8 },
    ...FEATURE_LINKS.map((link) => ({ url: `${SITE_URL}${link.path}`, priority: 0.8 })),
    { url: `${SITE_URL}${COMPARE_PATH}`, priority: 0.8 },
    ...COMPARISON_LINKS.map((link) => ({ url: `${SITE_URL}${link.path}`, priority: 0.8 })),
    { url: `${SITE_URL}${CHANGELOG_PATH}`, priority: 0.5 },
    { url: `${SITE_URL}${PRIVACY_PATH}`, priority: 0.3 },
    { url: `${SITE_URL}${CREDITS_PATH}`, priority: 0.2 },
  ];
}
