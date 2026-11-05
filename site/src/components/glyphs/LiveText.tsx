"use client";

import { useEffect, useId, useRef, type ReactNode } from "react";
import { subscribeGlyphs } from "./glyphEngine";
import { createBody, farOf, restlessness, stepBody, transformOf, type GlyphBody, type Pointer } from "./glyphPhysics";
import { RippleFilter, filterIdOf, rippleFrequencyAt } from "./RippleFilter";

type Glyph = { span: HTMLElement; centreX: number; centreY: number; body: GlyphBody };

const CALM_RIPPLE = 0.03;
const MOST_RIPPLE = 0.11;
const SOFTNESS = 0.012;
/** The ripple's edges drift slowly and in proportion to the type, so big
    type needs fresh noise more often: about 16 times a second at 320px, twice
    a second at 40px. Each step moves an edge about one device pixel. */
const PIXELS_PER_RIPPLE_STEP = 20;
const RIPPLE_FRAME_SHARE = 3;
const LONGEST_RIPPLE_WAIT_SECONDS = 1;

/** Where each glyph sits inside the text, measured again whenever the text
    reflows. */
function measure(root: HTMLElement, glyphs: Glyph[]) {
  const size = parseFloat(getComputedStyle(root).fontSize) || 16;
  const spans = root.querySelectorAll<HTMLElement>("[data-glyph]");
  spans.forEach((span, index) => {
    const glyph = glyphs[index] ?? { span, centreX: 0, centreY: 0, body: createBody(size, index * 1.37) };
    glyph.span = span;
    glyph.centreX = span.offsetLeft + span.offsetWidth / 2;
    glyph.centreY = span.offsetTop + span.offsetHeight / 2;
    glyph.body.size = size;
    glyphs[index] = glyph;
  });
  glyphs.length = spans.length;
  return size;
}

/** The pointer, if it's close enough to the text for any glyph to feel it. */
function pointerNear(box: DOMRect, pointer: Pointer, size: number): Pointer {
  if (!pointer) return null;
  const reach = farOf(size);
  const outside =
    pointer.x < box.left - reach || pointer.x > box.right + reach || pointer.y < box.top - reach || pointer.y > box.bottom + reach;
  return outside ? null : pointer;
}

/** A glyph gets its own compositor layer only while it's near the screen.
    `will-change` keeps the browser from drawing the glyph again each time
    its scale changes. */
function setGlyphsLive(glyphs: Glyph[], live: boolean) {
  for (const { span } of glyphs) {
    span.style.willChange = live ? "transform" : "";
    if (!live) span.style.transform = "";
  }
}

/** Writes an SVG attribute only when it changes: every write makes the browser
    filter the text again. */
function attributeWriter(element: Element | null | undefined, name: string) {
  let written = "";
  return (value: string) => {
    if (value === written) return;
    written = value;
    element?.setAttribute(name, value);
  };
}

/** Says when the text is due fresh noise. Where filtering takes longer than
    a frame, as it does in browsers that filter on the CPU, the wait stretches
    so the ripple takes about a third of the frames at most. */
function rippleClock() {
  let nextStep = 0;
  let longestFrame = 0;
  return (time: number, frameSeconds: number, size: number) => {
    longestFrame = Math.max(longestFrame, frameSeconds);
    if (time < nextStep) return false;
    const stretched = Math.min(LONGEST_RIPPLE_WAIT_SECONDS, longestFrame * RIPPLE_FRAME_SHARE);
    nextStep = time + Math.max(PIXELS_PER_RIPPLE_STEP / size, stretched);
    longestFrame = 0;
    return true;
  };
}

/** Steps the glyphs while the text is on screen, and stirs its ripple by
    how unsettled they are. With reduced motion the text stays still and
    dry. */
function useSoftGlyphs(
  root: React.RefObject<HTMLElement | null>,
  ripple: React.RefObject<SVGFilterElement | null>,
  filterId: string,
  text: string,
) {
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    const glyphs: Glyph[] = [];
    const writeScale = attributeWriter(ripple.current?.querySelector("feDisplacementMap"), "scale");
    const writeFrequency = attributeWriter(ripple.current?.querySelector("feTurbulence"), "baseFrequency");
    const blur = ripple.current?.querySelector("feGaussianBlur");
    let size = 16;
    let onScreen = false;
    let stirred = 0;
    let box: DOMRect | null = null;
    const rippleDue = rippleClock();
    const remeasure = () => {
      size = measure(element, glyphs);
      blur?.setAttribute("stdDeviation", (size * SOFTNESS).toFixed(2));
    };
    remeasure();
    const resizing = new ResizeObserver(remeasure);
    resizing.observe(element);
    void document.fonts?.ready.then(remeasure);
    const motionAllowed = window.matchMedia("(prefers-reduced-motion: no-preference)").matches;
    const seeing = new IntersectionObserver(
      ([entry]) => {
        onScreen = entry.isIntersecting;
        if (motionAllowed) setGlyphsLive(glyphs, onScreen);
      },
      { rootMargin: "20%" },
    );
    seeing.observe(element);
    const unsubscribe = subscribeGlyphs({
      read: () => {
        if (onScreen) box = element.getBoundingClientRect();
      },
      step: (dt, time, pointer, view) => {
        if (!onScreen || !box) return;
        const near = pointerNear(box, pointer, size);
        let restless = 0;
        for (const glyph of glyphs) {
          stepBody(glyph.body, box.left + glyph.centreX, box.top + glyph.centreY, near, dt);
          glyph.span.style.transform = transformOf(glyph.body, time);
          restless += restlessness(glyph.body);
        }
        stirred += ((restless / Math.max(1, glyphs.length)) * 0.05 - stirred) * Math.min(1, dt * 6);
        if (view.scrolling) return;
        writeScale((size * Math.min(MOST_RIPPLE, CALM_RIPPLE + stirred)).toFixed(1));
        if (rippleDue(time, view.frameSeconds, size)) writeFrequency(rippleFrequencyAt(time));
      },
    });
    if (motionAllowed) {
      element.querySelectorAll<HTMLElement>("[data-glyph] > span").forEach((ink) => ink.style.setProperty("filter", `url(#${filterId})`));
    }
    return () => {
      unsubscribe();
      resizing.disconnect();
      seeing.disconnect();
    };
  }, [root, ripple, filterId, text]);
}

/** Display text made of soft glyphs that lean towards the pointer, part
    around it up close and wobble back, seen through moving water. The outer
    span of a glyph moves; the inner one carries the filter, so moving a glyph
    doesn't filter it again. Screen readers get the plain text. Use it as the
    only content of a text element; `trailing` sits after the last word
    without breaking from it. */
export function LiveText({ text, trailing }: { text: string; trailing?: ReactNode }) {
  const root = useRef<HTMLSpanElement>(null);
  const ripple = useRef<SVGFilterElement>(null);
  const filterId = filterIdOf(useId());
  useSoftGlyphs(root, ripple, filterId, text);
  const words = text.split(" ");
  return (
    <>
      <span className="sr-only">{text}</span>
      <RippleFilter id={filterId} scale={0} softness={0} ref={ripple} />
      <span ref={root} aria-hidden className="relative block select-none">
        {words.map((word, wordIndex) => (
          <span key={wordIndex}>
            {wordIndex > 0 && " "}
            <span className="inline-block whitespace-nowrap">
              {Array.from(word).map((glyph, glyphIndex) => (
                <span key={glyphIndex} data-glyph className="inline-block">
                  <span className="inline-block">{glyph}</span>
                </span>
              ))}
              {wordIndex === words.length - 1 && trailing}
            </span>
          </span>
        ))}
      </span>
    </>
  );
}
