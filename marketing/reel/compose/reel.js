// The Gasp launch reel, drawn one frame at a time.
//
// stage.html?format=hero|vertical loads this. render.js calls
// window.renderFrame(t) for t = frame / 60 and screenshots the stage; the
// same timeline gives window.cues(), the soundtrack's cue list. Everything
// is a pure function of t, so a frame renders the same every time.
//
// App footage comes from the unpacked takes (../frames/<take>/NNNNN.png,
// one image per frame that changed on screen) through data.json, written
// by prep.py. A take is played on its nominal clock (see capture/take.py):
// frameAt() maps a nominal time to the wall-clock time it was recorded at,
// and that to the frame on screen then.

"use strict";

const Q = new URLSearchParams(location.search);
const FORMAT = Q.get("format") || "hero";
const HERO = FORMAT === "hero";
const W = HERO ? 1920 : 1080;
const H = HERO ? 1080 : 1920;
const FPS = 60;
const BEAT = 60 / 124;
const SRC_W = 2560;
const SRC_H = 1600;
const b = (n) => n * BEAT;

const C = {
  paper: "#F3EFE6",
  paperDeep: "#E9E3D7",
  ink: "#1B1A17",
  inkSoft: "rgba(27, 26, 23, 0.58)",
  inkFaint: "rgba(27, 26, 23, 0.36)",
  line: "rgba(27, 26, 23, 0.13)",
  red: "#C02B4A",
  black: "#0A0A0B",
  appBg: "#F6F6F7",
};

// ---- Loading ------------------------------------------------------------

let DATA = null;
let PEAKS = null;
let WHALE = null;
const cache = new Map();

function loadImage(url) {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("can't load " + url));
    image.src = url;
  });
}

async function getFrame(take, index) {
  const url = `../frames/${take}/${String(index).padStart(5, "0")}.png`;
  if (cache.has(url)) {
    const hit = cache.get(url);
    cache.delete(url);
    cache.set(url, hit);
    return hit;
  }
  const image = await loadImage(url);
  await image.decode();
  cache.set(url, image);
  while (cache.size > 24) cache.delete(cache.keys().next().value);
  return image;
}

window.ready = (async () => {
  DATA = await (await fetch("data.json")).json();
  PEAKS = await (await fetch("peaks.json")).json();
  WHALE = await loadImage("whale-breach.png");
  const faces = [];
  for (const weight of [300, 400, 500, 600, 700]) {
    faces.push(document.fonts.load(`${weight} 40px Bricolage`));
  }
  faces.push(document.fonts.load("400 20px 'JB Mono'"), document.fonts.load("400 40px Charter"),
    document.fonts.load("700 40px Charter"));
  await Promise.all(faces);
  setup();
  return true;
})();

// ---- Maths ----------------------------------------------------------------

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const lerp = (a, z, u) => a + (z - a) * u;
const unlerp = (a, z, v) => clamp((v - a) / (z - a), 0, 1);
const easeInOut = (u) => (u < 0.5 ? 4 * u * u * u : 1 - Math.pow(-2 * u + 2, 3) / 2);
const easeOut = (u) => 1 - Math.pow(1 - u, 3);
const easeIn = (u) => u * u * u;
const easeOutBack = (u) => { const c = 1.4; return 1 + (c + 1) * Math.pow(u - 1, 3) + c * Math.pow(u - 1, 2); };

function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x6d2b79f5) >>> 0;
    let x = s;
    x = Math.imul(x ^ (x >>> 15), x | 1);
    x ^= x + Math.imul(x ^ (x >>> 7), x | 61);
    return ((x ^ (x >>> 14)) >>> 0) / 4294967296;
  };
}

// ---- Takes ----------------------------------------------------------------

function realAt(take, n) {
  const a = take.anchors;
  if (n <= a[0][0]) return a[0][1] + (n - a[0][0]);
  const last = a[a.length - 1];
  if (n >= last[0]) return last[1] + (n - last[0]);
  let lo = 0;
  let hi = a.length - 1;
  while (hi - lo > 1) {
    const mid = (lo + hi) >> 1;
    if (a[mid][0] <= n) lo = mid; else hi = mid;
  }
  const [n0, r0] = a[lo];
  const [n1, r1] = a[hi];
  if (n1 === n0) return r1;
  return lerp(r0, r1, (n - n0) / (n1 - n0));
}

function frameAt(takeName, n) {
  const take = DATA.takes[takeName];
  const tick = clamp(Math.floor(realAt(take, n) * FPS + 1e-6), 0, take.ticks.length - 1);
  return Math.max(0, take.ticks[tick]);
}

function caretAt(takeName, n) {
  const take = DATA.takes[takeName];
  let index = frameAt(takeName, n);
  for (let i = index; i >= 0; i--) {
    const c = take.carets[i];
    if (c) return { x: c[0], y: (c[1] + c[2]) / 2, h: c[2] - c[1] };
  }
  return null;
}

// The caret, averaged over the last `lag` seconds, so a camera that
// follows it glides instead of jumping a character at a time.
function caretFollow(takeName, n, lag = 0.35) {
  let sx = 0;
  let sy = 0;
  let count = 0;
  for (let i = 0; i < 12; i++) {
    const c = caretAt(takeName, n - lag * (i / 11));
    if (c) { sx += c.x; sy += c.y; count++; }
  }
  return count ? { x: sx / count, y: sy / count } : null;
}

function eventsIn(takeName, n0, n1) {
  return DATA.takes[takeName].events.filter((e) => e.nominal >= n0 && e.nominal < n1 &&
    ["char", "key", "click", "press", "release"].includes(e.what));
}

// ---- Drawing: footage ----------------------------------------------------

async function drawFootage(ctx, takeName, n, cam, rect = { x: 0, y: 0, w: W, h: H }) {
  const image = await getFrame(takeName, frameAt(takeName, n));
  const z = Math.max(cam.z, rect.w / SRC_W, rect.h / SRC_H);
  const sw = rect.w / z;
  const sh = rect.h / z;
  const sx = clamp(cam.cx - sw / 2, 0, SRC_W - sw);
  const sy = clamp(cam.cy - sh / 2, 0, SRC_H - sh);
  ctx.save();
  ctx.beginPath();
  ctx.rect(rect.x, rect.y, rect.w, rect.h);
  ctx.clip();
  ctx.imageSmoothingEnabled = true;
  ctx.imageSmoothingQuality = "high";
  ctx.drawImage(image, sx, sy, sw, sh, rect.x, rect.y, rect.w, rect.h);
  ctx.restore();
}

// Camera keyframes: [[t, {cx, cy, z}], ...], eased between, with an
// optional hard cut (`cut: true` on a key jumps to it).
function camAt(keys, t) {
  if (t <= keys[0][0]) return keys[0][1];
  for (let i = 0; i < keys.length - 1; i++) {
    const [t0, k0] = keys[i];
    const [t1, k1] = keys[i + 1];
    if (t < t1) {
      if (k1.cut) return k0;
      const u = (k1.ease || easeInOut)(unlerp(t0, t1, t));
      return { cx: lerp(k0.cx, k1.cx, u), cy: lerp(k0.cy, k1.cy, u), z: lerp(k0.z, k1.z, u) };
    }
  }
  return keys[keys.length - 1][1];
}

// ---- Drawing: type (DOM, so the browser sets it properly) ---------------

const TYPE = () => document.getElementById("type");

function text(str, o) {
  const el = document.createElement("div");
  el.className = "t" + (o.cls ? " " + o.cls : "");
  el.textContent = str;
  const s = el.style;
  s.left = `${o.x}px`;
  s.top = `${o.y}px`;
  s.fontSize = `${o.size}px`;
  s.fontWeight = o.weight || 560;
  s.color = o.color || C.ink;
  if (o.stretch) s.fontStretch = `${o.stretch}%`;
  if (o.opacity !== undefined) s.opacity = o.opacity;
  if (o.lineHeight) s.lineHeight = o.lineHeight;
  if (o.width) { s.width = `${o.width}px`; s.whiteSpace = "normal"; }
  const shift = o.align === "center" ? "-50%" : o.align === "right" ? "-100%" : "0";
  s.transform = `translate(${shift}, 0) ${o.transform || ""}`;
  if (o.align === "center") s.textAlign = "center";
  (o.parent || TYPE()).appendChild(el);
  return el;
}

function box(o) {
  const el = document.createElement("div");
  const s = el.style;
  s.position = "absolute";
  s.left = `${o.x}px`;
  s.top = `${o.y}px`;
  s.width = `${o.w}px`;
  s.height = `${o.h}px`;
  s.background = o.fill || C.paper;
  s.borderRadius = `${o.radius ?? 18}px`;
  if (o.shadow !== false) s.boxShadow = "0 18px 50px rgba(20, 18, 14, 0.18), 0 2px 6px rgba(20, 18, 14, 0.10)";
  if (o.opacity !== undefined) s.opacity = o.opacity;
  if (o.transform) s.transform = o.transform;
  TYPE().appendChild(el);
  return el;
}

// Words that surface one by one through a waterline and breathe out to
// full width as they arrive. `t` is the time since the line started.
function kinetic(words, o, t) {
  const line = document.createElement("div");
  line.className = "t";
  const s = line.style;
  s.left = `${o.x}px`;
  s.top = `${o.y}px`;
  s.fontSize = `${o.size}px`;
  s.fontWeight = o.weight || 600;
  s.color = o.color || C.ink;
  s.letterSpacing = "0";
  if (o.align === "center") s.transform = "translate(-50%, 0)";
  const stagger = o.stagger ?? BEAT / 2;
  let lastShown = -1;
  words.forEach((word, i) => {
    const u = clamp((t - i * stagger) / 0.32, 0, 1);
    if (u > 0) lastShown = i;
    const mask = document.createElement("span");
    mask.style.display = "inline-block";
    mask.style.overflow = "hidden";
    mask.style.verticalAlign = "top";
    mask.style.paddingBottom = "0.3em";
    mask.style.marginBottom = "-0.3em";
    const inner = document.createElement("span");
    inner.style.display = "inline-block";
    inner.textContent = word;
    const e = easeOut(u);
    inner.style.transform = `translateY(${(1 - e) * 1.05}em)`;
    inner.style.fontStretch = `${lerp(78, 100, easeOut(clamp(u * 1.4, 0, 1)))}%`;
    mask.appendChild(inner);
    line.appendChild(mask);
    if (i < words.length - 1) line.appendChild(document.createTextNode(" "));
  });
  (o.parent || TYPE()).appendChild(line);
  return { line, lastShown };
}

// The note's text lines from the icon, with the red caret: the waterline.
function waterline(ctx, o, t) {
  const lengths = o.lengths || [0.86, 1.0, 0.74, 0.94, 0.6];
  ctx.save();
  for (let i = 0; i < (o.count || 3); i++) {
    const len = o.width * lengths[i % lengths.length];
    const y = o.y + i * o.gap;
    ctx.fillStyle = o.color || C.line;
    roundRect(ctx, o.x, y, len, o.thick, o.thick / 2);
    ctx.fill();
  }
  ctx.restore();
}

function caretBar(ctx, x, y, h, t, blink = true) {
  const on = !blink || Math.floor(t / (BEAT)) % 2 === 0;
  if (!on) return;
  ctx.fillStyle = C.red;
  roundRect(ctx, x, y, Math.max(3, h * 0.07), h, Math.max(1.5, h * 0.035));
  ctx.fill();
}

function roundRect(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

// ---- The breath ------------------------------------------------------------

// The inhale's waveform (peaks.json, from audio/synth.py, so it's the sound
// you hear), drawn left to right up to `progress`.
function drawWave(ctx, progress, o) {
  const peaks = PEAKS.peaks;
  const count = Math.floor(peaks.length * clamp(progress, 0, 1));
  const step = (o.x1 - o.x0) / peaks.length;
  ctx.save();
  ctx.fillStyle = o.color;
  for (let i = 0; i < count; i++) {
    const a = peaks[i] * o.amp;
    ctx.fillRect(o.x0 + i * step, o.y - a, Math.max(1, step * 0.6), Math.max(1, a * 2));
  }
  if (progress > 0 && progress < 1) {
    ctx.fillStyle = C.red;
    ctx.fillRect(o.x0 + count * step, o.y - o.amp * 0.9, 3, o.amp * 1.8);
  }
  ctx.restore();
}

// ---- Grain -------------------------------------------------------------------

let grainTiles = [];
function setup() {
  const stage = document.getElementById("stage");
  stage.style.width = `${W}px`;
  stage.style.height = `${H}px`;
  for (const id of ["c", "grain"]) {
    const canvas = document.getElementById(id);
    canvas.width = W;
    canvas.height = H;
  }
  const random = rng(7);
  for (let k = 0; k < 6; k++) {
    const tile = document.createElement("canvas");
    tile.width = 384;
    tile.height = 384;
    const g = tile.getContext("2d");
    const img = g.createImageData(384, 384);
    for (let i = 0; i < img.data.length; i += 4) {
      const v = 255 - Math.floor(Math.pow(random(), 3) * 70);
      img.data[i] = v;
      img.data[i + 1] = v - 2;
      img.data[i + 2] = v - 6;
      img.data[i + 3] = 255;
    }
    g.putImageData(img, 0, 0);
    grainTiles.push(tile);
  }
}

function drawGrain(t, strength) {
  const canvas = document.getElementById("grain");
  const g = canvas.getContext("2d");
  g.globalAlpha = 1;
  g.fillStyle = "#fff";
  g.fillRect(0, 0, W, H);
  if (strength <= 0) return;
  const frame = Math.round(t * FPS);
  const tile = grainTiles[frame % grainTiles.length];
  const r = rng(frame + 1);
  g.globalAlpha = strength;
  g.save();
  g.translate(-Math.floor(r() * 384), -Math.floor(r() * 384));
  g.fillStyle = g.createPattern(tile, "repeat");
  g.fillRect(0, 0, W + 384, H + 384);
  g.restore();
  g.globalAlpha = 1;
}

// ---- Chips: numbers ----------------------------------------------------------

// A measured number with what it measures, and a bar against Nielsen's
// 0.1 s limit for a response that feels instant.
function numberChip(o) {
  const w = o.w || 520;
  const h = o.bar ? 262 : 196;
  box({ x: o.x, y: o.y, w, h, opacity: o.opacity });
  text(o.value, { x: o.x + 36, y: o.y + 30, size: o.size || 104, weight: 620, color: C.ink, opacity: o.opacity });
  text(o.label, { x: o.x + 38, y: o.y + 30 + (o.size || 104) + 12, size: 30, weight: 440, color: C.inkSoft,
    opacity: o.opacity });
  if (o.bar) {
    const bx = o.x + 38;
    const by = o.y + h - 52;
    const bw = w - 76;
    const track = box({ x: bx, y: by, w: bw, h: 6, radius: 3, fill: C.line, shadow: false, opacity: o.opacity });
    const fill = Math.max(4, bw * o.bar.ms / 100 * clamp(o.bar.grow ?? 1, 0, 1));
    box({ x: bx, y: by - 3, w: fill, h: 12, radius: 6, fill: C.red, shadow: false, opacity: o.opacity });
    box({ x: bx + bw - 2, y: by - 10, w: 2, h: 26, radius: 1, fill: C.ink, shadow: false, opacity: o.opacity });
    text("100 ms feels instant (Nielsen)", { x: bx + bw, y: by + 18, size: 24, weight: 440, color: C.inkSoft,
      align: "right", opacity: o.opacity });
    void track;
  }
}

// ---- Shots -------------------------------------------------------------------

// A footage shot: a take played on its nominal clock from n0, with camera
// keys in shot time. `follow` makes the camera track the caret.
function footage(t0, t1, take, n0, o = {}) {
  return {
    t0, t1, take, n0, kind: "footage", grain: o.grain ?? 0.05,
    async draw(ctx, t) {
      const lt = t - t0;
      const n = n0 + lt;
      let cam = o.cam ? camAt(o.cam, lt) : { cx: SRC_W / 2, cy: SRC_H / 2, z: W / SRC_W };
      if (o.follow) {
        const c = caretFollow(take, n, o.follow.lag ?? 0.35);
        if (c) {
          const f = o.follow;
          const mix = f.mix ? f.mix(lt) : 1;
          // Follows the caret's line; across the line only within bounds.
          const x = f.x ? clamp(c.x + (f.dx || 0), f.x[0], f.x[1]) : cam.cx;
          cam = { cx: lerp(cam.cx, x, mix), cy: lerp(cam.cy, c.y + (f.dy || 0), mix), z: cam.z };
        }
      }
      ctx.fillStyle = C.appBg;
      ctx.fillRect(0, 0, W, H);
      await drawFootage(ctx, take, n, cam, o.rect || { x: 0, y: 0, w: W, h: H });
      if (o.overlay) await o.overlay(ctx, lt, n, t);
    },
  };
}

function card(t0, t1, words, o = {}) {
  return {
    t0, t1, kind: "card", grain: 0.5,
    async draw(ctx, t) {
      const lt = t - t0;
      ctx.fillStyle = C.paper;
      ctx.fillRect(0, 0, W, H);
      const size = o.size || (HERO ? 132 : 118);
      const x = o.x ?? (HERO ? 200 : 100);
      const lines = o.lines || [words];
      const lead = size * 1.06;
      const y = o.y ?? (HERO ? H / 2 - size * 0.7 - (lines.length - 1) * lead / 2 : 700);
      const stagger = o.stagger ?? BEAT / 2;
      let shown = 0;
      let lastEl = null;
      let widest = 0;
      lines.forEach((lineWords, li) => {
        const { line } = kinetic(lineWords, { x, y: y + li * lead, size, weight: 620, stagger },
          lt - shown * stagger);
        shown += lineWords.length;
        lastEl = line;
        widest = Math.max(widest, line.getBoundingClientRect().width);
      });
      const lineBottom = y + lines.length * lead;
      waterline(ctx, { x, y: lineBottom + size * 0.36, width: widest * 0.98, thick: HERO ? 13 : 12,
        gap: HERO ? 36 : 34, count: 3 }, lt);
      // The caret after the last word, once it's up.
      const allIn = lt > (shown - 1) * stagger + 0.3;
      if (allIn && lastEl) {
        const r = lastEl.getBoundingClientRect();
        caretBar(ctx, r.right + size * 0.08, y + (lines.length - 1) * lead + size * 0.04, size * 0.9, lt, true);
      }
    },
  };
}

// ---- The breach --------------------------------------------------------------

// The icon's geometry (apps/desktop/assets/icon/compose_icon.py), in the
// 824-unit tile, used as the breach's world.
const ICON = {
  tile: 824,
  lines: [520, 600, 452, 580, 380],
  lineLeft: 96,
  lineTop: 460,
  lineGap: 58,
  lineH: 18,
  surface: 469,
  whaleW: 824 * 0.98,
  whaleCx: 0.47 * 824,
  whaleCy: 0.46 * 824,
  angle: 38,
};

function superellipse(ctx, cx, cy, half, n = 5) {
  ctx.beginPath();
  for (let i = 0; i <= 128; i++) {
    const th = (i / 128) * Math.PI * 2;
    const c = Math.cos(th);
    const s = Math.sin(th);
    const x = cx + half * Math.sign(c) * Math.pow(Math.abs(c), 2 / n);
    const y = cy + half * Math.sign(s) * Math.pow(Math.abs(s), 2 / n);
    if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
  }
  ctx.closePath();
}

// The whale at time u of the breach (0 under, 1 landed), in tile units.
function whalePose(u) {
  // Rises along its own axis, past the icon's pose, and settles back.
  const along = u < 0.72 ? lerp(-760, 60, easeOut(u / 0.72)) : lerp(60, 0, easeInOut((u - 0.72) / 0.28));
  const angle = u < 0.72 ? lerp(52, 36, easeOut(u / 0.72)) : lerp(36, ICON.angle, easeInOut((u - 0.72) / 0.28));
  const th = (-angle * Math.PI) / 180;
  return {
    cx: ICON.whaleCx + Math.cos(th) * along,
    cy: ICON.whaleCy + Math.sin(th) * along,
    angle,
  };
}

// Where the whale's axis crosses a row, and how wide the body is there.
function whaleCrossing(pose, y) {
  const th = (-pose.angle * Math.PI) / 180;
  const dirx = Math.cos(th);
  const diry = Math.sin(th);
  const k = (y - pose.cy) / diry;
  const half = ICON.whaleW / 2;
  if (Math.abs(k) > half * 0.98) return null;
  const x = pose.cx + dirx * k;
  const thickness = (ICON.whaleW * 698 / 2400) * (1 - Math.pow(k / half, 2) * 0.7) * 0.55;
  return { x, half: thickness / Math.abs(Math.sin(th)) / 2 + 18 };
}

function drawWhale(ctx, pose, submergedAlpha, surfaceY) {
  const w = ICON.whaleW;
  const h = (w * WHALE.height) / WHALE.width;
  const paint = (clip) => {
    ctx.save();
    clip();
    ctx.translate(pose.cx, pose.cy);
    ctx.rotate((-pose.angle * Math.PI) / 180);
    ctx.drawImage(WHALE, -w / 2, -h / 2, w, h);
    ctx.restore();
  };
  // Above the surface, full ink; below it, faint, as in the icon.
  paint(() => { ctx.beginPath(); ctx.rect(-2000, -2000, 5000, surfaceY + 2000); ctx.clip(); });
  ctx.save();
  ctx.globalAlpha = submergedAlpha;
  paint(() => { ctx.beginPath(); ctx.rect(-2000, surfaceY, 5000, 3000); ctx.clip(); });
  ctx.restore();
}

// Letters thrown up where the whale breaks the surface.
function spray(ctx, since, x, surfaceY, seed) {
  if (since < 0) return;
  const random = rng(seed);
  const letters = "Fasterthanyoucangasp";
  for (let i = 0; i < 46; i++) {
    const vx = (random() - 0.5) * 900 + 180;
    const vy = -(380 + random() * 700);
    const x0 = x + (random() - 0.5) * 140;
    const delay = random() * 0.12;
    const s = since - delay;
    if (s < 0) continue;
    const g = 1700;
    const px = x0 + vx * s;
    const py = surfaceY + vy * s + 0.5 * g * s * s;
    const life = clamp(1 - s / (0.9 + random() * 0.5), 0, 1);
    if (life <= 0 || py > surfaceY + 30) continue;
    ctx.save();
    ctx.globalAlpha = life;
    ctx.fillStyle = i % 11 === 0 ? C.red : C.ink;
    ctx.translate(px, py);
    ctx.rotate((random() - 0.5) * 6 * s);
    ctx.font = `${random() < 0.5 ? 700 : 400} ${Math.round(18 + random() * 26)}px Charter`;
    ctx.fillText(letters[Math.floor(random() * letters.length)], 0, 0);
    ctx.restore();
  }
}

// The whole breach, in tile units, at time lt since it began. Returns the
// time the whale lands.
const BREACH = { land: HERO ? 1.4 : 0.9 };

function drawBreachWorld(ctx, lt, o) {
  const u = clamp(lt / BREACH.land, 0, 1);
  const pose = whalePose(u);
  // Water: the text lines part around the whale while it's in them, and
  // close again as it settles.
  const parting = Math.sin(clamp(u / 0.85, 0, 1) * Math.PI) * (u < 0.85 ? 1 : 0);
  ICON.lines.forEach((len, i) => {
    const y = ICON.lineTop + i * ICON.lineGap;
    const cross = whaleCrossing(pose, y + ICON.lineH / 2);
    const x0 = ICON.lineLeft;
    const x1 = ICON.lineLeft + len;
    ctx.fillStyle = `rgba(11, 11, 13, ${34 / 255 * (o.lineBoost || 1)})`;
    const wobble = Math.sin(lt * 9 + i * 1.7) * 5 * parting;
    if (cross && parting > 0.02 && cross.x > x0 - 40 && cross.x < x1 + 40) {
      const gap = cross.half * parting * 1.35;
      const a = cross.x - gap;
      const bb = cross.x + gap;
      if (a > x0) { roundRect(ctx, x0, y - wobble, a - x0, ICON.lineH, ICON.lineH / 2); ctx.fill(); }
      if (bb < x1) { roundRect(ctx, bb, y + wobble, x1 - bb, ICON.lineH, ICON.lineH / 2); ctx.fill(); }
    } else {
      roundRect(ctx, x0, y, len, ICON.lineH, ICON.lineH / 2);
      ctx.fill();
    }
  });
  drawWhale(ctx, pose, 0.38, ICON.surface);
  // Foam along the surface where the body meets it.
  const cross = whaleCrossing(pose, ICON.surface);
  if (cross) {
    ctx.fillStyle = "rgba(248, 246, 241, 0.9)";
    ctx.fillRect(cross.x - cross.half * 0.8, ICON.surface, cross.half * 1.6, 6);
  }
  // The caret, the one red mark, blinking on the first line.
  caretBar(ctx, ICON.lineLeft + ICON.lines[0] + 22, ICON.lineTop - 12, ICON.lineH + 24, lt + 0.2, lt > BREACH.land);
  // Letters thrown up as the head breaks the surface.
  spray(ctx, lt - BREACH.land * 0.3, ICON.whaleCx + 60, ICON.surface - 6, 42);
}

// ---- Scenes --------------------------------------------------------------------

function coldOpen(t0, t1, o) {
  const take = "cold_start_10";
  const execReal = DATA.takes[take].events.find((e) => e.what === "exec").t;
  const visible = o.firstVisible; // seconds after exec when the window's content was on screen
  const tExec = o.windowAt - visible;
  return {
    t0, t1, kind: "cold", grain: 0.0,
    async draw(ctx, t) {
      const lt = t - t0;
      const since = t - tExec;
      const n = execReal + since; // unpaced: nominal is real time
      ctx.fillStyle = C.black;
      ctx.fillRect(0, 0, W, H);
      const shown = since >= visible;
      if (shown) {
        const push = easeInOut(unlerp(o.windowAt, t1, t));
        const cam = HERO
          ? { cx: lerp(1280, 1180, push), cy: lerp(720, 640, push), z: lerp(0.75, 0.9, push) }
          : { cx: lerp(1100, 1000, push), cy: lerp(560, 520, push), z: lerp(0.62, 0.78, push) };
        const rect = HERO ? { x: 0, y: 0, w: W, h: H } : { x: 0, y: 640, w: W, h: 900 };
        await drawFootage(ctx, take, n, cam, rect);
      }
      // The breath, drawing across the top.
      const waveY = HERO ? 64 : 150;
      drawWave(ctx, unlerp(o.inhale[0], o.inhale[0] + o.inhale[1], t),
        { x0: HERO ? 120 : 80, x1: HERO ? W - 120 : W - 80, y: waveY, amp: HERO ? 40 : 50,
          color: shown && HERO ? "rgba(27,26,23,0.55)" : "rgba(243,239,230,0.8)" });
      const ms = clamp(since, 0, visible) * 1000;
      const value = `${Math.round(ms)} ms`;
      if (!shown) {
        const cy = HERO ? H / 2 - 110 : 820;
        text(value, { x: W / 2, y: cy, size: HERO ? 190 : 170, weight: 600, color: C.paper, align: "center" });
        text("Launch, in real time, no cuts", { x: W / 2, y: cy + (HERO ? 220 : 200), size: HERO ? 36 : 40,
          weight: 420, color: "rgba(243,239,230,0.7)", align: "center" });
      } else {
        const chip = HERO ? { x: 70, y: H - 70 - 190, w: 560 } : { x: 70, y: 250, w: W - 140 };
        numberChip({ ...chip, value, label: "from launch to the first frame, real time, no cuts", size: HERO ? 104 : 116 });
      }
    },
  };
}

// The keystroke number over the typing shot.
function typingOverlay(o) {
  return async (ctx, lt) => {
    if (lt < o.from) return;
    const grow = easeOut(unlerp(o.from, o.from + 0.5, lt));
    const chip = HERO ? { x: 70, y: H - 70 - 250, w: 600 } : { x: 70, y: 230, w: W - 140 };
    numberChip({ ...chip, value: `${o.ms} ms`, label: "from keystroke to painted frame, median", size: HERO ? 104 : 116,
      bar: { ms: o.ms, grow } });
  };
}

function searchOverlay(o) {
  return async (ctx, lt, n) => {
    if (n < o.at) return;
    const grow = easeOut(unlerp(o.at, o.at + 0.4, n));
    const chip = HERO ? { x: W - 70 - 640, y: H - 70 - 250, w: 640 } : { x: 70, y: 230, w: W - 140 };
    numberChip({ ...chip, value: `${o.ms} ms`, label: `to search all ${o.notes} notes for “${o.query}”`,
      size: HERO ? 104 : 116, bar: { ms: o.ms, grow } });
  };
}

function labelChip(str, o) {
  const size = o.size || 44;
  const w = o.w;
  box({ x: o.x, y: o.y, w, h: size + 56 });
  text(str, { x: o.x + 30, y: o.y + 26, size, weight: 600 });
}

// The agent: a terminal pane with the MCP exchange, the note beside it.
function agentScene(t0, t1, n0, o = {}) {
  const take = "agent_2";
  const td = DATA.takes[take];
  const start = td.events.find((e) => e.what === "mcp-start");
  const toNominal = (tReal) => start.nominal + (tReal - start.t);
  const calls = [];
  for (const line of td.mcp || []) {
    const msg = JSON.parse(line.text);
    if (line.dir === "out" && msg.method === "tools/call") {
      calls.push({ n: toNominal(line.t), args: msg.params.arguments, name: msg.params.name });
    }
    if (line.dir === "in" && msg.result && msg.result.content && calls.length && msg.id === calls.length) {
      calls[calls.length - 1].reply = msg.result.content[0].text;
      calls[calls.length - 1].replyN = toNominal(line.t);
    }
  }
  return {
    t0, t1, kind: "agent", grain: 0.1,
    async draw(ctx, t) {
      const lt = t - t0;
      const n = n0 + lt;
      ctx.fillStyle = C.appBg;
      ctx.fillRect(0, 0, W, H);
      const paneW = HERO ? 660 : W;
      const paneH = HERO ? H : 700;
      const noteRect = HERO ? { x: paneW, y: 0, w: W - paneW, h: H } : { x: 0, y: 1100, w: W, h: 820 };
      const cam = HERO ? { cx: 1275, cy: 820, z: 0.84 } : { cx: 1275, cy: 1010, z: 1.0 };
      await drawFootage(ctx, take, n, cam, noteRect);
      // The pane.
      const py = HERO ? 0 : 400;
      ctx.fillStyle = C.ink;
      ctx.fillRect(0, py, paneW, paneH);
      const lines = [];
      lines.push({ s: "$ gasp mcp \"Field Notes\"", c: "rgba(243,239,230,0.55)" });
      for (const call of calls) {
        if (n < call.n - 0.25) continue;
        lines.push({ s: "", c: C.paper });
        lines.push({ s: `→ tools/call  ${call.name}`, c: C.paper, strong: true });
        for (const [k, v] of Object.entries(call.args)) {
          lines.push({ s: `    ${k}: ${JSON.stringify(v)}`, c: "rgba(243,239,230,0.78)" });
        }
        if (call.reply && n >= call.replyN) {
          lines.push({ s: `← ${call.reply}`, c: "#E8A3B2" });
        }
      }
      const size = HERO ? 24 : 26;
      const lh = size * 1.55;
      let y = py + (HERO ? 150 : 70);
      const x = HERO ? 64 : 56;
      const wrapAt = HERO ? 40 : 56;
      for (const line of lines) {
        const parts = wrap(line.s, wrapAt);
        for (const part of parts) {
          text(part, { x, y, size, weight: line.strong ? 600 : 400, color: line.c, cls: "mono" });
          y += lh;
        }
      }
      if (o.overlay) o.overlay(ctx, lt);
    },
  };
}

function wrap(s, width) {
  if (s.length <= width) return [s];
  const out = [];
  let rest = s;
  let first = true;
  while (rest.length > width) {
    let cut = rest.lastIndexOf(" ", width);
    if (cut < 20) cut = width;
    out.push(rest.slice(0, cut));
    rest = (first ? "      " : "      ") + rest.slice(cut).trimStart();
    first = false;
  }
  out.push(rest);
  return out;
}

// A compressed replay: a few frames of every feature, inside one inhale.
function replay(t0, t1, flashes, o) {
  return {
    t0, t1, kind: "replay", grain: 0.1,
    async draw(ctx, t) {
      const lt = t - t0;
      const per = (t1 - t0) / flashes.length;
      const k = clamp(Math.floor(lt / per), 0, flashes.length - 1);
      const f = flashes[k];
      const cam = { ...f.cam, z: f.cam.z * (1 + 0.05 * ((lt - k * per) / per)) };
      ctx.fillStyle = C.appBg;
      ctx.fillRect(0, 0, W, H);
      await drawFootage(ctx, f.take, f.n + (lt - k * per), cam, o.rect || { x: 0, y: 0, w: W, h: H });
      const bandH = HERO ? 128 : 300;
      ctx.fillStyle = C.paper;
      ctx.fillRect(0, 0, W, bandH);
      drawWave(ctx, unlerp(0, t1 - t0, lt), { x0: HERO ? 120 : 80, x1: HERO ? W - 120 : W - 80,
        y: bandH / 2, amp: HERO ? 44 : 60, color: C.ink });
    },
  };
}

function endCard(t0, t1, o = {}) {
  return {
    t0, t1, kind: "end", grain: 0.5,
    async draw(ctx, t) {
      const lt = t - t0;
      ctx.fillStyle = C.paper;
      ctx.fillRect(0, 0, W, H);
      const land = BREACH.land;
      const k = HERO ? 1 : 0.6; // the vertical cut has less time
      // From a large breach to the logo's place.
      const shrink = easeInOut(unlerp(land - 0.05 * k, land + 0.55 * k, lt));
      const big = HERO ? { s: 1.05, x: W / 2 - 824 * 1.05 / 2 + 60, y: H / 2 - 824 * 1.05 * 0.56 }
        : { s: 1.12, x: W / 2 - 824 * 1.12 / 2 + 40, y: 420 };
      const logoSize = HERO ? 250 : 300;
      const small = HERO ? { s: logoSize / 824, x: 590, y: 330 } : { s: logoSize / 824, x: W / 2 - logoSize / 2, y: 470 };
      const s = lerp(big.s, small.s, shrink);
      const ox = lerp(big.x, small.x, shrink);
      const oy = lerp(big.y, small.y, shrink);
      ctx.save();
      ctx.translate(ox, oy);
      ctx.scale(s, s);
      // The tile forms around the whale as it lands.
      const tile = easeOut(unlerp(land - 0.1 * k, land + 0.35 * k, lt));
      if (tile > 0) {
        ctx.save();
        ctx.globalAlpha = tile;
        ctx.shadowColor = "rgba(20, 18, 14, 0.22)";
        ctx.shadowBlur = 60;
        ctx.shadowOffsetY = 16;
        superellipse(ctx, 412, 412, 412 * lerp(1.25, 1, tile));
        ctx.fillStyle = "#F8F6F1";
        ctx.fill();
        ctx.restore();
      }
      ctx.save();
      superellipse(ctx, 412, 412, 412 * lerp(3, 1, tile));
      ctx.clip();
      drawBreachWorld(ctx, lt, {});
      ctx.restore();
      ctx.restore();
      // Name, platforms, address.
      const after = (lt - (land + (HERO ? 0.25 : 0.34))) / k;
      if (after > 0) {
        const nameX = HERO ? 890 : W / 2;
        const nameY = HERO ? 300 : 830;
        kinetic(["Gasp"], { x: nameX, y: nameY, size: HERO ? 210 : 190, weight: 640,
          align: HERO ? undefined : "center" }, after);
        const lineY = HERO ? 560 : 1080;
        const u = easeOut(clamp((after - 0.35) / 0.4, 0, 1));
        text("macOS, Windows, Linux and iPhone", { x: nameX + (HERO ? 6 : 0), y: lineY, size: HERO ? 44 : 46,
          weight: 460, color: C.ink, opacity: u, align: HERO ? undefined : "center" });
        text("gasp.app", { x: nameX + (HERO ? 6 : 0), y: lineY + (HERO ? 70 : 76), size: HERO ? 44 : 46, weight: 600,
          color: C.red, opacity: easeOut(clamp((after - 0.55) / 0.4, 0, 1)), align: HERO ? undefined : "center" });
        const f = easeOut(clamp((after - 0.8) / 0.5, 0, 1));
        const foot = o.footnote;
        text(foot, { x: HERO ? 120 : 80, y: HERO ? H - 118 : 1300, size: HERO ? 19 : 22, weight: 400,
          color: C.inkSoft, opacity: f, width: HERO ? 1300 : W - 160, lineHeight: 1.4 });
        text("Whale after a model by Gutarra Díaz, Stubbs, Moon, Palmer and Benton, CC BY 4.0.",
          { x: HERO ? 120 : 80, y: HERO ? H - 56 : 1420, size: HERO ? 16 : 19, weight: 400, color: C.inkFaint,
            opacity: f, width: HERO ? 1300 : W - 160 });
      }
    },
  };
}

// ---- The timelines ---------------------------------------------------------------

function heroTimeline() {
  const m = DATA.measure;
  const shots = [];
  const cues = { music: [], inhales: [], keys: [], ticks: [], whoosh: [], splash: [], hits: [] };
  const inhale = [0.1, 0.72];
  cues.inhales.push(inhale);

  // 0: black, the breath, and the launch in one real-time take.
  shots.push(coldOpen(0, b(6), { windowAt: b(2), firstVisible: m.cold_visible_s, inhale }));
  cues.music.push([b(2), b(6), "light"]);

  // 6: typing, with the keystroke number.
  const typing = footage(b(6), b(14), "typing_2", 0.35, {
    cam: [[0, { cx: 1275, cy: 1000, z: 1.0 }], [b(3.5), { cx: 1275, cy: 1000, z: 1.05 }],
      [b(3.5) + 0.001, { cx: 1275, cy: 1000, z: 1.55, cut: true }], [b(8), { cx: 1275, cy: 1000, z: 1.6 }]],
    follow: { dx: -460, dy: -50, lag: 0.5, x: [1130, 1450] },
    overlay: typingOverlay({ ms: m.keystroke_ms, from: 0.5 }),
  });
  shots.push(typing);
  cues.music.push([b(6), b(14), "full"]);

  // 14: math.
  shots.push(card(b(14), b(16), ["Math", "as", "you", "type."], { stagger: BEAT / 4 }));
  shots.push(footage(b(16), b(22), "math_2", 1.05, {
    cam: [[0, { cx: 1150, cy: 560, z: 1.5 }], [b(6), { cx: 1150, cy: 560, z: 1.62 }]],
    follow: { dx: -520, dy: -80, lag: 0.6, x: [1150, 1350] },
  }));
  cues.music.push([b(14), b(22), "full"]);

  // 22: the grid table, cut on the beat.
  const tcam = (z, cx = 980, cy = 640) => [[0, { cx, cy, z }], [b(4), { cx, cy, z: z + 0.06 }]];
  shots.push(footage(b(22), b(23.5), "table_2", 0.3, { cam: tcam(1.4) }));
  shots.push(footage(b(23.5), b(24.5), "table_2", 2.2, { cam: tcam(1.05, 1280, 560) }));
  shots.push(footage(b(24.5), b(26), "table_2", 2.62, { cam: tcam(1.4, 1000, 640) }));
  shots.push(footage(b(26), b(30), "table_2", 4.7, { cam: tcam(1.45, 960, 650) }));
  cues.music.push([b(22), b(30), "full"]);

  // 30: search across the vault.
  const search = DATA.takes.search_4;
  const lastChar = search.events.filter((e) => e.what === "char").pop();
  shots.push(footage(b(30), b(31), "search_4", 0.35, { cam: [[0, { cx: 1280, cy: 760, z: 0.75 }], [b(1), { cx: 1280, cy: 740, z: 0.8 }]] }));
  shots.push(footage(b(31), b(38), "search_4", 1.85, {
    cam: [[0, { cx: 1280, cy: 700, z: 1.0 }], [b(1.5), { cx: 1280, cy: 700, z: 1.0 }],
      [b(1.5) + 0.001, { cx: 1300, cy: 600, z: 1.3, cut: true }], [b(7), { cx: 1300, cy: 610, z: 1.36 }]],
    overlay: searchOverlay({ ms: m.search_ms, notes: m.notes, query: "echo", at: lastChar.nominal + 0.05 }),
  }));
  cues.music.push([b(30), b(38), "full"]);

  // 38: make it yours.
  shots.push(footage(b(38), b(40.5), "accent_1", 2.0, { cam: [[0, { cx: 1380, cy: 780, z: 1.25 }], [b(2.5), { cx: 1420, cy: 800, z: 1.32 }]] }));
  shots.push(footage(b(40.5), b(42.5), "keymap_1", 0.35, { cam: [[0, { cx: 1500, cy: 1150, z: 1.3 }], [b(2), { cx: 1520, cy: 1160, z: 1.38 }]] }));
  shots.push(footage(b(42.5), b(44), "keymap_1", 2.45, { cam: [[0, { cx: 1275, cy: 1000, z: 1.3 }], [b(1.5), { cx: 1275, cy: 1000, z: 1.36 }]] }));
  shots.push(footage(b(44), b(46.5), "snippet_1", 0.4, {
    cam: [[0, { cx: 1000, cy: 520, z: 1.8 }], [b(2.5), { cx: 1000, cy: 520, z: 1.86 }]],
    follow: { dx: -200, dy: -60, lag: 0.5, x: [800, 1300] },
  }));
  shots.push(card(b(46.5), b(50), ["Yours,", "down", "to", "every", "token."], {
    lines: [["Yours,", "down", "to"], ["every", "token."]] }));
  cues.music.push([b(38), b(50), "full"]);

  // 50: sync.
  shots.push(footage(b(50), b(53), "sync_2", 0.85, {
    cam: [[0, { cx: 1275, cy: 1000, z: 1.3 }], [b(3), { cx: 1275, cy: 1010, z: 1.36 }]],
    follow: { dx: -200, dy: -150, lag: 0.5, x: [1100, 1400] },
  }));
  shots.push(footage(b(53), b(57), "sync_2", 4.05, {
    cam: [[0, { cx: 2180, cy: 1390, z: 2.0 }], [b(4), { cx: 2180, cy: 1390, z: 2.1 }]],
    overlay: (ctx, lt) => { if (lt > 0.15) labelChip("Synced through git.", { x: 70, y: 70, w: 560, size: 56 }); },
  }));
  cues.music.push([b(50), b(57), "full"]);

  // 57: agents.
  shots.push(card(b(57), b(60), ["Your", "agents", "can", "write", "here", "too."], {
    lines: [["Your", "agents", "can"], ["write", "here", "too."]], stagger: BEAT / 4 }));
  shots.push(agentScene(b(60), b(69), 0.35));
  cues.music.push([b(57), b(69), "full"]);

  // 69: the breath again, every feature inside it, then the line.
  const flashes = [
    { take: "typing_2", n: 3.0, cam: { cx: 1100, cy: 960, z: 1.5 } },
    { take: "math_2", n: 3.3, cam: { cx: 900, cy: 560, z: 1.6 } },
    { take: "table_2", n: 5.3, cam: { cx: 1380, cy: 760, z: 1.45 } },
    { take: "search_4", n: 3.4, cam: { cx: 1180, cy: 640, z: 1.3 } },
    { take: "accent_1", n: 2.3, cam: { cx: 1400, cy: 800, z: 1.3 } },
    { take: "keymap_1", n: 1.3, cam: { cx: 1740, cy: 1090, z: 1.55 } },
    { take: "sync_2", n: 4.8, cam: { cx: 2150, cy: 1400, z: 1.6 } },
    { take: "agent_2", n: 3.2, cam: { cx: 1230, cy: 760, z: 1.1 } },
  ];
  const replayLen = 0.72;
  shots.push(replay(b(69), b(69) + replayLen, flashes, {}));
  cues.inhales.push([b(69) - 0.02, replayLen]);
  shots.push(footage(b(69) + replayLen, b(79), "tagline_2", 0.25, {
    rect: { x: 0, y: 310, w: W, h: 440 },
    cam: [[0, { cx: 1240, cy: 405, z: 1.25 }], [b(8.5), { cx: 1240, cy: 405, z: 1.3 }]],
    overlay: (ctx) => {
      ctx.fillStyle = C.paper;
      ctx.fillRect(0, 0, W, 310);
      ctx.fillRect(0, 750, W, H - 750);
    },
  }));
  cues.music.push([b(69) + replayLen, b(79), "pulse"]);

  // 79: the breach, and the logo.
  shots.push(endCard(b(79), b(87), { footnote: m.footnote }));
  cues.whoosh.push([b(79) + 0.1, 0.5]);
  cues.splash.push(b(79) + BREACH.land * 0.3);
  cues.hits.push(b(79) + BREACH.land);
  return finish(shots, cues, b(87));
}

function verticalTimeline() {
  const m = DATA.measure;
  const shots = [];
  const cues = { music: [], inhales: [], keys: [], ticks: [], whoosh: [], splash: [], hits: [] };
  const inhale = [0.08, 0.72];
  cues.inhales.push(inhale);
  // Type in the top third, the app in the centre band, the bottom kept
  // clear for the platforms' own buttons and captions.
  const band = { x: 0, y: 640, w: W, h: 820 };
  const paperAround = (ctx) => {
    ctx.fillStyle = C.paper;
    ctx.fillRect(0, 0, W, band.y);
    ctx.fillRect(0, band.y + band.h, W, H - band.y - band.h);
  };
  const title = (lines, stagger = BEAT / 3) => (ctx, lt) => {
    paperAround(ctx);
    const size = 100;
    let shown = 0;
    lines.forEach((lw, li) => {
      kinetic(lw, { x: 80, y: 230 + li * size * 1.06, size, weight: 620, stagger }, lt - shown * stagger);
      shown += lw.length;
    });
  };
  const lastChar = DATA.takes.search_4.events.filter((e) => e.what === "char").pop().nominal;

  shots.push(coldOpen(0, b(3), { windowAt: b(2), firstVisible: m.cold_visible_s, inhale }));
  cues.music.push([b(2), b(3), "light"]);
  shots.push(footage(b(3), b(8), "typing_2", 1.9, {
    rect: band, cam: [[0, { cx: 1300, cy: 1000, z: 1.25 }], [b(5), { cx: 1300, cy: 1000, z: 1.32 }]],
    follow: { dx: -200, dy: -40, lag: 0.5, x: [900, 1500] },
    overlay: async (ctx, lt) => {
      paperAround(ctx);
      await typingOverlay({ ms: m.keystroke_ms, from: 0.1 })(ctx, lt);
    },
  }));
  shots.push(footage(b(8), b(14), "math_2", 1.25, {
    rect: band, cam: [[0, { cx: 1150, cy: 560, z: 1.3 }], [b(6), { cx: 1150, cy: 560, z: 1.38 }]],
    follow: { dx: -120, dy: -90, lag: 0.6, x: [900, 1500] },
    overlay: title([["Math", "as"], ["you", "type."]]),
  }));
  shots.push(footage(b(14), b(19.5), "search_4", 1.9, {
    rect: band, cam: [[0, { cx: 1180, cy: 620, z: 1.2 }], [b(5.5), { cx: 1180, cy: 640, z: 1.26 }]],
    overlay: async (ctx, lt, n) => {
      paperAround(ctx);
      await searchOverlay({ ms: m.search_ms, notes: m.notes, query: "echo", at: lastChar + 0.05 })(ctx, lt, n);
    },
  }));
  shots.push(footage(b(19.5), b(24), "agent_2", 1.35, {
    rect: band, cam: [[0, { cx: 990, cy: 980, z: 1.05 }], [b(4.5), { cx: 1000, cy: 990, z: 1.08 }]],
    overlay: title([["Your", "agents", "can"], ["write", "here", "too."]], BEAT / 4),
  }));
  cues.music.push([b(3), b(24), "full"]);
  shots.push(footage(b(24), b(26.5), "tagline_2", 2.05, {
    rect: { x: 0, y: 720, w: W, h: 420 }, cam: [[0, { cx: 1250, cy: 470, z: 0.74 }], [b(2), { cx: 1250, cy: 470, z: 0.76 }]],
    overlay: (ctx) => {
      ctx.fillStyle = C.paper;
      ctx.fillRect(0, 0, W, 720);
      ctx.fillRect(0, 1140, W, H - 1140);
    },
  }));
  cues.music.push([b(24), b(26.5), "pulse"]);
  shots.push(endCard(b(26.5), 15.0, { footnote: m.footnote_short }));
  cues.whoosh.push([b(26.5) + 0.03, 0.4]);
  cues.splash.push(b(26.5) + BREACH.land * 0.3);
  cues.hits.push(b(26.5) + BREACH.land);
  return finish(shots, cues, 15.0);
}

function finish(shots, cues, duration) {
  // An accent on every cut between sections, where the music is playing.
  cues.cuts = shots.slice(1).filter((s, i) => s.kind !== shots[i].kind || s.take !== shots[i].take)
    .map((s) => s.t0)
    .filter((t) => cues.music.some(([a, z, mode]) => t >= a - 1e-6 && t < z && mode === "full"));
  for (const shot of shots) {
    if (!shot.take) continue;
    for (const e of eventsIn(shot.take, shot.n0, shot.n0 + (shot.t1 - shot.t0))) {
      const at = shot.t0 + (e.nominal - shot.n0);
      if (e.what === "char" || e.what === "key") cues.keys.push([at, e.what === "char" ? 1.0 : 0.85]);
      else cues.ticks.push(at);
    }
  }
  return { shots, cues: { ...cues, duration, bpm: 124 }, duration };
}

let TIMELINE = null;
function timeline() {
  if (!TIMELINE) TIMELINE = HERO ? heroTimeline() : verticalTimeline();
  return TIMELINE;
}

window.cues = async () => { await window.ready; return timeline().cues; };
window.duration = async () => { await window.ready; return timeline().duration; };
window.cuts = async () => { await window.ready; return timeline().shots.map((s) => [s.t0, s.t1, s.kind, s.take || ""]); };

window.renderFrame = async (t) => {
  await window.ready;
  const tl = timeline();
  const shot = tl.shots.find((s) => t >= s.t0 - 1e-9 && t < s.t1 - 1e-9) || tl.shots[tl.shots.length - 1];
  TYPE().innerHTML = "";
  const ctx = document.getElementById("c").getContext("2d");
  ctx.setTransform(1, 0, 0, 1, 0, 0);
  ctx.globalAlpha = 1;
  await shot.draw(ctx, t);
  drawGrain(t, shot.grain ?? 0);
  return true;
};
