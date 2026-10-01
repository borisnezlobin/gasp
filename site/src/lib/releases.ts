import { REPO } from "./site";

const RELEASES_API = `https://api.github.com/repos/${REPO}/releases?per_page=20`;
const CACHE_SECONDS = 300;

type GitHubAsset = {
  name: string;
  browser_download_url: string;
  download_count: number;
  size: number;
  /** Such as `sha256:51ca…`, on assets uploaded since GitHub began hashing them. */
  digest?: string | null;
};

export type GitHubRelease = {
  tag_name: string;
  html_url: string;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  assets: GitHubAsset[];
};

export type Release = {
  tag: string;
  publishedAt: string | null;
  dmgUrl: string | null;
  downloads: number;
};

/** The newest stable version, as the Mac app's update check reads it. */
export type NewestVersion = {
  version: string;
  url: string;
  notes: string;
  published: string;
  size: number;
  sha256?: string;
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

/** Every release GitHub lists, newest first, or null when it can't be reached. */
export async function gitHubReleases(cacheSeconds = CACHE_SECONDS): Promise<GitHubRelease[] | null> {
  try {
    const response = await fetch(RELEASES_API, {
      headers: { Accept: "application/vnd.github+json" },
      next: { revalidate: cacheSeconds },
    });
    if (!response.ok) return null;
    const releases: unknown = await response.json();
    return Array.isArray(releases) ? (releases as GitHubRelease[]) : null;
  } catch {
    return null;
  }
}

/** Published releases, newest first, cached for a few minutes. Prereleases
    count: the first release is one, and GitHub's "latest" skips them. */
export async function publishedReleases(): Promise<Release[]> {
  const releases = (await gitHubReleases()) ?? [];
  return releases.filter((release) => !release.draft).map(toRelease);
}

export async function latestRelease(): Promise<Release | null> {
  const releases = await publishedReleases();
  return releases.find((release) => release.dmgUrl !== null) ?? null;
}

const SHA256_DIGEST = /^sha256:([0-9a-f]{64})$/i;

function sha256Of(asset: GitHubAsset): string | undefined {
  const match = SHA256_DIGEST.exec(asset.digest ?? "");
  return match ? match[1].toLowerCase() : undefined;
}

function isStable(release: GitHubRelease): boolean {
  return !release.draft && !release.prerelease && release.published_at !== null;
}

function newestFirst(a: GitHubRelease, b: GitHubRelease): number {
  return Date.parse(b.published_at ?? "") - Date.parse(a.published_at ?? "");
}

function toNewestVersion(release: GitHubRelease, dmg: GitHubAsset): NewestVersion {
  const sha256 = sha256Of(dmg);
  return {
    version: release.tag_name.replace(/^v/, ""),
    url: dmg.browser_download_url,
    notes: release.html_url,
    published: release.published_at ?? "",
    size: dmg.size,
    ...(sha256 ? { sha256 } : {}),
  };
}

/** The newest published release that isn't a draft or a prerelease and
    has a disk image, or null when there's none. */
export function newestStableVersion(releases: GitHubRelease[]): NewestVersion | null {
  for (const release of releases.filter(isStable).sort(newestFirst)) {
    const dmg = release.assets?.find(isDmg);
    if (dmg) return toNewestVersion(release, dmg);
  }
  return null;
}
