import Image from "next/image";
import Link from "next/link";
import icon from "@/app/icon.png";
import { PRIVACY_PATH } from "@/lib/site";
import { ButtonLink } from "./Button";
import { DownloadButton } from "./DownloadButton";

export function SiteHeader() {
  return (
    <header className="mx-auto flex h-18 max-w-7xl items-center gap-2 px-4 sm:px-8">
      <Link href="/" className="-ml-1 flex items-center gap-2.5 rounded-lg p-1 text-xl font-bold">
        <Image src={icon} alt="" width={32} height={32} className="size-8" priority />
        Gasp
      </Link>
      <nav className="ml-auto flex items-center gap-1" aria-label="Site">
        <ButtonLink href={PRIVACY_PATH} variant="quiet" className="max-sm:hidden">
          Privacy
        </ButtonLink>
        <DownloadButton />
      </nav>
    </header>
  );
}
