import { REPO } from "./site";

const RELEASES_API = `https://api.github.com/repos/${REPO}/releases?per_page=20`;
const CACHE_SECONDS = 300;

type GitHubAsset = {
  name: string;
  browser_download_url: string;
  download_count: number;
};

type GitHubRelease = {
  tag_name: string;
  draft: boolean;
  published_at: string | null;
  assets: GitHubAsset[];
};

export type Release = {
  tag: string;
  publishedAt: string | null;
  dmgUrl: string | null;
  downloads: number;
};

function isDmg(asset: GitHubAsset): boolean {
  return asset.name.toLowerCase().endsWith(".dmg");
}

function toRelease(release: GitHubRelease): Release {
  const dmg = release.assets.find(isDmg);
  return {
    tag: release.tag_name,
    publishedAt: release.published_at,
    dmgUrl: dmg?.browser_download_url ?? null,
    downloads: release.assets.filter(isDmg).reduce((sum, asset) => sum + asset.download_count, 0),
  };
}

/** Published releases, newest first, cached for a few minutes. Prereleases
    count: the first release is one, and GitHub's "latest" skips them. */
export async function publishedReleases(): Promise<Release[]> {
  try {
    const response = await fetch(RELEASES_API, {
      headers: { Accept: "application/vnd.github+json" },
      next: { revalidate: CACHE_SECONDS },
    });
    if (!response.ok) return [];
    const releases = (await response.json()) as GitHubRelease[];
    return releases.filter((release) => !release.draft).map(toRelease);
  } catch {
    return [];
  }
}

export async function latestRelease(): Promise<Release | null> {
  const releases = await publishedReleases();
  return releases.find((release) => release.dmgUrl !== null) ?? null;
}
