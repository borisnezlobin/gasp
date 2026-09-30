"use client";

import { useEffect, useRef } from "react";

/** The tour's swimming whale, along the bottom of the window, as far
    across as the page has been read: the app's own progress mark. */
export function ScrollSwimmer() {
  const swimmer = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = swimmer.current;
    if (!element) return;
    let frame = 0;
    const update = () => {
      frame = 0;
      const room = document.documentElement.scrollHeight - window.innerHeight;
      const read = room > 0 ? Math.min(1, Math.max(0, window.scrollY / room)) : 0;
      element.style.setProperty("--read", read.toFixed(4));
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener("scroll", schedule, { passive: true });
    window.addEventListener("resize", schedule);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener("scroll", schedule);
      window.removeEventListener("resize", schedule);
    };
  }, []);

  return (
    <div
      ref={swimmer}
      aria-hidden
      className="pointer-events-none fixed inset-x-0 bottom-0 z-30 hidden h-12 md:block"
    >
      <div className="absolute bottom-3 left-0 translate-x-[calc(var(--read,0)*(100vw-7.5rem))] opacity-55 transition-transform duration-300 ease-out-soft">
        <div className="h-[46px] w-[120px] bg-[url(/art/swim-light.png)] bg-size-[3600px_46px] motion-safe:animate-swim-small dark:bg-[url(/art/swim-dark.png)]" />
      </div>
    </div>
  );
}
