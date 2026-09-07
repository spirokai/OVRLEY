import { stat, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";

export const PACKAGING_DOCUMENTS = {
  notice: "THIRD_PARTY_NOTICES.txt",
  macosInstall: "INSTALL-macos.txt",
  linuxInstall: "INSTALL-linux.txt",
  debianInstall: "INSTALL-debian.txt",
};

export async function preparePackagingResources(rootDir = resolve(import.meta.dirname, "..")) {
  await ensureDirectory(join(rootDir, "fonts"), "Fonts source directory");
  await ensureDirectory(join(rootDir, "templates"), "Templates source directory");

  const ffmpegBinary = join(rootDir, "vendor", "ffmpeg", "bin", process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg");
  const ffprobeBinary = join(
    rootDir,
    "vendor",
    "ffmpeg",
    "bin",
    process.platform === "win32" ? "ffprobe.exe" : "ffprobe",
  );
  await ensureFile(ffmpegBinary, "FFmpeg binary");
  await ensureFile(ffprobeBinary, "FFprobe binary");

  const noticePath = join(rootDir, PACKAGING_DOCUMENTS.notice);
  await writeFile(noticePath, buildThirdPartyNotice(ffmpegBinary));
  await writeFile(join(rootDir, PACKAGING_DOCUMENTS.macosInstall), buildMacosInstallDocument());
  await writeFile(join(rootDir, PACKAGING_DOCUMENTS.linuxInstall), buildLinuxInstallDocument());
  await writeFile(join(rootDir, PACKAGING_DOCUMENTS.debianInstall), buildDebianInstallDocument());

  return {
    noticePath,
    macosInstallPath: join(rootDir, PACKAGING_DOCUMENTS.macosInstall),
    linuxInstallPath: join(rootDir, PACKAGING_DOCUMENTS.linuxInstall),
    debianInstallPath: join(rootDir, PACKAGING_DOCUMENTS.debianInstall),
  };
}

function buildThirdPartyNotice(ffmpegBinaryPath) {
  const versionText = runFfmpegCommand(ffmpegBinaryPath, ["-version"]);
  const licenseText = runFfmpegCommand(ffmpegBinaryPath, ["-L"]);

  return [
    "THIRD-PARTY NOTICES",
    "",
    "Gyroflow source extraction",
    "--------------------------",
    "Copyright 2021-2022 Adrian <adrian.eddy at gmail>. GPL-3.0-or-later.",
    "OVRLEY adapts PyrLK tracking and robust homography fitting from Gyroflow.",
    "Source: https://github.com/gyroflow/gyroflow/tree/d918ab3594e539f25a67a9f1d2b8e042798f61f7/src/core/synchronization",
    "Modifications and provenance: src-tauri/ovrley_core/src/synchronization/UPSTREAM.md in the OVRLEY source distribution.",
    "",
    "OpenCV Rust bindings 0.94.4",
    "--------------------------",
    "The MIT License (MIT)",
    "Copyright 2016-2018 — The OpenCV-Rust binding developpers",
    "",
    "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions:",
    "The above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software.",
    "THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.",
    "",
    "Native OpenCV is separately licensed under Apache-2.0. Its license and bundled",
    "third-party notices must accompany the native runtime in packaged distributions.",
    "Project: https://github.com/opencv/opencv/tree/4.11.0",
    "",
    "FFmpeg",
    "-------",
    "This OVRLEY distribution includes unmodified FFmpeg and FFprobe command-line binaries",
    "as separate components in the packaged resources.",
    "",
    "OVRLEY invokes ffmpeg as a subprocess for video encoding and ffprobe as a subprocess",
    "for video metadata extraction. FFmpeg and FFprobe are not linked into the OVRLEY executable.",
    "",
    "Project: https://ffmpeg.org/",
    "Source code: https://ffmpeg.org/download.html",
    "License information: https://ffmpeg.org/legal.html",
    "Upstream repository mirror: https://github.com/FFmpeg/FFmpeg",
    "",
    "Windows builds are downloaded from BtbN FFmpeg builds:",
    "https://github.com/BtbN/FFmpeg-Builds",
    "",
    "Linux builds are downloaded from BtbN FFmpeg builds:",
    "https://github.com/BtbN/FFmpeg-Builds",
    "",
    "macOS builds are downloaded from Evermeet/Tessus FFmpeg builds:",
    "https://ffmpeg.martin-riedl.de/",
    "",
    "ffmpeg -version",
    "---------------",
    versionText,
    "",
    "ffmpeg -L",
    "---------",
    licenseText,
    "",
  ].join("\n");
}

function buildMacosInstallDocument() {
  return [
    "OVRLEY FOR macOS",
    "",
    "Install",
    "-------",
    "1. Open the downloaded OVRLEY DMG.",
    "2. Move OVRLEY.app to your /Applications folder.",
    "",
    "Unsigned App Notice",
    "-------------------",
    "OVRLEY is ad-hoc signed and is not notarized with an Apple Developer certificate,",
    "so macOS gatekeeper may block it from opening by default. Use the following command to remove the quarantine attribute and allow OVRLEY to run:",
    "sudo xattr -cr /Applications/OVRLEY.app",
    "",
  ].join("\n");
}

function buildLinuxInstallDocument() {
  return [
    "OVRLEY FOR LINUX",
    "",
    "Install",
    "-------",
    "1. Download the OVRLEY AppImage.",
    "2. Mark it executable with: 'chmod +x OVRLEY-*.AppImage' or right click and select 'Properties' -> 'Allow executing file'.",
    "3. Run the AppImage.",
    "",
    "Likely Runtime Dependencies (Debian/Ubuntu)",
    "---------------------------------------------",
    "The AppImage includes OVRLEY, FFmpeg, and FFprobe, but uses the host system's FUSE, WebKitGTK, GTK, application indicator, SVG, SSL, and GStreamer libraries. You may need to install these packages if they are not already present on your system:",
    "",
    "Ubuntu 24.04 or Debian 13:",
    "sudo apt install libfuse2t64 libwebkit2gtk-4.1-0 libgtk-3-0t64 gstreamer1.0-libav gstreamer1.0-plugins-good",
    "",
    "Ubuntu 22.04 or Debian 12:",
    "sudo apt install libfuse2 libwebkit2gtk-4.1-0 libgtk-3-0 gstreamer1.0-libav gstreamer1.0-plugins-good",
    "",
    "Notes",
    "-----",
    "The AppImage includes OVRLEY and self-contained FFmpeg and FFprobe command-line tools.",
    "",
  ].join("\n");
}

function buildDebianInstallDocument() {
  return [
    "OVRLEY FOR DEBIAN-BASED LINUX",
    "",
    "Install",
    "-------",
    "1. Extract the downloaded OVRLEY Debian ZIP archive.",
    "2. Install the package with: sudo apt install ./OVRLEY-*.deb",
    "3. Launch OVRLEY from the application menu.",
    "",
    "Dependencies",
    "------------",
    "The Debian package declares these runtime dependencies; apt installs them automatically:",
    "- gstreamer1.0-libav",
    "- gstreamer1.0-plugins-good",
    "- libgtk-3-0",
    "- libwebkit2gtk-4.1-0",
    "",
    "Notes",
    "-----",
    "The Debian package includes OVRLEY and self-contained FFmpeg and FFprobe command-line tools.",
    "",
  ].join("\n");
}

function runFfmpegCommand(binaryPath, args) {
  const result = spawnSync(binaryPath, args, {
    encoding: "utf8",
  });

  if (result.error || result.status !== 0) {
    throw new Error(
      `Could not read ${binaryPath} ${args.join(" ")} output: ${result.stderr?.trim() || result.error?.message || `exit ${result.status}`}`,
    );
  }

  return result.stdout.trim();
}

async function ensureFile(path, label) {
  try {
    const entry = await stat(path);
    if (entry.isFile() && entry.size > 0) {
      return;
    }
  } catch {
    // Fall through to the shared error below.
  }
  throw new Error(`${label} not found or empty at ${path}`);
}

async function ensureDirectory(path, label) {
  try {
    const entry = await stat(path);
    if (entry.isDirectory()) {
      return;
    }
  } catch {
    // Fall through to the shared error below.
  }
  throw new Error(`${label} not found at ${path}`);
}
