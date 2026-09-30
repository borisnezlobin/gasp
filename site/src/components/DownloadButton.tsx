import { AppleLogo } from "@phosphor-icons/react/dist/ssr";
import { DOWNLOAD_PATH } from "@/lib/site";
import { ButtonLink } from "./Button";

export function DownloadButton({ size = "regular" }: { size?: "regular" | "large" }) {
  const iconSize = size === "large" ? 22 : 18;
  return (
    <ButtonLink href={DOWNLOAD_PATH} size={size} icon={<AppleLogo size={iconSize} weight="fill" aria-hidden />}>
      Download for Mac
    </ButtonLink>
  );
}
