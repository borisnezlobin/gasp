// Renders the reel's picture with headless Chromium, frame by frame.
//
//   node render.js <build-dir> <format> cues <out.json>
//   node render.js <build-dir> <format> stills <out-dir> <t> [<t> ...]
//   node render.js <build-dir> <format> video <out.mp4> [workers] [from] [to]
//
// <build-dir> holds stage.html, reel.js, data.json, peaks.json, the fonts
// and the whale; the unpacked takes are in <build-dir>/../frames. The video
// is lossless-ish H.264 (yuv444, CRF 8) for render.sh to finish.

const path = require("path");
const fs = require("fs");
const { spawn } = require("child_process");
const { chromium } = require(process.env.PLAYWRIGHT || "/opt/node22/lib/node_modules/playwright");

const [build, format, mode, out, ...rest] = process.argv.slice(2);
const size = format === "hero" ? { width: 1920, height: 1080 } : { width: 1080, height: 1920 };
const FPS = 60;

// A small static server for the work directory (the page fetches its data,
// and file:// pages can't).
let base = null;
function serve() {
  const http = require("http");
  const root = path.resolve(build, "..");
  const types = { ".html": "text/html", ".js": "text/javascript", ".json": "application/json", ".png": "image/png",
    ".ttf": "font/ttf" };
  const server = http.createServer((req, res) => {
    const file = path.join(root, decodeURIComponent(req.url.split("?")[0]));
    if (!file.startsWith(root)) { res.writeHead(403); res.end(); return; }
    fs.readFile(file, (error, body) => {
      if (error) { res.writeHead(404); res.end(); return; }
      res.writeHead(200, { "content-type": types[path.extname(file)] || "application/octet-stream" });
      res.end(body);
    });
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => {
    base = `http://127.0.0.1:${server.address().port}/${path.basename(path.resolve(build))}/`;
    resolve(server);
  }));
}

async function openStage(browser) {
  const page = await browser.newPage({ viewport: size, deviceScaleFactor: 1 });
  page.on("pageerror", (error) => console.error("page error:", error.message));
  page.on("console", (message) => { if (message.type() === "error") console.error("console:", message.text()); });
  await page.goto(base + "stage.html?format=" + format);
  await page.evaluate(() => window.ready);
  return page;
}

async function shoot(page, t) {
  await page.evaluate((time) => window.renderFrame(time), t);
  return page.screenshot({ type: "png", clip: { x: 0, y: 0, ...size }, animations: "disabled" });
}

async function main() {
  const server = await serve();
  const browser = await chromium.launch({ args: ["--allow-file-access-from-files", "--disable-web-security",
    "--font-render-hinting=none", "--force-color-profile=srgb"] });
  const page = await openStage(browser);
  if (mode === "cues") {
    fs.writeFileSync(out, JSON.stringify(await page.evaluate(() => window.cues()), null, 1));
    const cuts = await page.evaluate(() => window.cuts());
    fs.writeFileSync(out.replace(/\.json$/, ".cuts.json"), JSON.stringify(cuts, null, 1));
  } else if (mode === "stills") {
    fs.mkdirSync(out, { recursive: true });
    for (const t of rest.map(Number)) {
      fs.writeFileSync(path.join(out, `${format}-${t.toFixed(3)}.png`), await shoot(page, t));
    }
  } else if (mode === "video") {
    const workers = Number(rest[0] || 3);
    const duration = await page.evaluate(() => window.duration());
    const total = Math.round(duration * FPS);
    const from = Number(rest[1] || 0);
    const to = Math.min(total, Number(rest[2] || total));
    const per = Math.ceil((to - from) / workers);
    const parts = [];
    const jobs = [];
    for (let w = 0; w < workers; w++) {
      const a = from + w * per;
      const z = Math.min(to, a + per);
      if (a >= z) break;
      const part = `${out}.part${w}.mkv`;
      parts.push(part);
      jobs.push(renderRange(browser, w === 0 ? page : null, a, z, part));
    }
    await Promise.all(jobs);
    const list = `${out}.parts.txt`;
    fs.writeFileSync(list, parts.map((p) => `file '${path.resolve(p)}'`).join("\n"));
    await run("ffmpeg", ["-loglevel", "error", "-y", "-f", "concat", "-safe", "0", "-i", list, "-c", "copy", out]);
    for (const p of parts) fs.unlinkSync(p);
    fs.unlinkSync(list);
  }
  await browser.close();
  server.close();
}

async function renderRange(browser, page, a, z, part) {
  page = page || (await openStage(browser));
  const ffmpeg = spawn("ffmpeg", ["-loglevel", "error", "-y", "-f", "image2pipe", "-framerate", String(FPS),
    "-c:v", "png", "-i", "-", "-c:v", "libx264", "-preset", "fast", "-crf", "8", "-pix_fmt", "yuv444p",
    "-r", String(FPS), part], { stdio: ["pipe", "inherit", "inherit"] });
  const started = Date.now();
  for (let f = a; f < z; f++) {
    const png = await shoot(page, f / FPS);
    if (!ffmpeg.stdin.write(png)) await new Promise((resolve) => ffmpeg.stdin.once("drain", resolve));
    if ((f - a) % 120 === 0) {
      const rate = (f - a + 1) / ((Date.now() - started) / 1000);
      console.error(`${part}: frame ${f} of ${a}-${z}, ${rate.toFixed(1)} fps`);
    }
  }
  ffmpeg.stdin.end();
  await new Promise((resolve) => ffmpeg.on("close", resolve));
}

function run(cmd, args) {
  return new Promise((resolve, reject) => {
    const p = spawn(cmd, args, { stdio: "inherit" });
    p.on("close", (code) => (code === 0 ? resolve() : reject(new Error(`${cmd} exited ${code}`))));
  });
}

main().catch((error) => { console.error(error); process.exit(1); });
