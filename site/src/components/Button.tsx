import type { AnchorHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "quiet";
type Size = "regular" | "large";

const VARIANTS: Record<Variant, string> = {
  primary: "bg-button text-on-button shadow-lifted hover:opacity-90 active:scale-[0.98]",
  quiet: "text-ink hover:bg-fill active:bg-sea",
};

const SIZES: Record<Size, string> = {
  regular: "h-10 px-4 gap-2 text-base rounded-lg",
  large: "h-12 px-5 gap-2.5 text-lg rounded-xl",
};

type ButtonLinkProps = AnchorHTMLAttributes<HTMLAnchorElement> & {
  variant?: Variant;
  size?: Size;
  icon?: ReactNode;
};

/** A link drawn as a button. A plain anchor, so nothing prefetches it: the
    download link counts each visit. */
export function ButtonLink({
  variant = "primary",
  size = "regular",
  icon,
  className = "",
  children,
  ...anchor
}: ButtonLinkProps) {
  return (
    <a
      {...anchor}
      className={`inline-flex shrink-0 items-center justify-center font-bold whitespace-nowrap transition duration-150 ease-out-soft ${VARIANTS[variant]} ${SIZES[size]} ${className}`}
    >
      {icon}
      {children}
    </a>
  );
}
