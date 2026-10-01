import * as THREE from "three";
import { paletteFor, type ScenePalette } from "./palette";
import { WaveField } from "./waveField";
import { Whale } from "./whale";
import { WordField, type Bounds } from "./words";

/** The hero's sea of words: a note's lines floating on water seen from
    above, a humpback swimming under them, ripples where the pointer moves
    and a breach where it clicks. */

const BOUNDS: Bounds = { minX: -46, maxX: 46, minZ: -72, maxZ: 16 };
const STEP = 1 / 60;
const FIRST_BREACH_SECONDS = 1.4;
const IDLE_BREACH_SECONDS = 13;

export type SeaOptions = { dark: boolean; reducedMotion: boolean; onReady: () => void };
export type SeaScene = { dispose: () => void };

function makeRenderer(canvas: HTMLCanvasElement, palette: ScenePalette) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, alpha: true, powerPreference: "high-performance" });
  renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
  renderer.setClearColor(palette.paper, 0);
  renderer.shadowMap.enabled = true;
  renderer.shadowMap.type = THREE.VSMShadowMap;
  return renderer;
}

function makeLights(scene: THREE.Scene, dark: boolean) {
  scene.add(new THREE.HemisphereLight(0xffffff, dark ? 0x1d1c1a : 0xdcdce2, dark ? 1.1 : 1.7));
  const sun = new THREE.DirectionalLight(0xffffff, dark ? 1.2 : 1.5);
  sun.position.set(-20, 36, 18);
  sun.target.position.set(0, 0, -12);
  sun.castShadow = true;
  sun.shadow.mapSize.set(2048, 2048);
  sun.shadow.radius = 10;
  sun.shadow.blurSamples = 16;
  sun.shadow.bias = -0.0004;
  const reach = 60;
  Object.assign(sun.shadow.camera, { left: -reach, right: reach, top: reach, bottom: -reach, near: 1, far: 120 });
  scene.add(sun, sun.target);
}

function makeGround(scene: THREE.Scene, palette: ScenePalette) {
  const ground = new THREE.Mesh(
    new THREE.PlaneGeometry(200, 200),
    new THREE.ShadowMaterial({ color: palette.shadow, opacity: 0.14, depthWrite: false }),
  );
  ground.rotation.x = -Math.PI / 2;
  ground.position.y = -1.1;
  ground.receiveShadow = true;
  scene.add(ground);
}

/** Looks low across the page of words to a horizon that fades into the
    paper, leaving open sky above it for the name and the breach. Narrow
    screens stand further back. */
function placeCamera(camera: THREE.PerspectiveCamera, width: number, height: number) {
  const aspect = width / Math.max(1, height);
  const narrow = aspect < 1;
  camera.aspect = aspect;
  camera.fov = narrow ? 48 : 32;
  camera.position.set(0, narrow ? 11 : 8.5, narrow ? 34 : 30);
  camera.lookAt(0, narrow ? 0.4 : 3.6, -20);
  camera.updateProjectionMatrix();
}

/** Turns pointer moves into ripples and clicks into breaches. */
function watchPointer(canvas: HTMLCanvasElement, camera: THREE.PerspectiveCamera, water: WaveField, onBreach: (x: number, z: number) => void) {
  const ray = new THREE.Raycaster();
  const surface = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  const hit = new THREE.Vector3();
  let last: THREE.Vector3 | null = null;
  const pointOf = (event: PointerEvent) => {
    const box = canvas.getBoundingClientRect();
    const ndc = new THREE.Vector2(((event.clientX - box.left) / box.width) * 2 - 1, -((event.clientY - box.top) / box.height) * 2 + 1);
    ray.setFromCamera(ndc, camera);
    return ray.ray.intersectPlane(surface, hit) ? hit.clone() : null;
  };
  const onMove = (event: PointerEvent) => {
    const point = pointOf(event);
    if (!point) return;
    const travelled = last ? point.distanceTo(last) : 0;
    last = point;
    water.push(point.x, point.z, Math.min(0.5, travelled * 0.09), 2.6);
  };
  const onDown = (event: PointerEvent) => {
    const point = pointOf(event);
    if (point) onBreach(point.x, point.z);
  };
  const onLeave = () => (last = null);
  canvas.addEventListener("pointermove", onMove);
  canvas.addEventListener("pointerdown", onDown);
  canvas.addEventListener("pointerleave", onLeave);
  return () => {
    canvas.removeEventListener("pointermove", onMove);
    canvas.removeEventListener("pointerdown", onDown);
    canvas.removeEventListener("pointerleave", onLeave);
  };
}

/** Where the whale breaches on its own: centre-right on a wide screen,
    beside the name, and the middle of a narrow one. */
function idleBreachX(camera: THREE.PerspectiveCamera): number {
  const wobble = Math.sin(performance.now()) * 3;
  return camera.aspect < 1 ? wobble * 0.5 : 4 + wobble;
}

export async function createSeaScene(canvas: HTMLCanvasElement, options: SeaOptions): Promise<SeaScene> {
  const palette = paletteFor(options.dark);
  const renderer = makeRenderer(canvas, palette);
  const scene = new THREE.Scene();
  scene.fog = new THREE.Fog(palette.paper, 30, 92);
  const camera = new THREE.PerspectiveCamera(30, 1, 0.5, 200);
  makeLights(scene, options.dark);
  makeGround(scene, palette);

  const water = new WaveField(BOUNDS, 0.8);
  const lineMaterial = new THREE.MeshStandardMaterial({ color: palette.line, roughness: 0.48, metalness: 0 });
  const words = new WordField(BOUNDS, lineMaterial);
  scene.add(...words.meshes);

  const splash = (x: number, z: number, strength: number) => {
    water.push(x, z, -1.4 * strength, 5 * strength);
    words.scatter(x, z, 6 * strength, 6.5 * strength);
  };
  const whale = await Whale.load("/whale.glb", palette, splash);
  scene.add(whale.root);

  let idle = FIRST_BREACH_SECONDS;
  const breach = (x: number, z: number) => {
    idle = IDLE_BREACH_SECONDS;
    whale.breachAt(x, z);
  };
  const stopPointer = watchPointer(canvas, camera, water, breach);

  const resize = () => {
    const { clientWidth, clientHeight } = canvas;
    renderer.setSize(clientWidth, clientHeight, false);
    placeCamera(camera, clientWidth, clientHeight);
  };
  const resizing = new ResizeObserver(resize);
  resizing.observe(canvas);
  resize();

  const tick = (dt: number) => {
    idle -= dt;
    if (idle <= 0 && whale.isSwimming) breach(idleBreachX(camera), -12);
    whale.update(dt);
    if (whale.isSwimming) water.push(whale.back.x, whale.back.z, 0.03, 4);
    water.step(dt);
    words.update(water, dt);
  };

  const speed = Number(new URLSearchParams(window.location.search).get("sea-speed")) || 1;
  let frame = 0;
  let last = performance.now();
  let spare = 0;
  let visible = true;
  const loop = (now: number) => {
    frame = requestAnimationFrame(loop);
    if (!visible) return;
    spare = Math.min(0.1, spare + ((now - last) / 1000) * speed);
    last = now;
    while (spare >= STEP) {
      tick(STEP);
      spare -= STEP;
    }
    renderer.render(scene, camera);
  };
  const seeing = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    last = performance.now();
  });
  seeing.observe(canvas);

  if (options.reducedMotion) {
    whale.update(0);
    words.update(water, 0);
    renderer.render(scene, camera);
  } else {
    frame = requestAnimationFrame(loop);
  }
  options.onReady();

  return {
    dispose: () => {
      cancelAnimationFrame(frame);
      stopPointer();
      resizing.disconnect();
      seeing.disconnect();
      whale.dispose();
      words.dispose();
      lineMaterial.dispose();
      renderer.dispose();
    },
  };
}
