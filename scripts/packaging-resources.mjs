import { stat, writeFile } from 'node:fs/promises'
import { join, resolve } from 'node:path'
import { spawnSync } from 'node:child_process'

export const PACKAGING_DOCUMENTS = {
  notice: 'THIRD_PARTY_NOTICES.txt',
  macosInstall: 'INSTALL-macos.txt',
  linuxInstall: 'INSTALL-linux.txt',
  debianInstall: 'INSTALL-debian.txt',
}

export async function preparePackagingResources(rootDir = resolve(import.meta.dirname, '..')) {
  await ensureDirectory(join(rootDir, 'fonts'), 'Fonts source directory')
  await ensureDirectory(join(rootDir, 'templates'), 'Templates source directory')

  const ffmpegBinary = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffmpeg.exe' : 'ffmpeg')
  const ffprobeBinary = join(rootDir, 'vendor', 'ffmpeg', 'bin', process.platform === 'win32' ? 'ffprobe.exe' : 'ffprobe')
  await ensureFile(ffmpegBinary, 'FFmpeg binary')
  await ensureFile(ffprobeBinary, 'FFprobe binary')

  const noticePath = join(rootDir, PACKAGING_DOCUMENTS.notice)
  await writeFile(noticePath, buildThirdPartyNotice(ffmpegBinary))
  await writeFile(join(rootDir, PACKAGING_DOCUMENTS.macosInstall), buildMacosInstallDocument())
  await writeFile(join(rootDir, PACKAGING_DOCUMENTS.linuxInstall), buildLinuxInstallDocument())
  await writeFile(join(rootDir, PACKAGING_DOCUMENTS.debianInstall), buildDebianInstallDocument())

  return {
    noticePath,
    macosInstallPath: join(rootDir, PACKAGING_DOCUMENTS.macosInstall),
    linuxInstallPath: join(rootDir, PACKAGING_DOCUMENTS.linuxInstall),
    debianInstallPath: join(rootDir, PACKAGING_DOCUMENTS.debianInstall),
  }
}

function buildThirdPartyNotice(ffmpegBinaryPath) {
  const versionText = runFfmpegCommand(ffmpegBinaryPath, ['-version'])
  const licenseText = runFfmpegCommand(ffmpegBinaryPath, ['-L'])

  return [
    'THIRD-PARTY NOTICES',
    '',
    'FFmpeg',
    '-------',
    'This OVRLEY distribution includes unmodified FFmpeg and FFprobe command-line binaries',
    'as separate components in the packaged resources.',
    '',
    'OVRLEY invokes ffmpeg as a subprocess for video encoding and ffprobe as a subprocess',
    'for video metadata extraction. FFmpeg and FFprobe are not linked into the OVRLEY executable.',
    '',
    'Project: https://ffmpeg.org/',
    'Source code: https://ffmpeg.org/download.html',
    'License information: https://ffmpeg.org/legal.html',
    'Upstream repository mirror: https://github.com/FFmpeg/FFmpeg',
    '',
    'Windows builds are downloaded from BtbN FFmpeg builds:',
    'https://github.com/BtbN/FFmpeg-Builds',
    '',
    'Linux builds are downloaded from BtbN FFmpeg builds:',
    'https://github.com/BtbN/FFmpeg-Builds',
    '',
    'macOS builds are downloaded from Evermeet/Tessus FFmpeg builds:',
    'https://ffmpeg.martin-riedl.de/',
    '',
    'ffmpeg -version',
    '---------------',
    versionText,
    '',
    'ffmpeg -L',
    '---------',
    licenseText,
    '',
  ].join('\n')
}

function buildMacosInstallDocument() {
  return [
    'OVRLEY FOR macOS',
    '',
    'Install',
    '-------',
    '1. Open the downloaded OVRLEY DMG.',
    '2. Move OVRLEY.app to your /Applications folder.',
    '',
    'Unsigned App Notice',
    '-------------------',
    'OVRLEY is ad-hoc signed and is not notarized with an Apple Developer certificate,',
    'so macOS gatekeeper may block it from opening by default. Use the following command to remove the quarantine attribute and allow OVRLEY to run:',
    'sudo xattr -cr /Applications/OVRLEY.app',
    '',
  ].join('\n')
}

function buildLinuxInstallDocument() {
  return [
    'OVRLEY FOR LINUX',
    '',
    'Install',
    '-------',
    '1. Download the OVRLEY AppImage.',
    "2. Mark it executable with: 'chmod +x OVRLEY-*.AppImage' or right click and select 'Properties' -> 'Allow executing file'.",
    '3. Run the AppImage.',
    '',
    'What is inside, and what comes from your system',
    '-----------------------------------------------',
    'The AppImage contains OVRLEY, FFmpeg, FFprobe, fonts, and templates.',
    'It deliberately does NOT contain WebKitGTK, GTK, GLib, GStreamer, Mesa/EGL/GL, or GPU video drivers. Those must match your distribution, kernel, and GPU, and bundled copies are a common cause of crashes and blank windows on newer distributions.',
    'Package names below are the usual ones and can differ between releases. If a package is not found, search your package manager for the name in quotes.',
    '',
    '1. Required system packages',
    '---------------------------',
    'Debian 12 / Ubuntu 22.04 / Linux Mint 21:',
    '  sudo apt install libfuse2 libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1 librsvg2-2 libssl3 libegl1 libgl1 libgl1-mesa-dri',
    '',
    'Debian 13 / Ubuntu 24.04 and newer / Linux Mint 22:',
    '  sudo apt install libfuse2t64 libwebkit2gtk-4.1-0 libgtk-3-0t64 libayatana-appindicator3-1 librsvg2-2 libssl3t64 libegl1 libgl1 libgl1-mesa-dri',
    '',
    'Fedora (Workstation, KDE):',
    '  sudo dnf install fuse-libs webkit2gtk4.1 gtk3 libayatana-appindicator-gtk3 librsvg2 openssl-libs mesa-libEGL mesa-libGL mesa-dri-drivers',
    '',
    'Bazzite and other Fedora Atomic images:',
    "  Most of these are normally preinstalled. If something is missing, layering packages needs 'rpm-ostree install <package>' and a reboot, or run OVRLEY inside a Distrobox container.",
    '',
    'Arch Linux / CachyOS / Manjaro:',
    '  sudo pacman -S fuse2 webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg openssl mesa libglvnd',
    '',
    'OVRLEY needs the WebKitGTK 4.1 series (not 4.0 and not 6.0).',
    '',
    '2. Video preview (GStreamer)',
    '----------------------------',
    'The preview plays your video through WebKitGTK, which uses the system GStreamer. Without the codec plugins the preview can stutter, show frames only once a second, or fail to play.',
    '',
    'Debian / Ubuntu / Linux Mint:',
    '  sudo apt install gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-libav',
    '',
    'Fedora:',
    '  sudo dnf install gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free',
    "  Fedora does not ship H.264/HEVC decoders. Enable RPM Fusion and install 'gstreamer1-libav', or install 'gstreamer1-plugin-openh264' (H.264 only).",
    '',
    'Bazzite:',
    '  Media codecs are normally included.',
    '',
    'Arch Linux / CachyOS / Manjaro:',
    '  sudo pacman -S gst-plugins-base gst-plugins-good gst-plugins-bad gst-libav',
    '',
    '3. Hardware-accelerated export',
    '------------------------------',
    'OVRLEY checks your system at startup. Export options your system cannot use are shown as unavailable. CPU export always works and needs nothing extra.',
    'On Linux the hardware options are NVIDIA (NVENC) and VAAPI (Intel and AMD GPUs). Intel Quick Sync (QSV) and AMD AMF are not offered on Linux; Intel and AMD GPUs use VAAPI.',
    '',
    'Intel GPUs (Broadwell, 2014, and newer) -> VAAPI:',
    '  Debian / Ubuntu / Mint: sudo apt install intel-media-va-driver   (or intel-media-va-driver-non-free for extra encode features)',
    '  Fedora:                 intel-media-driver, from RPM Fusion',
    '  Arch / CachyOS:         sudo pacman -S intel-media-driver',
    '  Older Intel GPUs:       i965-va-driver (Debian/Ubuntu) or libva-intel-driver (Arch)',
    '',
    'AMD GPUs -> VAAPI:',
    '  Debian / Ubuntu / Mint: sudo apt install mesa-va-drivers   (on the newest Ubuntu releases it is part of mesa-libgallium, normally preinstalled)',
    "  Fedora:                 install 'mesa-va-drivers-freeworld' from RPM Fusion. Fedora's own Mesa build cannot encode H.264/HEVC.",
    '  Arch / CachyOS:         sudo pacman -S libva-mesa-driver',
    '  Bazzite:                normally preinstalled.',
    '',
    'NVIDIA GPUs -> NVENC (not VAAPI):',
    "  Install the proprietary NVIDIA driver, not the open 'nouveau' driver, including its encode library (libnvidia-encode).",
    "  Debian / Ubuntu / Mint: use the 'nvidia-driver' package for your release (Ubuntu: 'ubuntu-drivers install')",
    '  Fedora:                 akmod-nvidia and xorg-x11-drv-nvidia-cuda-libs, from RPM Fusion',
    "  Arch / CachyOS:         sudo pacman -S nvidia-utils plus 'nvidia' or 'nvidia-open'",
    '  Check with: nvidia-smi',
    '',
    'Check VAAPI (Intel and AMD):',
    "  Install 'vainfo' (Debian/Ubuntu) or 'libva-utils' (Fedora, Arch), then run: vainfo",
    "  A working setup lists H264 or HEVC entries with 'VAEntrypointEncSlice' or 'VAEntrypointEncSliceLP'.",
    '',
    'Device access:',
    '  VAAPI needs access to /dev/dri/renderD128. A normal desktop login already has it. On minimal setups, add yourself to the render group: sudo usermod -aG render,video $USER, then log out and in.',
    '',
    '4. Troubleshooting',
    '------------------',
    '- The AppImage will not start and mentions FUSE: install the FUSE package above, or run: ./OVRLEY-*.AppImage --appimage-extract-and-run',
    '- Blank or black window, or an EGL error in the terminal: try WEBKIT_DISABLE_DMABUF_RENDERER=1 ./OVRLEY-*.AppImage, and if that is not enough add WEBKIT_DISABLE_COMPOSITING_MODE=1. These are common WebKitGTK workarounds for some GPU and Wayland combinations.',
    '- Preview video stutters or plays upside down at first: install the GStreamer plugins in section 2, especially gst-plugins-bad.',
    '- A hardware export option is greyed out: install the matching driver in section 3 and restart OVRLEY.',
    '- A hardware export fails partway: choose CPU, or try the other hardware option, and report the GPU and driver version.',
    '',
    'Notes',
    '-----',
    'The AppImage includes OVRLEY and self-contained FFmpeg and FFprobe command-line tools.',
    '',
  ].join('\n')
}

function buildDebianInstallDocument() {
  return [
    'OVRLEY FOR DEBIAN-BASED LINUX',
    '',
    'Install',
    '-------',
    '1. Extract the downloaded OVRLEY Debian ZIP archive.',
    '2. Install the package with: sudo apt install ./OVRLEY-*.deb',
    '3. Launch OVRLEY from the application menu.',
    '',
    'Dependencies',
    '------------',
    'The Debian package declares these runtime dependencies; apt installs them automatically:',
    '- gstreamer1.0-libav',
    '- gstreamer1.0-plugins-bad',
    '- gstreamer1.0-plugins-good',
    '- libgtk-3-0',
    '- libwebkit2gtk-4.1-0',
    '',
    'The package also recommends GPU video drivers for hardware-accelerated export (VAAPI); apt installs them unless you pass --no-install-recommends:',
    '- intel-media-va-driver (or intel-media-va-driver-non-free) for Intel GPUs',
    '- mesa-va-drivers (or mesa-libgallium on newer releases) for AMD GPUs',
    'NVIDIA GPUs use the proprietary NVIDIA driver instead.',
    '',
    'Notes',
    '-----',
    'The Debian package includes OVRLEY and self-contained FFmpeg and FFprobe command-line tools.',
    '',
  ].join('\n')
}

function runFfmpegCommand(binaryPath, args) {
  const result = spawnSync(binaryPath, args, {
    encoding: 'utf8',
  })

  if (result.error || result.status !== 0) {
    throw new Error(
      `Could not read ${binaryPath} ${args.join(' ')} output: ${result.stderr?.trim() || result.error?.message || `exit ${result.status}`}`,
    )
  }

  return result.stdout.trim()
}

async function ensureFile(path, label) {
  try {
    const entry = await stat(path)
    if (entry.isFile() && entry.size > 0) {
      return
    }
  } catch {
    // Fall through to the shared error below.
  }
  throw new Error(`${label} not found or empty at ${path}`)
}

async function ensureDirectory(path, label) {
  try {
    const entry = await stat(path)
    if (entry.isDirectory()) {
      return
    }
  } catch {
    // Fall through to the shared error below.
  }
  throw new Error(`${label} not found at ${path}`)
}
