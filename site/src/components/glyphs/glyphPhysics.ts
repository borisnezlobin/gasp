/** One soft glyph: an offset from its place in the text that a spring pulls
    back, and a squash that rings like jelly after it's been moved. The
    pointer leans it closer from a distance and pushes it aside up close.
    Every length scales with the glyph's size, so a 300px wordmark and a
    40px heading move alike. */

export type Pointer = { x: number; y: number } | null;

export type GlyphBody = {
  size: number;
  phase: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  squash: number;
  squashVelocity: number;
  angle: number;
};

const SPRING = 90;
const DAMPING = 6.6;
const SQUASH_SPRING = 480;
const SQUASH_DAMPING = 7.5;
const PUSH = 0.34;
const PULL = 0.07;
const REACH = 0.45;
const SQUASH_PER_SPEED = 0.07;
const MOST_SQUASH = 0.32;

export const createBody = (size: number, phase: number): GlyphBody => ({
  size,
  phase,
  x: 0,
  y: 0,
  vx: 0,
  vy: 0,
  squash: 0,
  squashVelocity: 0,
  angle: 0,
});

/** How far from the pointer a glyph is pushed, and how far it's pulled. */
export const nearOf = (size: number) => size * 0.55 + 36;
export const farOf = (size: number) => nearOf(size) * 3;

/** Where the pointer wants the glyph to sit, as an offset from home:
    away inside `near`, a little towards it out to `far`. */
function pointerPull(body: GlyphBody, homeX: number, homeY: number, pointer: Pointer): [number, number] {
  if (!pointer) return [0, 0];
  const dx = pointer.x - (homeX + body.x);
  const dy = pointer.y - (homeY + body.y);
  const distance = Math.hypot(dx, dy) || 1;
  const near = nearOf(body.size);
  const far = farOf(body.size);
  if (distance > far) return [0, 0];
  const strength =
    distance < near
      ? -PUSH * (1 - distance / near) ** 2
      : PULL * Math.sin((Math.PI * (distance - near)) / (far - near));
  return [(dx / distance) * strength * body.size, (dy / distance) * strength * body.size];
}

function stepSquash(body: GlyphBody, dt: number) {
  const speed = Math.hypot(body.vx, body.vy);
  if (speed > body.size * 0.2) body.angle = Math.atan2(body.vy, body.vx);
  const target = Math.min(MOST_SQUASH, (speed / body.size) * SQUASH_PER_SPEED);
  body.squashVelocity += ((target - body.squash) * SQUASH_SPRING - body.squashVelocity * SQUASH_DAMPING) * dt;
  body.squash += body.squashVelocity * dt;
}

/** Moves the glyph on by `dt` seconds with its home at (homeX, homeY). */
export function stepBody(body: GlyphBody, homeX: number, homeY: number, pointer: Pointer, dt: number) {
  const [wantX, wantY] = pointerPull(body, homeX, homeY, pointer);
  body.vx += ((wantX - body.x) * SPRING - body.vx * DAMPING) * dt;
  body.vy += ((wantY - body.y) * SPRING - body.vy * DAMPING) * dt;
  body.x += body.vx * dt;
  body.y += body.vy * dt;
  const reach = body.size * REACH;
  const out = Math.hypot(body.x, body.y);
  if (out > reach) {
    body.x *= reach / out;
    body.y *= reach / out;
  }
  stepSquash(body, dt);
}

/** How unsettled the glyph is, in its own sizes per second. */
export const restlessness = (body: GlyphBody) => Math.hypot(body.vx, body.vy) / body.size;

/** The CSS transform for the glyph at `time` seconds, stretched along the
    way it's moving and breathing a little while it's still. */
export function transformOf(body: GlyphBody, time: number, baseX = 0, baseY = 0, grow = 1): string {
  const breathing = 0.022 * Math.sin(time * 1.7 + body.phase);
  const bob = body.size * 0.014 * Math.sin(time * 1.1 + body.phase * 2);
  const stretch = 1 + body.squash + breathing;
  const along = (stretch * grow).toFixed(4);
  const across = (grow / stretch).toFixed(4);
  const angle = body.angle.toFixed(3);
  return (
    `translate3d(${(baseX + body.x).toFixed(2)}px,${(baseY + body.y + bob).toFixed(2)}px,0) ` +
    `rotate(${angle}rad) scale(${along},${across}) rotate(${-body.angle}rad)`
  );
}
