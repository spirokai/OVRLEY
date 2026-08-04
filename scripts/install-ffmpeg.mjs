import { createWriteStream } from "node:fs";
import { chmod, cp, mkdir, rm, stat } from "node:fs/promises";
import { get } from "node:https";
import { tmpdir } from "node:os";
import { basename, dirname, join, resolve } from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

// Pin to the last monthly BtbN autobuild.
// Daily autobuilds are periodically removed and may return HTTP 404.
const PINNED_BTBN_AUTOBUILD = "autobuild-2026-07-31-14-10";
const PINNED_BTBN_FFMPEG_BUILD = "N-125875-g5d4d3bdc61";
const BTBN_BASE =
  `https://github.com/BtbN/FFmpeg-Builds/releases/download/${PINNED_BTBN_AUTOBUILD}`;
const PINNED_WINDOWS_FFMPEG_ARCHIVE =
  `${BTBN_BASE}/ffmpeg-${PINNED_BTBN_FFMPEG_BUILD}-win64-gpl-shared.zip`;
const PINNED_LINUX_FFMPEG_ARCHIVE =
  `${BTBN_BASE}/ffmpeg-${PINNED_BTBN_FFMPEG_BUILD}-linux64-gpl-shared.tar.xz`;

const PINNED_DARWIN_FFMPEG_VERSION = "8.1.2";
const rootDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const installDir = join(rootDir, "vendor", "ffmpeg");
const binDir = join(installDir, "bin");
const binaryName = process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg";
const probeBinaryName = process.platform === "win32" ? "ffprobe.exe" : "ffprobe";
const binaryPath = join(binDir, binaryName);
const probeBinaryPath = join(binDir, probeBinaryName);
const requiredEncoders =
  process.platform === "darwin"
    ? ["prores_ks", "qtrle", "prores_videotoolbox"]
    : process.platform === "win32" || process.platform === "linux"
      ? ["prores_ks", "qtrle", "h264_qsv", "hevc_qsv"]
      : ["prores_ks", "qtrle"];
const requiredFilters =
  process.platform === "win32" || process.platform === "linux"
    ? ["format", "hwupload", "overlay_qsv", "hwdownload"]
    : ["format", "hwupload"];

const defaultFfmpegArchives = {
  "win32-x64": PINNED_WINDOWS_FFMPEG_ARCHIVE,
  "linux-x64": PINNED_LINUX_FFMPEG_ARCHIVE,
  "darwin-arm64": "https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffmpeg.zip",
  "darwin-x64": "https://ffmpeg.martin-riedl.de/download/macos/amd64/1783018342_8.1.2/ffmpeg.zip",
};

const expectedFfmpegBuild =
  (process.platform === "win32" || process.platform === "linux") && !process.env.OVRLEY_FFMPEG_ARCHIVE_URL
    ? PINNED_BTBN_FFMPEG_BUILD
    : null;
const expectedFfmpegVersion =
  process.platform === "darwin" && !process.env.OVRLEY_FFMPEG_ARCHIVE_URL ? PINNED_DARWIN_FFMPEG_VERSION : null;

const defaultFfprobeArchives = {
  "darwin-arm64": "https://ffmpeg.martin-riedl.de/download/macos/arm64/1783011502_8.1.2/ffprobe.zip",
  "darwin-x64": "https://ffmpeg.martin-riedl.de/download/macos/amd64/1783018342_8.1.2/ffprobe.zip",
};

main().catch((error) => {
  console.error(`[ffmpeg] ${error.message}`);
  process.exit(1);
});

async function main() {
  if (process.env.OVRLEY_SKIP_FFMPEG_INSTALL === "1") {
    console.log("[ffmpeg] Skipping install because OVRLEY_SKIP_FFMPEG_INSTALL=1");
    return;
  }

  const existingStatus = await checkFfmpeg(binaryPath);
  if (existingStatus.usable) {
    console.log(`[ffmpeg] ${existingStatus.message}`);
    console.log(`[ffmpeg] Using ${binaryPath}`);
    verifyInstalledTools(binaryPath, probeBinaryPath);
    return;
  }
  console.log(`[ffmpeg] ${existingStatus.message}`);

  const platformKey = `${process.platform}-${process.arch}`;
  const archiveUrl = process.env.OVRLEY_FFMPEG_ARCHIVE_URL ?? defaultFfmpegArchives[platformKey];
  if (!archiveUrl) {
    console.log(`[ffmpeg] No bundled installer for ${platformKey}; install ffmpeg on PATH or set OVRLEY_FFMPEG.`);
    return;
  }

  const workDir = join(tmpdir(), `ovrley-ffmpeg-${process.pid}`);
  const archivePath = join(workDir, basename(new URL(archiveUrl).pathname));
  const extractDir = join(workDir, "extract");

  await rm(workDir, { recursive: true, force: true });
  await mkdir(extractDir, { recursive: true });
  await mkdir(binDir, { recursive: true });

  console.log(`[ffmpeg] Downloading ${archiveUrl}`);
  await download(archiveUrl, archivePath);
  await extractArchive(archivePath, extractDir);

  const ffprobeArchiveUrl = process.env.OVRLEY_FFPROBE_ARCHIVE_URL ?? defaultFfprobeArchives[platformKey];
  if (ffprobeArchiveUrl) {
    const ffprobeArchivePath = join(workDir, basename(new URL(ffprobeArchiveUrl).pathname));
    const ffprobeExtractDir = join(workDir, "extract-ffprobe");
    await mkdir(ffprobeExtractDir, { recursive: true });
    console.log(`[ffmpeg] Downloading ${ffprobeArchiveUrl}`);
    await download(ffprobeArchiveUrl, ffprobeArchivePath);
    await extractArchive(ffprobeArchivePath, ffprobeExtractDir);
  }

  const discoveredBinary = await findFile(extractDir, binaryName);
  if (!discoveredBinary) {
    throw new Error(`Downloaded archive did not contain ${binaryName}`);
  }

  const discoveredProbeBinary = await findFile(workDir, probeBinaryName);
  if (!discoveredProbeBinary) {
    throw new Error(`Downloaded archive did not contain ${probeBinaryName}`);
  }

  await rm(installDir, { recursive: true, force: true });
  await mkdir(binDir, { recursive: true });
  const discoveredBinDir = dirname(discoveredBinary);
  await copyExtractedFfmpegDir(discoveredBinDir, binDir);

  if (dirname(discoveredProbeBinary) !== discoveredBinDir) {
    await cp(discoveredProbeBinary, probeBinaryPath);
  }

  const discoveredLibDir = resolve(discoveredBinDir, "..", "lib");
  let copiedLibDir = false;
  try {
    await stat(discoveredLibDir);
    await copyExtractedFfmpegDir(discoveredLibDir, join(installDir, "lib"));
    copiedLibDir = true;
  } catch {
    /* no lib/ directory, that's fine */
  }
  if (process.platform === "linux" && !copiedLibDir) {
    throw new Error(`Downloaded Linux archive did not contain required shared libraries at ${discoveredLibDir}`);
  }

  if (process.platform !== "win32") {
    await chmod(binaryPath, 0o755);
    await chmod(probeBinaryPath, 0o755);
  }

  const installedStatus = await checkFfmpeg(binaryPath);
  if (!installedStatus.usable) {
    throw new Error(`Installed ffmpeg is not usable: ${installedStatus.message}`);
  }
  console.log(`[ffmpeg] ${installedStatus.message}`);
  verifyInstalledTools(binaryPath, probeBinaryPath);

  await rm(workDir, { recursive: true, force: true });
  console.log(`[ffmpeg] Installed ${binaryPath}`);
  console.log(`[ffmpeg] Installed ${probeBinaryPath}`);
  console.log("[ffmpeg] postinstall script finished");
}

function execFfmpeg(path, args, options) {
  const env = { ...process.env };
  if (process.platform === "linux") {
    env.LD_LIBRARY_PATH = `${join(dirname(path), "..", "lib")}:${env.LD_LIBRARY_PATH ?? ""}`;
  }
  return spawnSync(path, args, { ...options, env });
}

async function copyExtractedFfmpegDir(sourceDir, destinationDir) {
  await cp(sourceDir, destinationDir, {
    recursive: true,
  });
}

async function checkFfmpeg(path) {
  try {
    await stat(path);
  } catch {
    return {
      usable: false,
      message: `No bundled ffmpeg found at ${path}; downloading full build.`,
    };
  }

  const result = execFfmpeg(path, ["-version"], { encoding: "utf8" });
  if (result.status !== 0) {
    return {
      usable: false,
      message: `Bundled ffmpeg exists at ${path}, but failed to run; downloading full build.`,
    };
  }

  const version = parseVersion(result.stdout);

  const build = parseBuildIdentifier(result.stdout);
  if (expectedFfmpegBuild && (!build || !build.startsWith(expectedFfmpegBuild))) {
    return {
      usable: false,
      message: `Bundled ffmpeg build ${build ?? "unknown"} does not match pinned build ${expectedFfmpegBuild}; downloading the pinned build.`,
    };
  }
  if (expectedFfmpegVersion && version !== expectedFfmpegVersion) {
    return {
      usable: false,
      message: `Bundled ffmpeg version ${version} does not match pinned version ${expectedFfmpegVersion}; downloading the pinned build.`,
    };
  }

  const featureStatus = hasRequiredFfmpegFeatures(path);
  if (!featureStatus.usable) {
    return {
      usable: false,
      message: `Bundled ffmpeg ${version} is missing required features (${featureStatus.missing.join(", ")}); downloading full build.`,
    };
  }

  const probeStatus = checkFfprobe(join(dirname(path), probeBinaryName), expectedFfmpegBuild, expectedFfmpegVersion);
  if (!probeStatus.usable) {
    return probeStatus;
  }

  return {
    usable: true,
    message: `Bundled ffmpeg ${version} is current and has required features; ffprobe is available.`,
  };
}

function checkFfprobe(path, expectedBuild = null, expectedVersion = null) {
  const result = execFfmpeg(path, ["-version"], { encoding: "utf8" });
  if (result.status !== 0) {
    return {
      usable: false,
      message: `Bundled ffprobe is missing or failed to run at ${path}; downloading full build.`,
    };
  }

  const build = parseBuildIdentifier(result.stdout);
  if (expectedBuild && (!build || !build.startsWith(expectedBuild))) {
    return {
      usable: false,
      message: `Bundled ffprobe build ${build ?? "unknown"} does not match pinned build ${expectedBuild}; downloading the pinned build.`,
    };
  }
  const version = parseVersion(result.stdout);
  if (expectedVersion && version !== expectedVersion) {
    return {
      usable: false,
      message: `Bundled ffprobe version ${version ?? "unknown"} does not match pinned version ${expectedVersion}; downloading the pinned build.`,
    };
  }

  return {
    usable: true,
    message: `Bundled ffprobe is available at ${path}.`,
  };
}

function verifyInstalledTools(ffmpegPath, ffprobePath) {
  const ffmpegVersion = runVersionCheck("ffmpeg", ffmpegPath);
  const ffprobeVersion = runVersionCheck("ffprobe", ffprobePath);
  console.log(`[ffmpeg] Verified ffmpeg: ${ffmpegVersion}`);
  console.log(`[ffmpeg] Verified ffmpeg path: ${ffmpegPath}`);
  console.log(`[ffmpeg] Verified ffprobe: ${ffprobeVersion}`);
  console.log(`[ffmpeg] Verified ffprobe path: ${ffprobePath}`);
}

function runVersionCheck(label, path) {
  const result = execFfmpeg(path, ["-version"], { encoding: "utf8" });
  if (result.status !== 0) {
    const stderr = result.stderr?.trim();
    throw new Error(`${label} verification failed at ${path}${stderr ? `: ${stderr}` : ""}`);
  }

  return firstNonEmptyLine(result.stdout) ?? `${label} -version completed successfully`;
}

function firstNonEmptyLine(output) {
  return output
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean);
}

function parseVersion(output) {
  const build = parseBuildIdentifier(output);
  const match = build?.match(/^(?:n-?)?(\d+(?:\.\d+){0,2})(?:-|$)/i);
  return match?.[1] ?? null;
}

function parseBuildIdentifier(output) {
  const line = firstNonEmptyLine(output);
  const match = line?.match(/^(?:ffmpeg|ffprobe) version\s+(\S+)/i);
  return match?.[1] ?? null;
}

function hasRequiredFfmpegFeatures(path) {
  const encoders = execFfmpeg(path, ["-hide_banner", "-encoders"], { encoding: "utf8" });
  if (encoders.status !== 0) {
    return {
      usable: false,
      missing: ["encoder-list"],
    };
  }

  const filters = execFfmpeg(path, ["-hide_banner", "-filters"], { encoding: "utf8" });
  if (filters.status !== 0) {
    return {
      usable: false,
      missing: ["filter-list"],
    };
  }

  const missingEncoders = requiredEncoders.filter((encoder) => !hasListedFeature(encoders.stdout, encoder));
  const missingFilters = requiredFilters.filter((filter) => !hasListedFeature(filters.stdout, filter));
  if (missingEncoders.length > 0 || missingFilters.length > 0) {
    return {
      usable: false,
      missing: [
        ...missingEncoders.map((encoder) => `encoder:${encoder}`),
        ...missingFilters.map((filter) => `filter:${filter}`),
      ],
    };
  }

  return {
    usable: true,
    missing: [],
  };
}

function hasListedFeature(output, feature) {
  return new RegExp(`(^|\\s)${escapeRegExp(feature)}(\\s|$)`, "m").test(output);
}

function escapeRegExp(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

function download(url, destination) {
  return new Promise((resolvePromise, reject) => {
    const request = get(url, (response) => {
      if ([301, 302, 303, 307, 308].includes(response.statusCode ?? 0)) {
        const location = response.headers.location;
        response.resume();
        if (!location) {
          reject(new Error(`Redirect from ${url} did not include a location`));
          return;
        }
        download(new URL(location, url).toString(), destination).then(resolvePromise, reject);
        return;
      }

      if (response.statusCode !== 200) {
        reject(new Error(`Failed to download ${url}: HTTP ${response.statusCode}`));
        return;
      }

      const file = createWriteStream(destination);
      const totalBytes = Number(response.headers["content-length"] ?? 0);
      let downloadedBytes = 0;
      let lastProgressAt = 0;

      response.on("data", (chunk) => {
        downloadedBytes += chunk.length;
        const now = Date.now();
        if (now - lastProgressAt < 500) {
          return;
        }
        lastProgressAt = now;
        writeDownloadProgress(downloadedBytes, totalBytes);
      });

      response.pipe(file);
      file.on("finish", () => {
        writeDownloadProgress(downloadedBytes, totalBytes);
        process.stdout.write("\n");
        file.close(resolvePromise);
      });
      file.on("error", reject);
    });
    request.on("error", reject);
  });
}

function writeDownloadProgress(downloadedBytes, totalBytes) {
  const downloadedMb = bytesToMiB(downloadedBytes);
  if (totalBytes > 0) {
    const totalMb = bytesToMiB(totalBytes);
    const percent = Math.min(100, (downloadedBytes / totalBytes) * 100);
    process.stdout.write(`\r[ffmpeg] Downloaded ${downloadedMb} / ${totalMb} MiB (${percent.toFixed(1)}%)`);
    return;
  }

  process.stdout.write(`\r[ffmpeg] Downloaded ${downloadedMb} MiB`);
}

function bytesToMiB(bytes) {
  return (bytes / 1024 / 1024).toFixed(1);
}

function extractArchive(archivePath, destination) {
  if (process.platform === "win32") {
    return run("powershell", [
      "-NoProfile",
      "-Command",
      `Expand-Archive -LiteralPath '${archivePath.replaceAll("'", "''")}' -DestinationPath '${destination.replaceAll("'", "''")}' -Force`,
    ]);
  }
  if (archivePath.endsWith(".tar.xz")) {
    return run("tar", ["-xf", archivePath, "-C", destination]);
  }
  return run("unzip", ["-q", archivePath, "-d", destination]);
}

function run(command, args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { stdio: "inherit" });
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) resolvePromise();
      else reject(new Error(`${command} exited with ${code}`));
    });
  });
}

async function findFile(dir, targetName) {
  const { readdir } = await import("node:fs/promises");
  const entries = await readdir(dir, { withFileTypes: true });
  for (const entry of entries) {
    const path = join(dir, entry.name);
    if (entry.isFile() && entry.name === targetName) {
      return path;
    }
    if (entry.isDirectory()) {
      const found = await findFile(path, targetName);
      if (found) return found;
    }
  }
  return null;
}
