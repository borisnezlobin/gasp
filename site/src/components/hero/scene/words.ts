import * as THREE from "three";
import type { WaveField } from "./waveField";

/** The note's lines as words: capsules lying along x in rows, each riding
    the water and thrown up when the whale breaks the surface nearby. */

const RADIUS = 0.4;
/** Word lengths in metres, caps included: one shared shape per length. */
const LENGTHS = [1.4, 2.1, 2.9, 3.8, 4.9, 6.2];
const ROW_PITCH = 2.1;
const WORD_GAP = 0.85;
const SPRING = 7;
const FRICTION = 2.6;
const GRAVITY = 16;

type Word = {
  x: number;
  z: number;
  mesh: number;
  slot: number;
  lift: THREE.Vector3;
  velocity: THREE.Vector3;
  spin: number;
  spinVelocity: number;
};

export type Bounds = { minX: number; maxX: number; minZ: number; maxZ: number };

/** The same pseudo-random numbers on every load, so the note never jumps. */
function seeded(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state * 1664525 + 1013904223) % 4294967296;
    return state / 4294967296;
  };
}

/** How far across a row reaches: paragraphs run full width and end on a
    short line, then leave a blank one. */
function rowReach(rowInParagraph: number, paragraphLength: number, random: () => number): number {
  if (rowInParagraph === paragraphLength) return 0;
  if (rowInParagraph === paragraphLength - 1) return 0.25 + random() * 0.35;
  return 0.86 + random() * 0.14;
}

function layoutRow(z: number, reach: number, bounds: Bounds, random: () => number): Omit<Word, "slot" | "lift" | "velocity" | "spin" | "spinVelocity">[] {
  const width = (bounds.maxX - bounds.minX) * reach;
  const words: Omit<Word, "slot" | "lift" | "velocity" | "spin" | "spinVelocity">[] = [];
  let x = bounds.minX;
  while (x < bounds.minX + width) {
    const mesh = Math.floor(random() * LENGTHS.length);
    const length = LENGTHS[mesh];
    words.push({ x: x + length / 2, z, mesh });
    x += length + WORD_GAP;
  }
  return words;
}

function layoutWords(bounds: Bounds): Word[] {
  const random = seeded(11);
  const words: Word[] = [];
  let paragraphLength = 3 + Math.floor(random() * 4);
  let rowInParagraph = 0;
  for (let z = bounds.maxZ; z > bounds.minZ; z -= ROW_PITCH) {
    const reach = rowReach(rowInParagraph, paragraphLength, random);
    for (const word of layoutRow(z, reach, bounds, random)) {
      words.push({ ...word, slot: 0, lift: new THREE.Vector3(), velocity: new THREE.Vector3(), spin: 0, spinVelocity: 0 });
    }
    rowInParagraph += 1;
    if (rowInParagraph > paragraphLength) {
      rowInParagraph = 0;
      paragraphLength = 3 + Math.floor(random() * 4);
    }
  }
  return words;
}

const scratch = {
  matrix: new THREE.Matrix4(),
  position: new THREE.Vector3(),
  rotation: new THREE.Quaternion(),
  euler: new THREE.Euler(0, 0, 0, "YXZ"),
  scale: new THREE.Vector3(1, 1, 1),
};

export class WordField {
  readonly meshes: THREE.InstancedMesh[];
  private readonly words: Word[];

  constructor(bounds: Bounds, material: THREE.Material) {
    this.words = layoutWords(bounds);
    const counts = LENGTHS.map(() => 0);
    for (const word of this.words) word.slot = counts[word.mesh]++;
    this.meshes = LENGTHS.map((length, index) => {
      const shape = new THREE.CapsuleGeometry(RADIUS, length - 2 * RADIUS, 6, 14);
      shape.rotateZ(Math.PI / 2);
      shape.scale(1, 0.62, 1);
      const mesh = new THREE.InstancedMesh(shape, material, Math.max(1, counts[index]));
      mesh.count = counts[index];
      mesh.castShadow = true;
      mesh.receiveShadow = true;
      return mesh;
    });
  }

  /** Throws the words near (x, z) up and outward, harder nearer the middle. */
  scatter(x: number, z: number, strength: number, radius: number) {
    for (const word of this.words) {
      const dx = word.x - x;
      const dz = word.z - z;
      const distance = Math.hypot(dx, dz);
      if (distance > radius) continue;
      const force = strength * (1 - distance / radius);
      const away = distance > 0.01 ? 1 / distance : 0;
      word.velocity.x += dx * away * force * 0.5;
      word.velocity.z += dz * away * force * 0.5;
      word.velocity.y += force * (0.8 + 0.4 * Math.sin(word.x * 7.1 + word.z));
      word.spinVelocity += (Math.sin(word.x * 3.3 - word.z) * force) / 2;
    }
  }

  update(water: WaveField, dt: number) {
    for (const word of this.words) {
      this.settle(word, dt);
      this.place(word, water);
    }
    for (const mesh of this.meshes) mesh.instanceMatrix.needsUpdate = true;
  }

  /** A thrown word flies, falls, and is pulled back to its place in the line. */
  private settle(word: Word, dt: number) {
    const airborne = word.lift.y > 0.05;
    word.velocity.y -= (airborne ? GRAVITY : 0) * dt;
    word.velocity.addScaledVector(word.lift, -SPRING * dt);
    word.velocity.multiplyScalar(Math.exp(-FRICTION * dt));
    word.lift.addScaledVector(word.velocity, dt);
    word.spinVelocity -= word.spin * SPRING * dt;
    word.spinVelocity *= Math.exp(-FRICTION * dt);
    word.spin += word.spinVelocity * dt;
  }

  private place(word: Word, water: WaveField) {
    const x = word.x + word.lift.x;
    const z = word.z + word.lift.z;
    const slope = water.slopeAt(x, z);
    scratch.position.set(x, water.heightAt(x, z) + word.lift.y, z);
    scratch.euler.set(-Math.atan(slope.z) * 0.9 + word.spin, 0, Math.atan(slope.x) * 0.9);
    scratch.rotation.setFromEuler(scratch.euler);
    scratch.matrix.compose(scratch.position, scratch.rotation, scratch.scale);
    this.meshes[word.mesh].setMatrixAt(word.slot, scratch.matrix);
  }

  dispose() {
    for (const mesh of this.meshes) mesh.geometry.dispose();
  }
}
