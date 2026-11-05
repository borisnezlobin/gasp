"use client";

import { useEffect, useRef, type ReactNode } from "react";

let whalesInView = 0;

/** Stops the water's noise while no whale is on screen to look through it. */
function runWaterWhileWhalesInView() {
  const source = document.getElementById("water-source") as unknown as SVGSVGElement | null;
  if (whalesInView > 0) source?.unpauseAnimations();
  else source?.pauseAnimations();
}

/** Its children glide a little way along, seen through the moving water.
    The filter covers only this box and its margin, and neither the glide nor
    the water runs while the box is off screen. */
export function UnderWater({ className, children }: { className: string; children: ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  const glider = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const element = box.current;
    if (!element) return;
    let seen = false;
    const setSeen = (now: boolean) => {
      if (glider.current) glider.current.style.animationPlayState = now ? "running" : "paused";
      if (now !== seen) whalesInView += now ? 1 : -1;
      seen = now;
      runWaterWhileWhalesInView();
    };
    setSeen(false);
    const observer = new IntersectionObserver(([entry]) => setSeen(entry.isIntersecting));
    observer.observe(element);
    return () => {
      observer.disconnect();
      setSeen(false);
    };
  }, []);

  return (
    <div ref={box} className={`${className} [filter:url(#water)]`}>
      <div ref={glider} className="motion-safe:animate-glide">
        {children}
      </div>
    </div>
  );
}
