import * as THREE from "three";

/** The scroll story as keyframes over progress 0 to 1: the camera dives
    under the words, the whale swims at it out of the haze, breaches through
    the surface with the camera following it up, and falls back while the
    words land as the name. Everything is a function of progress, so
    scrolling backwards plays it backwards. */

type Vec = [number, number, number];
type Key<T> = { at: number; value: T };

export type Shot = { position: THREE.Vector3; look: THREE.Vector3 };
export type WhalePose = { position: THREE.Vector3; heading: number; pitch: number; roll: number };

/** From here on the story steers the whale and the camera; before it,
    the hero is free to play with. */
export const STORY_STARTS = 0.12;
export const STORY_HOLDS = 0.18;

/** The moments the words burst from the surface and land as letters. */
export const BURST = { from: 0.585, to: 0.64 };
export const FLIGHT = { from: 0.66, to: 0.86 };
export const BREATH = { from: 0.88, to: 1 };

const CAMERA: Key<{ position: Vec; look: Vec }>[] = [
  { at: 0, value: { position: [0, 8.5, 30], look: [0, 3.6, -20] } },
  { at: 0.1, value: { position: [0, 8.5, 30], look: [0, 3.6, -20] } },
  { at: 0.2, value: { position: [0, 1.4, 10], look: [0, -0.6, -14] } },
  { at: 0.29, value: { position: [0, -3.6, 2], look: [0, -2.6, -22] } },
  { at: 0.44, value: { position: [1.5, -4.4, -3], look: [-1, -4.2, -40] } },
  { at: 0.53, value: { position: [3.5, -5.2, -8], look: [-1.5, 1.5, -17] } },
  { at: 0.63, value: { position: [2.5, 2.6, -3], look: [0, 8.5, -18] } },
  { at: 0.78, value: { position: [0, 7.2, 15], look: [0, 7.4, -10] } },
  { at: 1, value: { position: [0, 7.2, 19], look: [0, 7.4, -10] } },
];

/** Heading −π/2 points the whale's nose at the camera, along +z. */
const TOWARDS_CAMERA = -Math.PI / 2;

const WHALE: Key<{ position: Vec; pitch: number; roll: number }>[] = [
  { at: STORY_HOLDS, value: { position: [-2, -6.5, -78], pitch: 0, roll: 0 } },
  { at: 0.45, value: { position: [-3, -5.4, -15], pitch: 0.05, roll: 0 } },
  { at: 0.52, value: { position: [-2, -7.5, -17], pitch: 1.15, roll: 0.1 } },
  { at: 0.6, value: { position: [-1, 2.5, -18], pitch: 1.35, roll: 0.6 } },
  { at: 0.66, value: { position: [0, 8.8, -19], pitch: 0.55, roll: 1.6 } },
  { at: 0.74, value: { position: [1, -1.5, -20], pitch: -0.95, roll: 2.8 } },
  { at: 0.82, value: { position: [1.5, -7.5, -21], pitch: -1.2, roll: 2.95 } },
  { at: 1, value: { position: [9, -8, -30], pitch: -0.2, roll: 3.1 } },
];

/** Smooth in and out between each pair of keys. */
function between<T>(keys: Key<T>[], progress: number): { from: T; to: T; t: number } {
  const last = keys[keys.length - 1];
  if (progress <= keys[0].at) return { from: keys[0].value, to: keys[0].value, t: 0 };
  if (progress >= last.at) return { from: last.value, to: last.value, t: 0 };
  const next = keys.findIndex((key) => key.at > progress);
  const from = keys[next - 1];
  const to = keys[next];
  const t = THREE.MathUtils.smootherstep((progress - from.at) / (to.at - from.at), 0, 1);
  return { from: from.value, to: to.value, t };
}

const mixVec = (from: Vec, to: Vec, t: number, into: THREE.Vector3) =>
  into.set(from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t, from[2] + (to[2] - from[2]) * t);

const shot: Shot = { position: new THREE.Vector3(), look: new THREE.Vector3() };
const pose: WhalePose = { position: new THREE.Vector3(), heading: TOWARDS_CAMERA, pitch: 0, roll: 0 };

export function cameraAt(progress: number): Shot {
  const { from, to, t } = between(CAMERA, progress);
  mixVec(from.position, to.position, t, shot.position);
  mixVec(from.look, to.look, t, shot.look);
  return shot;
}

export function whaleAt(progress: number): WhalePose {
  const { from, to, t } = between(WHALE, progress);
  mixVec(from.position, to.position, t, pose.position);
  pose.pitch = THREE.MathUtils.lerp(from.pitch, to.pitch, t);
  pose.roll = THREE.MathUtils.lerp(from.roll, to.roll, t);
  return pose;
}

/** 0 above the water, 1 once the camera is a metre under it. */
export const underwaterness = (cameraY: number) => THREE.MathUtils.smoothstep(-cameraY, -0.2, 1);

/** How far `progress` is through a span, 0 before it and 1 after. */
export const through = (span: { from: number; to: number }, progress: number) =>
  THREE.MathUtils.clamp((progress - span.from) / (span.to - span.from), 0, 1);
