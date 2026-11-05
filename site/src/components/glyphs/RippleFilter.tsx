import type { Ref } from "react";

const CALM_FREQUENCY = [0.005, 0.018];
const STIRRED_FREQUENCY = [0.008, 0.026];
const RIPPLE_PERIOD_SECONDS = 7;

/** The noise's `baseFrequency` at `time` seconds, swelling and easing back
    over one ripple period. */
export function rippleFrequencyAt(time: number): string {
  const position = (time % RIPPLE_PERIOD_SECONDS) / RIPPLE_PERIOD_SECONDS;
  const swell = 1 - Math.abs(2 * position - 1);
  return CALM_FREQUENCY.map((calm, axis) => (calm + (STIRRED_FREQUENCY[axis] - calm) * swell).toFixed(5)).join(" ");
}

/** The water the whale swims through, as a filter for glyphs: slow
    ripples bend the glyph, then a soft blur pulled back to a crisp edge
    rounds it like a drop of ink. It holds still unless a glyph's owner moves
    its noise, so the browser can keep the filtered glyph as a finished
    picture and slide it around without filtering it again. */
export function RippleFilter({
  id,
  scale,
  softness,
  ref,
}: {
  id: string;
  scale: number;
  softness: number;
  ref?: Ref<SVGFilterElement>;
}) {
  return (
    <svg aria-hidden className="absolute size-0">
      <filter ref={ref} id={id} x="-40%" y="-40%" width="180%" height="180%" colorInterpolationFilters="sRGB">
        <feTurbulence type="fractalNoise" baseFrequency={CALM_FREQUENCY.join(" ")} numOctaves="1" seed="7" result="noise" />
        <feDisplacementMap in="SourceGraphic" in2="noise" scale={scale} xChannelSelector="R" yChannelSelector="G" />
        <feGaussianBlur stdDeviation={softness} />
        <feColorMatrix values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 14 -6" />
      </filter>
    </svg>
  );
}

/** A filter id usable inside `url(#…)`, made from React's `useId`. */
export const filterIdOf = (reactId: string) => `ripple-${reactId.replace(/[^a-zA-Z0-9_-]/g, "")}`;
