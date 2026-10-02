import * as THREE from "three";

/** The whale's blow: a column of mist that rises from the water, spreads
    and drifts, drawn as soft points. Its shape is a function of `t`
    (0 to 1), so scrolling scrubs it. */

const COUNT = 520;
const RISE = 9;

const SHADER = {
  vertex: /* glsl */ `
    attribute vec3 aDrift;
    attribute float aDelay;
    uniform float uT;
    uniform float uScale;
    varying float vAlpha;
    void main() {
      float t = clamp((uT - aDelay) / (1.0 - aDelay), 0.0, 1.0);
      float rise = 1.0 - pow(1.0 - t, 2.2);
      vec3 p = position + vec3(aDrift.x * t * 4.0, rise * ${RISE.toFixed(1)} * aDrift.y - t * t * 2.0, aDrift.z * t * 3.0);
      vAlpha = smoothstep(0.0, 0.08, t) * (1.0 - smoothstep(0.55, 1.0, t));
      vec4 view = modelViewMatrix * vec4(p, 1.0);
      gl_PointSize = uScale * (0.35 + t * 1.6) / -view.z;
      gl_Position = projectionMatrix * view;
    }`,
  fragment: /* glsl */ `
    uniform vec3 uColor;
    varying float vAlpha;
    void main() {
      float d = length(gl_PointCoord - 0.5);
      float soft = smoothstep(0.5, 0.0, d);
      gl_FragColor = vec4(uColor, soft * vAlpha * 0.32);
    }`,
};

function noise(seed: number): number {
  const x = Math.sin(seed * 12.9898) * 43758.5453;
  return x - Math.floor(x);
}

export class Breath {
  readonly points: THREE.Points;
  private readonly material: THREE.ShaderMaterial;

  constructor(colour: number) {
    const positions = new Float32Array(COUNT * 3);
    const drifts = new Float32Array(COUNT * 3);
    const delays = new Float32Array(COUNT);
    for (let index = 0; index < COUNT; index++) {
      const angle = noise(index + 1) * Math.PI * 2;
      const spread = noise(index + 50) * 0.35;
      positions.set([Math.cos(angle) * spread, 0, Math.sin(angle) * spread], index * 3);
      drifts.set([(noise(index + 100) - 0.5) * 1.2, 0.55 + noise(index + 200) * 0.6, (noise(index + 300) - 0.5) * 0.8], index * 3);
      delays[index] = noise(index + 400) * 0.25;
    }
    const geometry = new THREE.BufferGeometry();
    geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    geometry.setAttribute("aDrift", new THREE.BufferAttribute(drifts, 3));
    geometry.setAttribute("aDelay", new THREE.BufferAttribute(delays, 1));
    this.material = new THREE.ShaderMaterial({
      uniforms: { uT: { value: 0 }, uScale: { value: 900 }, uColor: { value: new THREE.Color(colour) } },
      vertexShader: SHADER.vertex,
      fragmentShader: SHADER.fragment,
      transparent: true,
      depthWrite: false,
    });
    this.points = new THREE.Points(geometry, this.material);
    this.points.frustumCulled = false;
  }

  /** Shows the blow `t` of the way through, from where the whale surfaced. */
  update(t: number, from: THREE.Vector3, pixelHeight: number) {
    this.points.visible = t > 0 && t < 1;
    this.points.position.copy(from);
    this.material.uniforms.uT.value = t;
    this.material.uniforms.uScale.value = pixelHeight * 0.9;
  }

  dispose() {
    this.points.geometry.dispose();
    this.material.dispose();
  }
}
