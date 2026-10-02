import * as THREE from "three";
import type { ScenePalette } from "./palette";

/** What the camera sees once it's under the words: the surface as a
    bright, gently moving ceiling, shafts of light slanting down through
    it, and a haze that closes in, cooler than the paper above. */

const SURFACE_SHADER = {
  vertex: /* glsl */ `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }`,
  fragment: /* glsl */ `
    uniform float uTime;
    uniform float uStrength;
    uniform vec3 uColor;
    varying vec2 vUv;
    float wave(vec2 p) {
      return sin(p.x * 9.0 + uTime * 0.7) * sin(p.y * 7.0 - uTime * 0.5)
        + 0.5 * sin((p.x + p.y) * 17.0 + uTime * 1.1);
    }
    void main() {
      float shimmer = 0.5 + 0.5 * wave(vUv * 6.0);
      float edge = smoothstep(0.5, 0.15, distance(vUv, vec2(0.5)));
      gl_FragColor = vec4(uColor, uStrength * edge * (0.55 + 0.35 * shimmer));
    }`,
};

const SHAFT_SHADER = {
  vertex: /* glsl */ `
    varying vec2 vUv;
    void main() {
      vUv = uv;
      gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
    }`,
  fragment: /* glsl */ `
    uniform float uStrength;
    uniform float uTime;
    uniform float uSeed;
    varying vec2 vUv;
    void main() {
      float across = smoothstep(0.0, 0.5, vUv.x) * smoothstep(1.0, 0.5, vUv.x);
      float down = smoothstep(0.0, 0.85, vUv.y);
      float flicker = 0.75 + 0.25 * sin(uTime * 0.8 + uSeed * 6.0);
      gl_FragColor = vec4(vec3(1.0), uStrength * across * down * flicker * 0.22);
    }`,
};

function makeSurface(palette: ScenePalette) {
  const material = new THREE.ShaderMaterial({
    uniforms: { uTime: { value: 0 }, uStrength: { value: 0 }, uColor: { value: new THREE.Color(palette.paper).lerp(new THREE.Color(0xffffff), 0.6) } },
    vertexShader: SURFACE_SHADER.vertex,
    fragmentShader: SURFACE_SHADER.fragment,
    transparent: true,
    depthWrite: false,
    side: THREE.DoubleSide,
  });
  const surface = new THREE.Mesh(new THREE.PlaneGeometry(140, 140), material);
  surface.rotation.x = Math.PI / 2;
  surface.position.set(0, 0.35, -20);
  surface.renderOrder = -1;
  return { surface, material };
}

function makeShafts() {
  const shafts: { mesh: THREE.Mesh; material: THREE.ShaderMaterial }[] = [];
  const spots: [number, number, number][] = [
    [-9, -18, 0.18], [-3, -26, -0.08], [4, -14, 0.12], [10, -30, -0.15], [1, -40, 0.05], [-14, -34, 0.2],
  ];
  spots.forEach(([x, z, lean], index) => {
    const material = new THREE.ShaderMaterial({
      uniforms: { uStrength: { value: 0 }, uTime: { value: 0 }, uSeed: { value: index } },
      vertexShader: SHAFT_SHADER.vertex,
      fragmentShader: SHAFT_SHADER.fragment,
      transparent: true,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      side: THREE.DoubleSide,
    });
    const mesh = new THREE.Mesh(new THREE.PlaneGeometry(3.2 + (index % 3), 26), material);
    mesh.position.set(x, -12, z);
    mesh.rotation.z = lean;
    shafts.push({ mesh, material });
  });
  return shafts;
}

export class Underwater {
  readonly group = new THREE.Group();
  private readonly surface: ReturnType<typeof makeSurface>;
  private readonly shafts = makeShafts();
  private readonly above: THREE.Color;
  private readonly below: THREE.Color;
  private readonly mixed = new THREE.Color();

  constructor(palette: ScenePalette, dark: boolean) {
    this.surface = makeSurface(palette);
    this.group.add(this.surface.surface, ...this.shafts.map((shaft) => shaft.mesh));
    this.above = new THREE.Color(palette.paper);
    this.below = new THREE.Color(dark ? 0x0f1416 : 0xdde3e8);
  }

  /** Blends the haze, the background and the light by how far under the
      camera is (`depth`, 0 to 1), facing the shafts towards it. */
  update(depth: number, time: number, scene: THREE.Scene, camera: THREE.Camera) {
    this.group.visible = depth > 0.01;
    this.surface.material.uniforms.uStrength.value = depth;
    this.surface.material.uniforms.uTime.value = time;
    for (const shaft of this.shafts) {
      shaft.material.uniforms.uStrength.value = depth;
      shaft.material.uniforms.uTime.value = time;
      shaft.mesh.rotation.y = Math.atan2(camera.position.x - shaft.mesh.position.x, camera.position.z - shaft.mesh.position.z);
    }
    this.mixed.copy(this.above).lerp(this.below, depth);
    const fog = scene.fog as THREE.Fog;
    fog.color.copy(this.mixed);
    fog.near = THREE.MathUtils.lerp(30, 4, depth);
    fog.far = THREE.MathUtils.lerp(92, 58, depth);
  }

  get haze(): THREE.Color {
    return this.mixed;
  }

  dispose() {
    this.surface.surface.geometry.dispose();
    this.surface.material.dispose();
    for (const shaft of this.shafts) {
      shaft.mesh.geometry.dispose();
      shaft.material.dispose();
    }
  }
}
