import * as THREE from "three";
import { Breath } from "./breath";
import { letterStrokes } from "./letters";
import { Liquid, type TextLayer } from "./liquid";
import { paletteFor, type ScenePalette } from "./palette";
import { BREATH, BURST, STORY_HOLDS, STORY_STARTS, cameraAt, through, underwaterness, whaleAt, type Shot } from "./story";
import { Underwater } from "./underwater";
import { WaveField } from "./waveField";
import { Whale } from "./whale";
import { WordField, type Bounds } from "./words";

/** The sea of words and its scroll story. At the top it's free to play
    with: the pointer ripples the words and a click makes the whale breach.
    Scrolling dives under the surface, meets the whale, follows its breach
    up into the air and lands the words as the name. Every frame is drawn
    through a liquid layer the pointer stirs. */

const BOUNDS: Bounds = { minX: -46, maxX: 46, minZ: -72, maxZ: 16 };
const STEP = 1 / 60;
const FIRST_BREACH_SECONDS = 1.4;
const IDLE_BREACH_SECONDS = 13;
const NAME_CENTRE = new THREE.Vector3(0, 8, -10);
const STORY_BREACH = new THREE.Vector2(-1, -18);
const BREATH_FROM = new THREE.Vector3(9, 0.2, -24);
const SWIM_CLOCK = 46;

export type SeaOptions = {
  dark: boolean;
  reducedMotion: boolean;
  progress: () => number;
  text: TextLayer;
  onReady: () => void;
};
export type SeaScene = { dispose: () => void };

function makeRenderer(canvas: HTMLCanvasElement) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: false, powerPreference: "high-performance" });
  renderer.setPixelRatio(Math.min(2, window.devicePixelRatio || 1));
  renderer.setClearColor(0x000000, 0);
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
  return ground;
}

/** Narrow screens stand further back and see wider, so the same story
    still fits a phone held upright. */
function frameShot(camera: THREE.PerspectiveCamera, shot: Shot) {
  const narrow = camera.aspect < 1;
  camera.position.copy(shot.position);
  if (narrow) camera.position.add(new THREE.Vector3(0, 2.2, 7));
  camera.lookAt(shot.look);
  const fov = narrow ? 50 : 32;
  if (camera.fov !== fov) {
    camera.fov = fov;
    camera.updateProjectionMatrix();
  }
}

/** The name's width in metres: most of what the camera sees at the end. */
function nameWidth(camera: THREE.PerspectiveCamera): number {
  const end = cameraAt(1);
  const distance = end.position.z + (camera.aspect < 1 ? 7 : 0) - NAME_CENTRE.z;
  const visible = 2 * distance * Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)) * camera.aspect;
  return Math.min(26, visible * (camera.aspect < 1 ? 0.88 : 0.66));
}

/** Ripples where the pointer crosses the water, a breach where it clicks,
    and a stir of the liquid wherever it moves on the page. */
function watchPointer(
  canvas: HTMLCanvasElement,
  camera: THREE.PerspectiveCamera,
  handlers: { ripple: (x: number, z: number, amount: number) => void; breach: (x: number, z: number) => void; stir: (point: THREE.Vector2, force: THREE.Vector2) => void },
) {
  const ray = new THREE.Raycaster();
  const surface = new THREE.Plane(new THREE.Vector3(0, 1, 0), 0);
  const hit = new THREE.Vector3();
  let lastWater: THREE.Vector3 | null = null;
  let lastScreen: { x: number; y: number; at: number } | null = null;
  const uvOf = (event: PointerEvent) => {
    const box = canvas.getBoundingClientRect();
    return new THREE.Vector2((event.clientX - box.left) / box.width, 1 - (event.clientY - box.top) / box.height);
  };
  const waterOf = (uv: THREE.Vector2) => {
    ray.setFromCamera(new THREE.Vector2(uv.x * 2 - 1, uv.y * 2 - 1), camera);
    return ray.ray.intersectPlane(surface, hit) ? hit.clone() : null;
  };
  const onMove = (event: PointerEvent) => {
    const uv = uvOf(event);
    const now = performance.now();
    if (lastScreen) {
      const seconds = Math.max(0.008, (now - lastScreen.at) / 1000);
      handlers.stir(uv, new THREE.Vector2((uv.x - lastScreen.x) / seconds, (uv.y - lastScreen.y) / seconds).multiplyScalar(0.35));
    }
    lastScreen = { x: uv.x, y: uv.y, at: now };
    const point = waterOf(uv);
    if (!point) return;
    handlers.ripple(point.x, point.z, Math.min(0.5, (lastWater ? point.distanceTo(lastWater) : 0) * 0.09));
    lastWater = point;
  };
  const onDown = (event: PointerEvent) => {
    if (event.target !== canvas) return;
    const point = waterOf(uvOf(event));
    if (point) handlers.breach(point.x, point.z);
  };
  window.addEventListener("pointermove", onMove, { passive: true });
  window.addEventListener("pointerdown", onDown);
  return () => {
    window.removeEventListener("pointermove", onMove);
    window.removeEventListener("pointerdown", onDown);
  };
}

/** Fires the story's splash once each time the scroll crosses the burst,
    whichever way it's going. */
function burstCrossing() {
  let last = 0;
  return (progress: number, splash: () => void) => {
    const crossed = (last < BURST.from) !== (progress < BURST.from);
    if (crossed && progress > last) splash();
    last = progress;
  };
}

export async function createSeaScene(canvas: HTMLCanvasElement, options: SeaOptions): Promise<SeaScene> {
  const palette = paletteFor(options.dark);
  const renderer = makeRenderer(canvas);
  const scene = new THREE.Scene();
  scene.fog = new THREE.Fog(palette.paper, 30, 92);
  const camera = new THREE.PerspectiveCamera(32, 1, 0.3, 220);
  makeLights(scene, options.dark);
  const ground = makeGround(scene, palette);

  const water = new WaveField(BOUNDS, 0.8);
  const lineMaterial = new THREE.MeshStandardMaterial({ color: palette.line, roughness: 0.48, metalness: 0 });
  const words = new WordField(BOUNDS, lineMaterial);
  scene.add(...words.meshes);
  const underwater = new Underwater(palette, options.dark);
  scene.add(underwater.group);
  const breath = new Breath(options.dark ? 0xd9d4cb : 0x9a9aa2);
  scene.add(breath.points);
  const liquid = new Liquid(new THREE.Color(options.dark ? 0xf7f5f1 : 0x18181b));

  const splash = (x: number, z: number, strength: number) => {
    water.push(x, z, -1.4 * strength, 5 * strength);
    words.scatter(x, z, 6 * strength, 6.5 * strength);
  };
  const whale = await Whale.load("/whale.glb", palette, splash);
  scene.add(whale.root);

  let idle = FIRST_BREACH_SECONDS;
  let progress = 0;
  const breach = (x: number, z: number) => {
    if (progress > STORY_STARTS) return;
    idle = IDLE_BREACH_SECONDS;
    whale.breachAt(x, z);
  };
  const stopPointer = watchPointer(canvas, camera, {
    ripple: (x, z, amount) => camera.position.y > 0 && water.push(x, z, amount, 2.6),
    breach,
    stir: (point, force) => liquid.stir(point, force),
  });

  const resize = () => {
    const { clientWidth, clientHeight } = canvas;
    renderer.setSize(clientWidth, clientHeight, false);
    camera.aspect = clientWidth / Math.max(1, clientHeight);
    frameShot(camera, cameraAt(progress));
    camera.updateProjectionMatrix();
    const scale = renderer.getPixelRatio();
    liquid.resize(Math.round(clientWidth * scale), Math.round(clientHeight * scale), options.text, canvas.getBoundingClientRect(), scale);
    const width = nameWidth(camera);
    const pitch = width / 32;
    words.assignLetters(letterStrokes("Gasp", width, pitch), NAME_CENTRE, STORY_BREACH, pitch * 0.62);
  };
  const resizing = new ResizeObserver(resize);
  resizing.observe(canvas);
  void document.fonts?.ready.then(resize);
  resize();

  const storyBurst = burstCrossing();
  const tick = (dt: number) => {
    progress = options.progress();
    const free = progress < STORY_HOLDS;
    idle -= dt;
    if (free && idle <= 0 && whale.isSwimming && progress < STORY_STARTS) breach(camera.aspect < 1 ? 0 : 4 + Math.sin(performance.now()) * 3, -12);
    if (free) whale.update(dt);
    else whale.follow(whaleAt(progress), progress * SWIM_CLOCK);
    if (free && whale.isSwimming) water.push(whale.back.x, whale.back.z, 0.03, 4);
    storyBurst(progress, () => splash(STORY_BREACH.x, STORY_BREACH.y, 1.8));
    water.step(dt);
    words.update(water, dt, progress);
  };

  const draw = (seconds: number) => {
    frameShot(camera, cameraAt(progress));
    const depth = underwaterness(camera.position.y);
    whale.aboveWater = 1 - depth;
    ground.visible = depth < 0.5;
    underwater.update(depth, seconds, scene, camera);
    breath.update(through(BREATH, progress), BREATH_FROM, canvas.clientHeight);
    renderer.setRenderTarget(liquid.sceneTarget);
    renderer.clear();
    renderer.render(scene, camera);
    const hero = 1 - THREE.MathUtils.smoothstep(progress, 0.02, 0.08);
    const final = THREE.MathUtils.smoothstep(progress, 0.86, 0.93);
    liquid.present(renderer, underwater.haze, hero, final);
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
    draw(now / 1000);
  };
  const seeing = new IntersectionObserver(([entry]) => {
    visible = entry.isIntersecting;
    last = performance.now();
  });
  seeing.observe(canvas);

  if (options.reducedMotion) {
    whale.update(0);
    words.update(water, 0, 0);
    draw(0);
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
      underwater.dispose();
      breath.dispose();
      liquid.dispose();
      lineMaterial.dispose();
      renderer.dispose();
    },
  };
}
