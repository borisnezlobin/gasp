/** The tour's swimming whale (`assets/tour/swim-*.png`: 30 frames of
    440 px, 66 ms each), crossing the lines along the bottom of the page. */
export function SwimmingWhale() {
  return (
    <div aria-hidden className="relative h-36 overflow-hidden [container-type:inline-size]">
      <div className="absolute inset-x-0 top-[5.25rem] flex flex-col gap-6">
        <div className="sea-line w-full" />
        <div className="sea-line w-[72%]" />
      </div>
      <div className="absolute top-2 left-0 motion-safe:animate-cruise">
        <div className="h-[85px] w-[220px] bg-[url(/art/swim-light.png)] bg-size-[6600px_85px] opacity-80 motion-safe:animate-swim dark:bg-[url(/art/swim-dark.png)]" />
      </div>
      <div className="absolute inset-x-0 top-[5.5rem] bottom-0 bg-paper/55" />
    </div>
  );
}
