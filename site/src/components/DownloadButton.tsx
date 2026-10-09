"use client";

import type { ReactNode } from "react";
import { AppleLogo, DownloadSimple, LinuxLogo, WindowsLogo } from "@phosphor-icons/react/dist/ssr";
import { DOWNLOAD_PATH } from "@/lib/site";
import type { VisitorSystem } from "@/lib/visitorSystem";
import { ButtonLink } from "./Button";
import { useVisitorSystem } from "./useVisitorSystem";

type Size = "regular" | "large";

type Offer = { label: string; href: string; icon: (size: number) => ReactNode };

/** What the button offers on each system. The Mac downloads at once;
    Linux has two files to choose from, so it goes to them; Windows isn't
    out yet. Before the page knows the system, as the server and crawlers
    see it, the button goes to the download page. */
const OFFERS: Record<VisitorSystem | "unknown", Offer> = {
  mac: {
    label: "Download for Mac",
    href: `${DOWNLOAD_PATH}/mac`,
    icon: (size) => <AppleLogo size={size} weight="fill" aria-hidden />,
  },
  linux: {
    label: "Download for Linux",
    href: `${DOWNLOAD_PATH}#linux`,
    icon: (size) => <LinuxLogo size={size} weight="fill" aria-hidden />,
  },
  windows: {
    label: "Windows coming soon",
    href: `${DOWNLOAD_PATH}#windows`,
    icon: (size) => <WindowsLogo size={size} weight="fill" aria-hidden />,
  },
  other: {
    label: "Download Gasp",
    href: DOWNLOAD_PATH,
    icon: (size) => <DownloadSimple size={size} weight="bold" aria-hidden />,
  },
  unknown: {
    label: "Download Gasp",
    href: DOWNLOAD_PATH,
    icon: (size) => <DownloadSimple size={size} weight="bold" aria-hidden />,
  },
};

/** The download button, for the system the visitor is on. With
    `others`, a link to every system's download sits beside it. */
export function DownloadButton({ size = "regular", others = false }: { size?: Size; others?: boolean }) {
  const system = useVisitorSystem();
  const offer = OFFERS[system ?? "unknown"];
  const button = (
    <ButtonLink href={offer.href} size={size} icon={offer.icon(size === "large" ? 22 : 18)}>
      {offer.label}
    </ButtonLink>
  );
  if (!others) return button;
  const showOthers = system !== null && system !== "other";
  return (
    <div className="flex flex-wrap items-center gap-x-5 gap-y-3">
      {button}
      {showOthers && (
        <a href={DOWNLOAD_PATH} className="small text-ink-soft underline decoration-rule underline-offset-4 hover:decoration-ink">
          Other systems
        </a>
      )}
    </div>
  );
}
