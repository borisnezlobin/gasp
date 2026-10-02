import * as THREE from "three";
import { GLTFLoader } from "three/examples/jsm/loaders/GLTFLoader.js";
import type { ScenePalette } from "./palette";
import type { WhalePose } from "./story";

/** The humpback (`public/whale.glb`, 14 m, forward +x, up +y): shaded like
    the site's ink drawings, with a dark back and a pale belly and flippers,
    fading into the water the deeper it swims. */

const SWIM_DEPTH = -3.4;
const SWIM_SPEED = 3.2;
const APPROACH_SPEED = 10;
const LEAP_SECONDS = 2.5;
const RECOVER_SECONDS = 1.6;
const LEAP_RISE = 13.5;
const LEAP_TRAVEL = 11;
/** How far from side-on, towards the camera, the whale leaps (radians). */
const LEAP_ANGLE = -0.35;
const FAR_BREACH = 22;
const SLIP_DISTANCE = 12;

type Phase =
  | { kind: "swimming" }
  | { kind: "approaching"; target: THREE.Vector2 }
  | { kind: "leaping"; elapsed: number; start: THREE.Vector3; heading: number; surfaced: boolean; splashed: boolean }
  | { kind: "recovering"; elapsed: number; roll: number };

export type SplashHandler = (x: number, z: number, strength: number) => void;

/** Pale where the body faces down (the throat pleats) and on the long
    flippers, which reach far out to the sides. */
function inkColours(geometry: THREE.BufferGeometry, palette: ScenePalette) {
  const positions = geometry.getAttribute("position");
  const normals = geometry.getAttribute("normal");
  const back = new THREE.Color(palette.whaleBack);
  const belly = new THREE.Color(palette.whaleBelly);
  const colours = new Float32Array(positions.count * 3);
  const colour = new THREE.Color();
  for (let index = 0; index < positions.count; index++) {
    const underside = THREE.MathUtils.smoothstep(-normals.getY(index), 0.15, 0.55);
    const flipper = THREE.MathUtils.smoothstep(Math.abs(positions.getZ(index)), 1.5, 2.3);
    colour.copy(back).lerp(belly, Math.max(underside, flipper));
    colour.toArray(colours, index * 3);
  }
  geometry.setAttribute("color", new THREE.BufferAttribute(colours, 3));
}

/** The ink look: an edge drawn where the body turns away from the eye,
    like the contour of a pen drawing, and, seen from above the water, the
    whale mixed into the paper the deeper it swims. Under the water the
    haze does that instead, so `uAbove` turns the fade off. */
function inkShading(material: THREE.MeshStandardMaterial, palette: ScenePalette) {
  const uniforms = {
    uPaper: { value: new THREE.Color(palette.paper) },
    uInk: { value: new THREE.Color(palette.whaleBack) },
    uAbove: { value: 1 },
  };
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms);
    shader.vertexShader = shader.vertexShader
      .replace("#include <common>", "#include <common>\nvarying float vSeaY;")
      .replace("#include <skinning_vertex>", "#include <skinning_vertex>\nvSeaY = (modelMatrix * vec4(transformed, 1.0)).y;");
    shader.fragmentShader = shader.fragmentShader
      .replace("#include <common>", "#include <common>\nvarying float vSeaY;\nuniform vec3 uPaper;\nuniform vec3 uInk;\nuniform float uAbove;")
      .replace(
        "#include <dithering_fragment>",
        [
          "float edge = pow(1.0 - abs(dot(normalize(normal), normalize(vViewPosition))), 2.6);",
          "gl_FragColor.rgb = mix(gl_FragColor.rgb, uInk, edge * 0.65);",
          "gl_FragColor.rgb = mix(gl_FragColor.rgb, uPaper, smoothstep(0.3, -6.0, vSeaY) * 0.82 * uAbove);",
          "#include <dithering_fragment>",
        ].join("\n"),
      );
  };
  return uniforms;
}

export class Whale {
  readonly root = new THREE.Group();
  private readonly mixer: THREE.AnimationMixer;
  private readonly actions = new Map<string, THREE.AnimationAction>();
  private playing = "";
  private phase: Phase = { kind: "swimming" };
  private heading = Math.PI * 0.85;
  private readonly position = new THREE.Vector3(18, SWIM_DEPTH, -14);
  private wander = 0;

  private constructor(
    model: THREE.Object3D,
    clips: THREE.AnimationClip[],
    private readonly shading: ReturnType<typeof inkShading>,
    private readonly onSplash: SplashHandler,
  ) {
    this.root.add(model);
    this.mixer = new THREE.AnimationMixer(model);
    for (const clip of clips) this.actions.set(clip.name, this.mixer.clipAction(clip));
    this.play("cruise_loop");
  }

  static async load(url: string, palette: ScenePalette, onSplash: SplashHandler): Promise<Whale> {
    const gltf = await new GLTFLoader().loadAsync(url);
    const material = new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.62, metalness: 0 });
    const shading = inkShading(material, palette);
    gltf.scene.traverse((part) => {
      if (!(part instanceof THREE.SkinnedMesh)) return;
      inkColours(part.geometry, palette);
      part.material = material;
      part.castShadow = true;
      part.frustumCulled = false;
    });
    return new Whale(gltf.scene, gltf.animations, shading, onSplash);
  }

  get isSwimming(): boolean {
    return this.phase.kind === "swimming";
  }

  /** Where the whale's back is, for the water above it to rise. */
  get back(): THREE.Vector3 {
    return this.position;
  }

  /** 1 while the camera is above the water, 0 once it's under. */
  set aboveWater(amount: number) {
    this.shading.uAbove.value = amount;
  }

  /** Puts the whale where the scroll story says, its tail beating as the
      story's clock says; the free hero takes over again from there. */
  follow(pose: WhalePose, clock: number) {
    this.play("fast_swim_loop", 0);
    this.mixer.setTime(clock);
    this.position.copy(pose.position);
    this.heading = pose.heading;
    this.phase = { kind: "swimming" };
    this.pose(pose.pitch, pose.roll);
  }

  /** Swims to (x, z) fast and breaches there. */
  breachAt(x: number, z: number) {
    if (this.phase.kind === "leaping" || this.phase.kind === "recovering") return;
    this.slipCloser(x, z);
    this.phase = { kind: "approaching", target: new THREE.Vector2(x, z) };
    this.play("fast_swim_loop");
  }

  /** A breach far from the whale shouldn't wait for it to cross the whole
      page: it slips in deep under the words a short swim away instead. */
  private slipCloser(x: number, z: number) {
    const distance = Math.hypot(x - this.position.x, z - this.position.z);
    if (distance < FAR_BREACH) return;
    const from = Math.atan2(-(z - this.position.z), x - this.position.x);
    this.heading = from;
    this.position.set(x - Math.cos(from) * SLIP_DISTANCE, -6, z + Math.sin(from) * SLIP_DISTANCE);
  }

  update(dt: number) {
    this.mixer.update(dt);
    const phase = this.phase;
    if (phase.kind === "swimming") this.swim(dt);
    else if (phase.kind === "approaching") this.approach(phase.target, dt);
    else if (phase.kind === "leaping") this.leap(phase, dt);
    else this.recover(phase, dt);
  }

  private play(name: string, fade = 0.4) {
    if (name === this.playing) return;
    const next = this.actions.get(name);
    if (!next) return;
    next.reset().fadeIn(fade).play();
    this.actions.get(this.playing)?.fadeOut(fade);
    this.playing = name;
  }

  /** Cruises below the surface, turning gently and back towards the middle. */
  private swim(dt: number) {
    this.wander += dt;
    const home = Math.atan2(-(-12 - this.position.z), -this.position.x);
    const drift = Math.sin(this.wander * 0.23) * 0.35;
    this.turnTowards(home + drift, 0.25 * dt);
    this.advance(SWIM_SPEED * dt);
    this.position.y = SWIM_DEPTH + Math.sin(this.wander * 0.7) * 0.3;
    this.pose(0, 0);
  }

  private approach(target: THREE.Vector2, dt: number) {
    const toward = Math.atan2(-(target.y - this.position.z), target.x - this.position.x);
    this.turnTowards(toward, 3.2 * dt);
    const distance = Math.hypot(target.x - this.position.x, target.y - this.position.z);
    this.advance(Math.min(distance, APPROACH_SPEED * dt));
    this.position.y = THREE.MathUtils.lerp(this.position.y, -6, 2 * dt);
    this.pose(0, 0);
    if (distance < LEAP_TRAVEL * 0.45) this.startLeap();
  }

  /** Leaves the water side-on to the camera, a little towards it, which
      is how a breach reads: the long body, the flippers and the roll. */
  private startLeap() {
    this.heading = Math.cos(this.heading) >= 0 ? LEAP_ANGLE : Math.PI - LEAP_ANGLE;
    this.phase = { kind: "leaping", elapsed: 0, start: this.position.clone(), heading: this.heading, surfaced: false, splashed: false };
  }

  /** Out along an arc, nose up, rolling onto its back, and down again. */
  private leap(phase: Extract<Phase, { kind: "leaping" }>, dt: number) {
    phase.elapsed += dt;
    const progress = Math.min(1, phase.elapsed / LEAP_SECONDS);
    const along = LEAP_TRAVEL * progress;
    this.position.set(
      phase.start.x + Math.cos(phase.heading) * along,
      -6 + LEAP_RISE * Math.sin(Math.PI * progress),
      phase.start.z - Math.sin(phase.heading) * along,
    );
    const pitch = THREE.MathUtils.lerp(1.25, -1.05, progress);
    const roll = THREE.MathUtils.smootherstep(progress, 0.15, 0.9) * 2.6;
    this.pose(pitch, roll);
    this.splashes(phase, progress);
    if (progress >= 1) this.phase = { kind: "recovering", elapsed: 0, roll };
  }

  private splashes(phase: Extract<Phase, { kind: "leaping" }>, progress: number) {
    if (!phase.surfaced && progress > 0.08) {
      phase.surfaced = true;
      this.onSplash(this.position.x, this.position.z, 0.7);
    }
    if (!phase.splashed && progress > 0.78) {
      phase.splashed = true;
      this.onSplash(this.position.x, this.position.z, 1.6);
    }
  }

  private recover(phase: Extract<Phase, { kind: "recovering" }>, dt: number) {
    phase.elapsed += dt;
    const progress = Math.min(1, phase.elapsed / RECOVER_SECONDS);
    this.advance(SWIM_SPEED * dt);
    this.position.y = THREE.MathUtils.lerp(this.position.y, SWIM_DEPTH, 2.5 * dt);
    this.pose(THREE.MathUtils.lerp(-1.05, 0, progress), phase.roll * (1 - THREE.MathUtils.smootherstep(progress, 0, 1)));
    if (progress < 1) return;
    this.phase = { kind: "swimming" };
    this.play("cruise_loop");
  }

  private turnTowards(angle: number, rate: number) {
    const difference = Math.atan2(Math.sin(angle - this.heading), Math.cos(angle - this.heading));
    this.heading += THREE.MathUtils.clamp(difference, -rate, rate);
  }

  private advance(distance: number) {
    this.position.x += Math.cos(this.heading) * distance;
    this.position.z -= Math.sin(this.heading) * distance;
  }

  private pose(pitch: number, roll: number) {
    this.root.position.copy(this.position);
    this.root.rotation.set(roll, this.heading, pitch, "YZX");
  }

  dispose() {
    this.mixer.stopAllAction();
  }
}
