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
