import * as THREE from "three";

/** The screen as liquid: a small velocity field that the pointer stirs,
    carried along by its own flow and slowly settling, through which the
    finished frame is drawn. Display type lives in the background layer of
    that frame, behind the whale and the words, and bends further than they
    do, so the name smears like ink in water when the pointer passes. */

const FIELD_SCALE = 1 / 6;
const DISSIPATION = 0.965;
const SPLAT_RADIUS = 0.0035;

const QUAD_VERTEX = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = vec4(position.xy, 0.0, 1.0);
  }`;

const ADVECT = /* glsl */ `
  uniform sampler2D uVelocity;
  uniform float uDissipation;
  uniform float uDt;
  varying vec2 vUv;
  void main() {
    vec2 velocity = texture2D(uVelocity, vUv).xy;
    vec2 from = vUv - velocity * uDt;
    gl_FragColor = vec4(texture2D(uVelocity, from).xy * uDissipation, 0.0, 1.0);
  }`;

const SPLAT = /* glsl */ `
  uniform sampler2D uVelocity;
  uniform vec2 uPoint;
  uniform vec2 uForce;
  uniform float uRadius;
  uniform float uAspect;
  varying vec2 vUv;
  void main() {
    vec2 d = vUv - uPoint;
    d.x *= uAspect;
    float falloff = exp(-dot(d, d) / uRadius);
    vec2 velocity = texture2D(uVelocity, vUv).xy + uForce * falloff;
    gl_FragColor = vec4(velocity, 0.0, 1.0);
  }`;

const COMPOSITE = /* glsl */ `
  uniform sampler2D uScene;
  uniform sampler2D uVelocity;
  uniform sampler2D uText;
  uniform vec3 uHaze;
  uniform vec3 uInk;
  uniform float uHero;
  uniform float uFinal;
  varying vec2 vUv;
  void main() {
    vec2 velocity = texture2D(uVelocity, vUv).xy;
    vec2 shift = velocity * 0.045;
    float split = clamp(length(velocity) * 0.9, 0.0, 1.0) * 0.006;
    vec4 base = texture2D(uScene, vUv - shift);
    float red = texture2D(uScene, vUv - shift + vec2(split, 0.0)).r;
    float blue = texture2D(uScene, vUv - shift - vec2(split, 0.0)).b;
    vec2 text = texture2D(uText, vUv - shift * 1.5).rg;
    float ink = clamp(text.r * uHero + text.g * uFinal, 0.0, 1.0);
    vec3 background = mix(uHaze, uInk, ink);
    vec3 scene = vec3(red, base.g, blue);
    gl_FragColor = vec4(scene + background * (1.0 - base.a), 1.0);
  }`;

type Pass = { scene: THREE.Scene; material: THREE.ShaderMaterial };

function makePass(fragmentShader: string, uniforms: Record<string, THREE.IUniform>): Pass {
  const material = new THREE.ShaderMaterial({ vertexShader: QUAD_VERTEX, fragmentShader, uniforms, depthTest: false, depthWrite: false });
  const scene = new THREE.Scene();
  scene.add(new THREE.Mesh(new THREE.PlaneGeometry(2, 2), material));
  return { scene, material };
}

function fieldTarget(width: number, height: number) {
  return new THREE.WebGLRenderTarget(width, height, {
    type: THREE.HalfFloatType,
    minFilter: THREE.LinearFilter,
    magFilter: THREE.LinearFilter,
    depthBuffer: false,
  });
}

/** Draws the display type, in its page position and font, into a canvas
    the composite reads: the name in red, the closing line in green. */
export type TextLayer = { hero: HTMLElement | null; final: HTMLElement | null };

function drawText(canvas: HTMLCanvasElement, layer: TextLayer, frame: DOMRect, scale: number) {
  const context = canvas.getContext("2d");
  if (!context) return;
  context.clearRect(0, 0, canvas.width, canvas.height);
  context.globalCompositeOperation = "lighter";
  const entries: [HTMLElement | null, string][] = [[layer.hero, "#ff0000"], [layer.final, "#00ff00"]];
  for (const [element, colour] of entries) {
    if (!element) continue;
    const box = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    context.font = `${style.fontWeight} ${parseFloat(style.fontSize) * scale}px ${style.fontFamily}`;
    context.fillStyle = colour;
    context.textBaseline = "alphabetic";
    const lineHeight = parseFloat(style.lineHeight) || parseFloat(style.fontSize);
    const ascentShare = 0.78;
    const centred = style.textAlign === "center";
    context.textAlign = centred ? "center" : "left";
    const x = (box.left - frame.left + (centred ? box.width / 2 : 0)) * scale;
    const y = (box.top - frame.top + (lineHeight - parseFloat(style.fontSize)) / 2 + parseFloat(style.fontSize) * ascentShare) * scale;
    context.fillText(element.textContent ?? "", x, y);
  }
}

export class Liquid {
  readonly sceneTarget: THREE.WebGLRenderTarget;
  private read: THREE.WebGLRenderTarget;
  private write: THREE.WebGLRenderTarget;
  private readonly advect: Pass;
  private readonly splat: Pass;
  private readonly composite: Pass;
  private readonly camera = new THREE.Camera();
  private readonly textCanvas = document.createElement("canvas");
  private readonly textTexture: THREE.CanvasTexture;
  private readonly pending: { point: THREE.Vector2; force: THREE.Vector2 }[] = [];

  constructor(ink: THREE.Color) {
    this.sceneTarget = new THREE.WebGLRenderTarget(1, 1, { samples: 4, type: THREE.HalfFloatType });
    this.read = fieldTarget(1, 1);
    this.write = fieldTarget(1, 1);
    this.textTexture = new THREE.CanvasTexture(this.textCanvas);
    this.advect = makePass(ADVECT, { uVelocity: { value: null }, uDissipation: { value: DISSIPATION }, uDt: { value: 1 / 60 } });
    this.splat = makePass(SPLAT, {
      uVelocity: { value: null }, uPoint: { value: new THREE.Vector2() }, uForce: { value: new THREE.Vector2() },
      uRadius: { value: SPLAT_RADIUS }, uAspect: { value: 1 },
    });
    this.composite = makePass(COMPOSITE, {
      uScene: { value: this.sceneTarget.texture }, uVelocity: { value: null }, uText: { value: this.textTexture },
      uHaze: { value: new THREE.Color() }, uInk: { value: ink }, uHero: { value: 1 }, uFinal: { value: 0 },
    });
  }

  resize(width: number, height: number, layer: TextLayer, frame: DOMRect, scale: number) {
    this.sceneTarget.setSize(width, height);
    const fieldWidth = Math.max(32, Math.round(width * FIELD_SCALE));
    const fieldHeight = Math.max(32, Math.round(height * FIELD_SCALE));
    this.read.setSize(fieldWidth, fieldHeight);
    this.write.setSize(fieldWidth, fieldHeight);
    this.splat.material.uniforms.uAspect.value = width / Math.max(1, height);
    this.textCanvas.width = width;
    this.textCanvas.height = height;
    drawText(this.textCanvas, layer, frame, scale);
    this.textTexture.needsUpdate = true;
  }

  /** Stirs the liquid at `point` (0 to 1 across and up the frame) with a
      push of `force`, in frame-widths per second. */
  stir(point: THREE.Vector2, force: THREE.Vector2) {
    this.pending.push({ point: point.clone(), force: force.clone() });
  }

  /** Moves the liquid on, then draws the scene's frame through it. */
  present(renderer: THREE.WebGLRenderer, haze: THREE.Color, hero: number, final: number) {
    this.run(renderer, this.advect, { uVelocity: this.read.texture });
    for (const { point, force } of this.pending.splice(0)) {
      this.run(renderer, this.splat, { uVelocity: this.read.texture, uPoint: point, uForce: force });
    }
    const uniforms = this.composite.material.uniforms;
    uniforms.uVelocity.value = this.read.texture;
    uniforms.uHaze.value.copy(haze);
    uniforms.uHero.value = hero;
    uniforms.uFinal.value = final;
    renderer.setRenderTarget(null);
    renderer.render(this.composite.scene, this.camera);
  }

  private run(renderer: THREE.WebGLRenderer, pass: Pass, values: Record<string, unknown>) {
    for (const [name, value] of Object.entries(values)) {
      const uniform = pass.material.uniforms[name];
      if (value instanceof THREE.Vector2) (uniform.value as THREE.Vector2).copy(value);
      else uniform.value = value;
    }
    renderer.setRenderTarget(this.write);
    renderer.render(pass.scene, this.camera);
    [this.read, this.write] = [this.write, this.read];
  }

  dispose() {
    for (const target of [this.sceneTarget, this.read, this.write]) target.dispose();
    for (const pass of [this.advect, this.splat, this.composite]) pass.material.dispose();
    this.textTexture.dispose();
  }
}
