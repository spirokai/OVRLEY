// Generates the animated previews of the transition picker with the bundled FFmpeg:
// one looping WebP sprite sheet holding a cell per `xfade` transition in catalogue
// order (`assets/transitions.json`, row by row), each blending two neutral generated
// frames (a blue grid fading into an orange box). Run after changing the catalogue:
//   node scripts/generate-transition-previews.mjs
import { readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const catalogue = JSON.parse(await readFile(join(rootDir, "assets", "transitions.json"), "utf8"));
const output = join(rootDir, "assets", "transition-previews.webp");
const ffmpeg = join(rootDir, "vendor", "ffmpeg", "bin", process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg");

const { columns, width, height } = catalogue.preview;
const transitions = catalogue.xfade;
const SIZE = `${width}x${height}`;
const FPS = 15;
// Each preview holds 0.6 s of A, a 1 s transition and 0.6 s of B, then loops.
const CLIP_SECONDS = 1.6;
const OFFSET_SECONDS = 0.6;
const DURATION_SECONDS = 1;
const FRAME_A = `color=c=0x2563eb:s=${SIZE}:r=${FPS}:d=${CLIP_SECONDS},drawgrid=w=15:h=17:t=1:c=white@0.45`;
const FRAME_B =
  `color=c=0xf97316:s=${SIZE}:r=${FPS}:d=${CLIP_SECONDS},` +
  "drawbox=x=30:y=14:w=60:h=40:t=fill:c=0xfff7ed@0.85,drawbox=x=45:y=24:w=30:h=20:t=fill:c=0xc2410c";

const labels = (prefix) => transitions.map((_, index) => `[${prefix}${index}]`).join("");
const cells = transitions.map((_, index) => `${(index % columns) * width}_${Math.floor(index / columns) * height}`);
const graph = [
  `${FRAME_A},split=${transitions.length}${labels("a")}`,
  `${FRAME_B},split=${transitions.length}${labels("b")}`,
  ...transitions.map(
    (transition, index) =>
      `[a${index}][b${index}]xfade=transition=${transition}:duration=${DURATION_SECONDS}:offset=${OFFSET_SECONDS}[x${index}]`,
  ),
  `${labels("x")}xstack=inputs=${transitions.length}:layout=${cells.join("|")}:fill=black`,
].join(";");

const result = spawnSync(
  ffmpeg,
  ["-hide_banner", "-loglevel", "error", "-y", "-filter_complex", graph, "-c:v", "libwebp_anim", "-quality", "60", "-loop", "0", output],
  { encoding: "utf8" },
);
if (result.status !== 0) {
  throw new Error(`FFmpeg could not render the transition previews: ${result.stderr || result.error}`);
}
console.log(`Wrote ${transitions.length} transition previews to ${output}`);
