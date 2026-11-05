import { ArrowRight } from "@phosphor-icons/react/dist/ssr";
import type { Icon } from "@phosphor-icons/react";
import Link from "next/link";
import type { PageLink } from "@/lib/pages";
import { LineIcon } from "./LineIcon";

/** A hub's pages, one row each: the name large enough to scan, the fact
    that sets it apart under it, and the whole row a link. */
export function HubList({ links }: { links: (PageLink & { icon?: Icon })[] }) {
  return (
    <ul className="mt-12 divide-y divide-rule border-y border-rule">
      {links.map(({ path, name, summary, icon: LinkIcon }) => (
        <li key={path}>
          <Link href={path} className="group -mx-4 flex items-start gap-4 rounded-xl px-4 py-6 transition-colors duration-150 hover:bg-fill">
            {LinkIcon && (
              <span className="request">
                <LineIcon>
                  <LinkIcon size={28} aria-hidden />
                </LineIcon>
              </span>
            )}
            <span className="min-w-0 flex-1">
              <span className="request block">{name}</span>
              <span className="body mt-1 block text-ink-soft">{summary}</span>
            </span>
            <ArrowRight
              size={22}
              weight="bold"
              aria-hidden
              className="shrink-0 self-center text-ink-muted transition-transform duration-150 ease-out-soft group-hover:translate-x-1 group-hover:text-ink"
            />
          </Link>
        </li>
      ))}
    </ul>
  );
}
