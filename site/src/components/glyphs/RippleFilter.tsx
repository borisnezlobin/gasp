import type { Ref } from "react";

/** The water the whale swims through, as a filter for glyphs: slow
    ripples bend the glyph, then a soft blur pulled back to a crisp edge
    rounds it like a drop of ink. The glyphs set the ripple's strength from
    how unsettled they are. */
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
        <feTurbulence type="fractalNoise" baseFrequency="0.005 0.018" numOctaves="1" seed="7" result="noise">
          <animate
            attributeName="baseFrequency"
            dur="7s"
            values="0.005 0.018;0.008 0.026;0.005 0.018"
            repeatCount="indefinite"
          />
        </feTurbulence>
        <feDisplacementMap in="SourceGraphic" in2="noise" scale={scale} xChannelSelector="R" yChannelSelector="G" />
        <feGaussianBlur stdDeviation={softness} />
        <feColorMatrix values="1 0 0 0 0  0 1 0 0 0  0 0 1 0 0  0 0 0 14 -6" />
      </filter>
    </svg>
  );
}

/** A filter id usable inside `url(#…)`, made from React's `useId`. */
export const filterIdOf = (reactId: string) => `ripple-${reactId.replace(/[^a-zA-Z0-9_-]/g, "")}`;
