import type { ReactNode } from "react";
import swimDark from "@/assets/whale-swim-dark.png";
import swimLight from "@/assets/whale-swim-light.png";
import { Caret } from "./Caret";
import { LiveText } from "./glyphs/LiveText";
import { InkImage } from "./InkImage";
import { RippleLines } from "./RippleLines";

/** The note's lines under the name, as the app icon draws them. */
const SEA_LENGTHS = [1, 0.96, 0.88, 0.64, 0.92, 0.4];

/** The app icon made into a page: the name set huge in soft glyphs, over
    the lines of a note with a humpback gliding beneath them, seen through
    moving water. The pointer rings the lines and pushes the glyphs. */
export function SwimHero({ children }: { children: ReactNode }) {
  return (
    <section className="relative overflow-x-clip">
      <div className="relative mx-auto flex max-w-7xl flex-col px-4 pt-4 pb-20 [--cross:46%] [--whale-w:min(86vw,28rem)] sm:px-8 md:[--cross:72%] md:[--whale-w:min(52vw,32rem)] lg:[--cross:66%] lg:[--whale-w:min(44vw,42rem)]">
        <h1 className="wordmark order-1 select-none">
          <LiveText text="Gasp" />
        </h1>

        <p className="lede relative z-10 order-4 mt-8 max-w-[26rem] text-ink sm:text-2xl lg:order-2 lg:mt-6 lg:max-w-[40%]">
          A Markdown editor optimized for speed and efficiency.
          <Caret />
        </p>

        <div className="relative order-3 mt-10">
          <div className="relative z-10 min-h-[calc(var(--whale-w)*0.42)]">
            <RippleLines lengths={SEA_LENGTHS} playful />
          </div>
          <div
            aria-hidden
            className="pointer-events-none absolute -inset-x-[50vw] top-0 h-[calc(var(--whale-w)*0.5+2rem)] overflow-hidden [mask-image:linear-gradient(to_bottom,black_55%,transparent)]"
          >
            <div className="absolute inset-x-[50vw] -top-6 bottom-0 [filter:url(#water)]">
              <div className="absolute top-[calc(1.5rem+var(--whale-w)*0.07)] left-[calc(var(--cross)-var(--whale-w)*0.5)] w-(--whale-w) motion-safe:animate-glide">
                <InkImage light={swimLight} dark={swimDark} alt="" sizes="(min-width: 1024px) 44vw, 86vw" priority className="h-auto w-full" />
              </div>
            </div>
            <div className="absolute inset-0 bg-paper/65" />
          </div>
        </div>

        <div className="relative z-10 order-5 mt-10">{children}</div>
      </div>
    </section>
  );
}
