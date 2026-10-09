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
  body?: string | null;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  assets: GitHubAsset[];
};

/** The files a release offers, one per way of installing Gasp. */
export const INSTALLERS = ["mac", "linux", "deb"] as const;
export type Installer = (typeof INSTALLERS)[number];

export type Download = {
  url: string;
  name: string;
  size: number;
  sha256?: string;
};

export type Release = {
  tag: string;
  version: string;
  publishedAt: string | null;
  files: Partial<Record<Installer, Download>>;
  /** GitHub's count of downloads of every installer in the release. */
  downloads: number;
};

/** The newest stable version, as the app's update check reads it. */
export type NewestVersion = {
  version: string;
  url: string;
  notes: string;
  published: string;
  size: number;
  sha256?: string;
};

/** Which installer an asset is: the Mac's disk image, the Linux tarball
    (`Gasp-0.2.5-linux-x86_64.tar.gz`) or the Debian package. */
export function installerOf(name: string): Installer | null {
  const lower = name.toLowerCase();
  if (lower.endsWith(".dmg")) return "mac";
  if (/-linux-x86_64\.tar\.gz$/.test(lower)) return "linux";
  if (/_amd64\.deb$/.test(lower)) return "deb";
  return null;
}

function isInstaller(asset: GitHubAsset): boolean {
  return installerOf(asset.name) !== null;
}

function toDownload(asset: GitHubAsset): Download {
  const sha256 = sha256Of(asset);
  return {
    url: asset.browser_download_url,
    name: asset.name,
    size: asset.size,
    ...(sha256 ? { sha256 } : {}),
  };
}

function toRelease(release: GitHubRelease): Release {
  const files: Partial<Record<Installer, Download>> = {};
  for (const asset of release.assets) {
    const installer = installerOf(asset.name);
    if (installer && !files[installer]) files[installer] = toDownload(asset);
  }
  return {
    tag: release.tag_name,
    version: release.tag_name.replace(/^v/, ""),
    publishedAt: release.published_at,
    files,
    downloads: release.assets.filter(isInstaller).reduce((sum, asset) => sum + asset.download_count, 0),
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

/** The newest published release with `installer`, and that file. */
export async function latestDownload(installer: Installer): Promise<{ release: Release; file: Download } | null> {
  const releases = await publishedReleases();
  for (const release of releases) {
    const file = release.files[installer];
    if (file) return { release, file };
  }
  return null;
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

function toNewestVersion(release: GitHubRelease, file: GitHubAsset): NewestVersion {
  const sha256 = sha256Of(file);
  return {
    version: release.tag_name.replace(/^v/, ""),
    url: file.browser_download_url,
    notes: release.html_url,
    published: release.published_at ?? "",
    size: file.size,
    ...(sha256 ? { sha256 } : {}),
  };
}

/** The newest published release that isn't a draft or a prerelease and
    has `installer` (the Mac's disk image unless said), or null when
    there's none. */
export function newestStableVersion(releases: GitHubRelease[], installer: Installer = "mac"): NewestVersion | null {
  for (const release of releases.filter(isStable).sort(newestFirst)) {
    const file = release.assets?.find((asset) => installerOf(asset.name) === installer);
    if (file) return toNewestVersion(release, file);
  }
  return null;
}

export type ChangelogEntry = {
  version: string;
  publishedAt: string;
  notes: string | null;
  url: string;
  prerelease: boolean;
};

/** Every published release, newest first, with its notes as written on
    GitHub. */
export function changelogEntries(releases: GitHubRelease[]): ChangelogEntry[] {
  return releases
    .filter((release) => !release.draft && release.published_at !== null)
    .sort(newestFirst)
    .map((release) => ({
      version: release.tag_name.replace(/^v/, ""),
      publishedAt: release.published_at ?? "",
      notes: release.body ?? null,
      url: release.html_url,
      prerelease: release.prerelease,
    }));
}
