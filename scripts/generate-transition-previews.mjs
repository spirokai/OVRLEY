// Generates the animated previews of the transition picker with the bundled FFmpeg:
// one small looping WebP per `xfade` transition, from two neutral generated frames
// (matching teal and orange boxes). Run after changing the catalogue:
//   node scripts/generate-transition-previews.mjs
import { mkdir, readdir, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { XFADE_TRANSITIONS } from "../app/src/lib/video-program/transition-catalog.js";

const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const outputDir = join(rootDir, "assets", "transition-previews");
const ffmpeg = join(rootDir, "vendor", "ffmpeg", "bin", process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg");

const SIZE = "120x68";
const FPS = 15;
// Each preview holds 0.6 s of A, a 1 s transition and 0.6 s of B, then loops.
const CLIP_SECONDS = 1.6;
const OFFSET_SECONDS = 0.6;
const DURATION_SECONDS = 1;
function createFrame(backgroundColor, detailColor) {
  return (
    `color=c=${backgroundColor}:s=${SIZE}:r=${FPS}:d=${CLIP_SECONDS},format=rgb24,` +
    "drawbox=x=30:y=14:w=60:h=40:t=fill:c=0xfff7ed@0.85," +
    `drawbox=x=45:y=24:w=30:h=20:t=fill:c=${detailColor}`
  );
}
const FRAME_A = createFrame("0x005b5b", "0x003d3d");
const FRAME_B = createFrame("0xf97316", "0xc2410c");

await mkdir(outputDir, { recursive: true });
for (const file of await readdir(outputDir)) {
  if (file.endsWith(".webp")) await rm(join(outputDir, file));
}
for (const transition of XFADE_TRANSITIONS) {
  const result = spawnSync(
    ffmpeg,
    [
      "-hide_banner",
      "-loglevel",
      "error",
      "-y",
      "-f",
      "lavfi",
      "-i",
      FRAME_A,
      "-f",
      "lavfi",
      "-i",
      FRAME_B,
      "-filter_complex",
      `[0][1]xfade=transition=${transition}:duration=${DURATION_SECONDS}:offset=${OFFSET_SECONDS}`,
      "-c:v",
      "libwebp_anim",
      "-pix_fmt",
      "bgra",
      "-lossless",
      "1",
      // In lossless mode, quality controls compression effort rather than fidelity.
      "-quality",
      "80",
      "-loop",
      "0",
      join(outputDir, `${transition}.webp`),
    ],
    { encoding: "utf8" },
  );
  if (result.status !== 0) {
    throw new Error(`FFmpeg could not render the ${transition} preview: ${result.stderr || result.error}`);
  }
}
console.log(`Wrote ${XFADE_TRANSITIONS.length} transition previews to ${outputDir}`);
