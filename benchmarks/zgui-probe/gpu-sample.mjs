// GPU time of a probe, read from outside: runs a probe command and reads the GPU time of the
// probe's process and of the compositor at four points: the start and end of the probe's idle span
// and of its frames span.
//
// node benchmarks/zgui-probe/gpu-sample.mjs <probe command…>
//
// - macOS (Apple Silicon): ioreg's per-client Metal `accumulatedGPUTime` (ns), summed per pid; the
//   compositor is WindowServer.
// - Windows: the `\GPU Engine(pid_<pid>_*engtype_3D)\Running Time` counters (100 ns), summed per
//   pid, through PowerShell's Get-Counter; the compositor is dwm.exe. The counter names are the
//   English ones, so a Windows in another display language needs them translated.
//
// PROBE_IDLE defaults to 6 here, so that the compositor has a baseline to subtract, and
// PROBE_SECONDS to 30 as in the probes. Frames per second come from the probe's own lines (zgui,
// gpu_probe's raw and slint-video modes) and are otherwise the fps it was asked for.
// SAMPLE_DEBUG=1 prints each reading and when it was taken.
import { execFile, execFileSync, spawn } from "node:child_process";
import { promisify } from "node:util";

const command = process.argv.slice(2);
if (command.length === 0) {
  console.error("usage: node gpu-sample.mjs <probe command…>");
  process.exit(2);
}
const env = { WGPU_VALIDATION: "0", WGPU_DEBUG: "0", PROBE_IDLE: "6", PROBE_SECONDS: "30", ...process.env };
const idleSeconds = Number(env.PROBE_IDLE);
const totalSeconds = Number(env.PROBE_SECONDS);
if (idleSeconds < 5 || totalSeconds - idleSeconds < 12) {
  console.error("needs PROBE_IDLE ≥ 5 and PROBE_SECONDS − PROBE_IDLE ≥ 12");
  process.exit(2);
}

/** One reading: `{ t: ms, probe: ms of GPU time, compositor: ms of GPU time }`. */
const read = process.platform === "darwin" ? readMac : process.platform === "win32" ? readWindows : null;
if (read === null) {
  console.error(`no GPU sampler for ${process.platform}`);
  process.exit(2);
}

async function readMac(pid) {
  const text = execFileSync("ioreg", ["-r", "-c", "IOAccelerator", "-l", "-w0"], {
    encoding: "utf8",
    maxBuffer: 64 << 20,
  });
  const ns = new Map();
  let compositor = null;
  for (const block of text.split(/\+-o /)) {
    const creator = block.match(/"IOUserClientCreator" = "pid (\d+), ([^"]*)"/);
    if (!creator) continue;
    const owner = Number(creator[1]);
    if (creator[2] === "WindowServer") compositor = owner;
    let sum = 0;
    for (const match of block.matchAll(/"accumulatedGPUTime"=(\d+)/g)) sum += Number(match[1]);
    ns.set(owner, (ns.get(owner) ?? 0) + sum);
  }
  return { t: performance.now(), probe: (ns.get(pid) ?? 0) / 1e6, compositor: (ns.get(compositor) ?? 0) / 1e6 };
}

const execFileAsync = promisify(execFile);

async function readWindows(pid) {
  // The time is taken inside PowerShell, around the counters, because starting it takes long; the
  // reading counts as taken halfway through Get-Counter.
  const script = `
$ErrorActionPreference = 'SilentlyContinue'
$dwm = (Get-Process dwm | Select-Object -First 1).Id
$paths = @("\\GPU Engine(pid_${pid}_*engtype_3D)\\Running Time", "\\GPU Engine(pid_$($dwm)_*engtype_3D)\\Running Time")
$before = [DateTime]::UtcNow.Ticks
$samples = (Get-Counter -Counter $paths).CounterSamples
$after = [DateTime]::UtcNow.Ticks
"t=$([long](($before + $after) / 2))"
"dwm=$dwm"
foreach ($s in $samples) { "$($s.InstanceName)=$($s.RawValue)" }
`;
  const { stdout } = await execFileAsync("powershell", ["-NoProfile", "-NonInteractive", "-Command", script]);
  let t = null;
  let dwm = null;
  const units = new Map();
  for (const line of stdout.split(/\r?\n/)) {
    const [key, value] = line.split("=");
    if (key === "t") t = Number(value) / 1e4;
    else if (key === "dwm") dwm = Number(value);
    else {
      const owner = key.match(/^pid_(\d+)_/);
      if (owner) units.set(Number(owner[1]), (units.get(Number(owner[1])) ?? 0) + Number(value));
    }
  }
  if (t === null) throw new Error(`Get-Counter gave no reading:\n${stdout}`);
  // Running Time is in 100 ns units.
  return { t, probe: (units.get(pid) ?? 0) / 1e4, compositor: (units.get(dwm) ?? 0) / 1e4 };
}

let pid = null;
let fps = null;
let framesStart = null;
let measuredFps = null;
const perSecond = [];
const readings = {};

const child = spawn(command[0], command.slice(1), { env, stdio: ["ignore", "pipe", "inherit"] });
let pending = "";
child.stdout.on("data", (chunk) => {
  pending += chunk;
  const lines = pending.split(/\r?\n/);
  pending = lines.pop();
  for (const line of lines) {
    console.log(line);
    const head = line.match(/^probe: pid (\d+) mode \S+ fps (\d+)/);
    if (head) {
      [pid, fps] = [Number(head[1]), Number(head[2])];
      schedule(performance.now());
    }
    if (line.startsWith("probe: frames start")) framesStart = performance.now();
    const second = line.match(/^probe: (\d+) frames/);
    if (second && framesStart !== null) perSecond.push({ t: performance.now(), frames: Number(second[1]) });
    const run = line.match(/^probe: run \d+ frames over [\d.]+ s \(([\d.]+) fps\)/);
    if (run) measuredFps = Number(run[1]);
  }
});

/** The four readings, timed from the probe's start: the idle span's last 4 s, then the frames
 * span less 3 s at its start and 3 s before the probe quits. */
function schedule(start) {
  const at = (name, seconds) =>
    setTimeout(async () => {
      readings[name] = await read(pid);
      if (process.env.SAMPLE_DEBUG) console.error(`reading ${name} at ${((performance.now() - start) / 1000).toFixed(1)} s: ${JSON.stringify(readings[name])}`);
    }, Math.max(0, start + seconds * 1000 - performance.now()));
  at("idleFrom", idleSeconds - 5);
  at("idleTo", idleSeconds - 2);
  at("activeFrom", idleSeconds + 3);
  at("activeTo", totalSeconds - 3);
}

child.on("exit", (code) => {
  const missing = ["idleFrom", "idleTo", "activeFrom", "activeTo"].filter((name) => !readings[name]);
  if (code !== 0 || pid === null || missing.length > 0) {
    console.error(`probe exited with ${code}; readings missing: ${missing.join(", ") || "none"}`);
    process.exit(1);
  }
  if (measuredFps === null && framesStart !== null) {
    const inside = perSecond.filter((s) => s.t >= framesStart + 3000);
    if (inside.length > 0) measuredFps = inside.reduce((sum, s) => sum + s.frames, 0) / inside.length;
  }
  const idle = span(readings.idleFrom, readings.idleTo);
  const active = span(readings.activeFrom, readings.activeTo);
  // Per frame at the rate the probe was driven at; at fps 0 per second only.
  const rate = measuredFps || fps || 1;
  const compositorDelta = active.compositor - idle.compositor;
  if (active.probe > 1000 || active.compositor > 1000) {
    console.error("warning: more than 1000 ms of GPU time per second; the counters are not what this script assumes");
  }
  console.log(
    `gpu: ${JSON.stringify({
      command: command.join(" "),
      platform: process.platform,
      fps: Number(rate.toFixed(1)),
      fpsMeasured: Boolean(measuredFps),
      probeGpuMsPerFrame: round(active.probe / rate),
      probeGpuMsPerSecond: round(active.probe),
      compositorGpuMsPerSecond: { idle: round(idle.compositor), active: round(active.compositor) },
      compositorDeltaMsPerFrame: round(compositorDelta / rate),
      totalGpuMsPerFrame: round((active.probe + compositorDelta) / rate),
    })}`,
  );
});

/** GPU ms per second of the probe and of the compositor between two readings. */
function span(from, to) {
  const seconds = (to.t - from.t) / 1000;
  return { probe: (to.probe - from.probe) / seconds, compositor: (to.compositor - from.compositor) / seconds };
}

function round(value) {
  return Number(value.toFixed(3));
}
