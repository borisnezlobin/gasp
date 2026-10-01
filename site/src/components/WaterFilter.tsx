/** Moving water, as an SVG filter any element can look through with
    `[filter:url(#water)]`: the hero's whale below the surface and the one
    under the footer. Defined once, in the root layout. */
export function WaterFilter() {
  return (
    <svg aria-hidden className="absolute size-0">
      <filter id="water" x="-10%" y="-10%" width="120%" height="120%">
        <feTurbulence type="fractalNoise" baseFrequency="0.008 0.05" numOctaves="2" seed="4" result="noise">
          <animate
            attributeName="baseFrequency"
            dur="9s"
            values="0.008 0.05;0.011 0.07;0.008 0.05"
            repeatCount="indefinite"
          />
        </feTurbulence>
        <feDisplacementMap in="SourceGraphic" in2="noise" scale="14" xChannelSelector="R" yChannelSelector="G" />
      </filter>
    </svg>
  );
}
