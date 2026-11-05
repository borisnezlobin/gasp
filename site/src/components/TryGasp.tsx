import { DownloadButton } from "./DownloadButton";
import { Chapter } from "./PageShell";

/** The page's last word: the download, for the system the visitor is on. */
export function TryGasp({ line }: { line?: string }) {
  return (
    <Chapter id="try" title="Try Gasp" intro={line}>
      <DownloadButton size="large" others />
    </Chapter>
  );
}
