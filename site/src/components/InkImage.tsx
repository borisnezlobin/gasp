import Image, { type StaticImageData } from "next/image";

type InkImageProps = {
  light: StaticImageData | string;
  dark: StaticImageData | string;
  alt: string;
  width?: number;
  height?: number;
  className?: string;
  sizes?: string;
  priority?: boolean;
  loading?: "eager" | "lazy";
};

/** One picture drawn twice, in ink for the light theme and in chalk for
    the dark one; the page shows whichever matches the system. */
export function InkImage({ light, dark, alt, className = "", ...image }: InkImageProps) {
  return (
    <>
      <Image src={light} alt={alt} className={`dark:hidden ${className}`} {...image} />
      <Image src={dark} alt={alt} className={`hidden dark:block ${className}`} {...image} />
    </>
  );
}
