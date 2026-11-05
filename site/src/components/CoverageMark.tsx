import { Check, CircleHalf, Minus } from "@phosphor-icons/react/dist/ssr";
import { LineIcon } from "./LineIcon";
import type { Icon } from "@phosphor-icons/react";
import type { Coverage } from "@/lib/comparison";

const MARKS: Record<Coverage, { icon: Icon; label: string }> = {
  yes: { icon: Check, label: "Yes" },
  partly: { icon: CircleHalf, label: "Partly" },
  no: { icon: Minus, label: "No" },
};

/** The text colour that goes with each mark, so a "no" reads quieter. */
export const COVERAGE_TONE: Record<Coverage, string> = {
  yes: "text-ink",
  partly: "text-ink-soft",
  no: "text-ink-muted",
};

/** A check, a half circle or a dash, so support reads by shape. */
export function CoverageMark({ coverage }: { coverage: Coverage }) {
  const { icon: MarkIcon, label } = MARKS[coverage];
  return (
    <LineIcon>
      <MarkIcon size={20} weight="bold" role="img" aria-label={label} />
    </LineIcon>
  );
}
