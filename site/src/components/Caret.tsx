/** The app icon's red caret, blinking where the next word would go. */
export function Caret({ blinking = true }: { blinking?: boolean }) {
  return (
    <span
      aria-hidden
      className={`ml-[0.08em] inline-block h-[1.05em] w-[3px] translate-y-[0.16em] rounded-full bg-caret ${blinking ? "animate-blink" : ""}`}
    />
  );
}
