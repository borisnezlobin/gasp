import { ArrowRight } from "@phosphor-icons/react/dist/ssr";
import Image from "next/image";
import Link from "next/link";
import obsidianIcon from "@/assets/icon-obsidian.png";
import { FEATURES_PATH } from "@/lib/site";

/** The hero's second fact: Gasp opens an Obsidian vault as it is. */
export function ObsidianLink() {
  return (
    <Link
      href={`${FEATURES_PATH}/obsidian-vaults`}
      className="group inline-flex min-h-11 items-center gap-2.5 rounded-xl py-2 pr-3 pl-2 font-bold transition-colors duration-150 hover:bg-fill"
    >
      <Image src={obsidianIcon} alt="" width={28} height={28} className="size-7" />
      Obsidian-compatible
      <ArrowRight
        size={18}
        weight="bold"
        aria-hidden
        className="text-ink-muted transition-transform duration-150 ease-out-soft group-hover:translate-x-0.5 group-hover:text-ink"
      />
    </Link>
  );
}
