import type { Metadata } from "next";
import type { ReactNode } from "react";
import { AppleLogo, LinuxLogo, WindowsLogo } from "@phosphor-icons/react/dist/ssr";
import { ButtonLink } from "@/components/Button";
import { LiveText } from "@/components/glyphs/LiveText";
import { SiteFooter } from "@/components/SiteFooter";
import { SiteHeader } from "@/components/SiteHeader";
import { YourSystemMark } from "@/components/YourSystemMark";
import { latestDownload, type Download, type Installer, type Release } from "@/lib/releases";
import { CHANGELOG_PATH, DOWNLOAD_PATH, REPO_URL, SITE_URL } from "@/lib/site";
import type { VisitorSystem } from "@/lib/visitorSystem";

const title = "Download Gasp for Mac and Linux";
const description =
  "Download Gasp, the fast, free Markdown editor for your notes and Obsidian vault: a disk image for macOS, and a .deb or a tarball for Ubuntu, Debian, Fedora and other Linux distributions.";

export const metadata: Metadata = {
  title: { absolute: title },
  description,
  alternates: { canonical: DOWNLOAD_PATH },
  openGraph: { title, description, url: `${SITE_URL}${DOWNLOAD_PATH}`, siteName: "Gasp", type: "website" },
  twitter: { card: "summary_large_image", title, description },
};

/** Ten minutes, like the update check, so a new release shows up soon
    after it's published. */
export const revalidate = 600;

type Found = { release: Release; file: Download } | null;

const linkStyle = "underline decoration-rule underline-offset-4 hover:decoration-ink";
const releaseDate = new Intl.DateTimeFormat("en-US", { dateStyle: "long", timeZone: "UTC" });

function megabytes(bytes: number): string {
  return `${Math.round(bytes / 1_000_000)} MB`;
}

/** The file's name and size under its button. */
function FileLine({ found }: { found: Found }) {
  if (!found) return null;
  return (
    <p className="small text-ink-muted">
      <span className="code">{found.file.name}</span>, {megabytes(found.file.size)}
    </p>
  );
}

function Platform({
  id,
  system,
  icon,
  name,
  requirements,
  children,
}: {
  id: string;
  system: VisitorSystem;
  icon: ReactNode;
  name: string;
  requirements: ReactNode;
  children: ReactNode;
}) {
  return (
    <section id={id} aria-labelledby={`${id}-title`} className="flex scroll-mt-8 flex-col rounded-2xl bg-surface p-6 shadow-lifted">
      <div className="flex flex-wrap items-center gap-3">
        {icon}
        <h2 id={`${id}-title`} className="subheading text-2xl">
          {name}
        </h2>
        <YourSystemMark system={system} />
      </div>
      <p className="body mt-3 text-ink-soft">{requirements}</p>
      <div className="mt-6 flex flex-col items-start gap-3">{children}</div>
    </section>
  );
}

function DownloadLink({ installer, found, children, quiet = false }: { installer: Installer; found: Found; children: ReactNode; quiet?: boolean }) {
  if (!found) return null;
  return (
    <ButtonLink href={`${DOWNLOAD_PATH}/${installer}`} variant={quiet ? "quiet" : "primary"} className={quiet ? "-ml-4" : ""}>
      {children}
    </ButtonLink>
  );
}

function Part({ id, title, children }: { id?: string; title: string; children: ReactNode }) {
  return (
    <section id={id} className="mt-16 scroll-mt-8">
      <h2 className="subheading text-2xl">
        <LiveText text={title} />
      </h2>
      <div className="body mt-4 space-y-4 text-ink-soft">{children}</div>
    </section>
  );
}

function Command({ children }: { children: string }) {
  return <pre className="code overflow-x-auto rounded-xl bg-fill p-5 leading-relaxed text-ink">{children}</pre>;
}

/** What search engines read about the app, from the newest release. */
function StructuredData({ version }: { version: string | null }) {
  const data = {
    "@context": "https://schema.org",
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
  };
  return <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(data).replace(/</g, "\\u003c") }} />;
}

function Checksums({ files }: { files: Found[] }) {
  const listed = files.filter((found): found is NonNullable<Found> => found !== null && found.file.sha256 !== undefined);
  if (listed.length === 0) return null;
  return (
    <Part title="Checksums">
      <p>Each file&apos;s SHA-256, as GitHub records it:</p>
      <Command>{listed.map((found) => `${found.file.sha256}  ${found.file.name}`).join("\n")}</Command>
    </Part>
  );
}

function Newest({ found }: { found: Found }) {
  if (!found?.release.publishedAt) return null;
  return (
    <p className="small mt-4 text-ink-muted">
      Version {found.release.version}, released {releaseDate.format(new Date(found.release.publishedAt))}.{" "}
      <a href={CHANGELOG_PATH} className={linkStyle}>
        What&apos;s new
      </a>
    </p>
  );
}

export default async function DownloadPage() {
  const [mac, linux, deb] = await Promise.all([latestDownload("mac"), latestDownload("linux"), latestDownload("deb")]);
  const linuxOut = linux !== null || deb !== null;
  const tarball = linux?.file.name ?? "Gasp-<version>-linux-x86_64.tar.gz";
  const folder = tarball.replace(/\.tar\.gz$/, "");
  const package_ = deb?.file.name ?? "gasp_<version>_amd64.deb";
  return (
    <>
      <SiteHeader />
      <StructuredData version={mac?.release.version ?? linux?.release.version ?? null} />
      <main className="mx-auto max-w-5xl px-4 pt-12 pb-20 sm:px-8 lg:pt-20">
        <h1 className="heading">
          <LiveText text="Download Gasp" />
        </h1>
        <p className="lede mt-6 max-w-3xl text-ink-soft">
          Gasp is free, for Mac and Linux, with Windows on its way. It opens your notes as they are, Markdown files in
          a folder, so there&apos;s nothing to import and nothing to export.
        </p>
        <Newest found={mac ?? linux} />

        <div className="mt-12 grid gap-5 md:grid-cols-3">
          <Platform
            id="mac"
            system="mac"
            icon={<AppleLogo size={28} weight="fill" aria-hidden />}
            name="Mac"
            requirements="macOS 12 Monterey or later, on Apple silicon or Intel."
          >
            {mac ? (
              <>
                <DownloadLink installer="mac" found={mac}>
                  Download for Mac
                </DownloadLink>
                <FileLine found={mac} />
              </>
            ) : (
              <a href={REPO_URL} className={`small ${linkStyle}`}>
                Releases on GitHub
              </a>
            )}
          </Platform>

          <Platform
            id="linux"
            system="linux"
            icon={<LinuxLogo size={28} weight="fill" aria-hidden />}
            name="Linux"
            requirements="64-bit Intel or AMD, on Ubuntu 22.04, Debian 12, Fedora 36 or newer, with graphics that support Vulkan."
          >
            {linuxOut ? (
              <>
                <DownloadLink installer="deb" found={deb}>
                  .deb for Ubuntu and Debian
                </DownloadLink>
                <FileLine found={deb} />
                <DownloadLink installer="linux" found={linux} quiet>
                  Tarball for any distribution
                </DownloadLink>
                <FileLine found={linux} />
              </>
            ) : (
              <p className="small text-ink-muted">The first Linux build is on its way.</p>
            )}
          </Platform>

          <Platform
            id="windows"
            system="windows"
            icon={<WindowsLogo size={28} weight="fill" aria-hidden />}
            name="Windows"
            requirements="Coming soon. Gasp is built to run on Windows too, and it's being readied now."
          >
            <a href={REPO_URL} className={`small ${linkStyle}`}>
              Follow along on GitHub
            </a>
          </Platform>
        </div>

        <Part id="install-mac" title="Installing on a Mac">
          <p>
            Open the disk image and drag Gasp into Applications. Gasp checks for a new version once a day, and updating
            takes one click.
          </p>
        </Part>

        <Part id="install-linux" title="Installing on Linux">
          <p>
            On Ubuntu, Debian, Linux Mint and Pop!_OS, open the .deb with your software installer, or install it from a
            terminal:
          </p>
          <Command>{`sudo apt install ./${package_}`}</Command>
          <p>
            On any other distribution, unpack the tarball and run its installer. It installs Gasp for you alone, in
            ~/.local, with an entry in your apps, and that copy updates itself when a new version is out:
          </p>
          <Command>{`tar -xzf ${tarball}\n./${folder}/install.sh`}</Command>
          <p>
            <span className="code">./install.sh --uninstall</span> removes it again. Neither touches your notes. A copy
            installed from the .deb tells you when a new version is out, and you install it the same way.
          </p>
        </Part>

        <Checksums files={[mac, deb, linux]} />
      </main>
      <SiteFooter />
    </>
  );
}
