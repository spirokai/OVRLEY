// Installs MDK's SDK (video playback in the native app) into vendor/mdk-sdk, as install-ffmpeg.mjs
// does FFmpeg. An SDK already there is kept, and so is one that MDK_SDK points at.
import { mkdir, rename, rm, stat } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { download, run } from './install-support.mjs'

// The nightly: the v0.40.1 release crashes with Windows' D3D12 decoders. SourceForge keeps one
// nightly per platform, so the startup log names the version.
const NIGHTLY_URL = 'https://sourceforge.net/projects/mdk-sdk/files/nightly'
const sdks = {
  darwin: { archive: 'mdk-sdk-macOS.tar.xz', library: join('lib', 'mdk.framework') },
  linux: { archive: 'mdk-sdk-linux.tar.xz', library: join('lib', process.arch === 'arm64' ? 'arm64' : 'amd64', 'libmdk.so') },
  win32: { archive: 'mdk-sdk-windows-clang.7z', library: join('lib', 'x64', 'mdk.lib') },
}
const vendorDir = resolve(dirname(fileURLToPath(import.meta.url)), '..', 'vendor')
const installDir = join(vendorDir, 'mdk-sdk')

main().catch((error) => {
  console.error(`[mdk] ${error.message}`)
  process.exit(1)
})

async function main() {
  if (process.env.OVRLEY_SKIP_MDK_INSTALL === '1') {
    console.log('[mdk] Skipping install because OVRLEY_SKIP_MDK_INSTALL=1')
    return
  }
  if (process.env.MDK_SDK) {
    console.log(`[mdk] Skipping install because MDK_SDK points at ${process.env.MDK_SDK}`)
    return
  }
  const sdk = sdks[process.platform]
  if (!sdk) {
    console.log(`[mdk] No SDK for ${process.platform}; the native app builds on macOS, Windows and Linux.`)
    return
  }
  if (await isSdk(installDir, sdk)) {
    console.log(`[mdk] Using ${installDir}`)
    return
  }

  // Beside its destination, so that the rename keeps the framework's symlinks.
  const workDir = join(vendorDir, `.mdk-sdk-install-${process.pid}`)
  try {
    await mkdir(workDir, { recursive: true })
    const url = process.env.OVRLEY_MDK_ARCHIVE_URL ?? `${NIGHTLY_URL}/${sdk.archive}/download`
    const archivePath = join(workDir, sdk.archive)
    console.log(`[mdk] Downloading ${url}`)
    await download(url, archivePath, 'mdk')
    await extract(archivePath, workDir)
    const extracted = join(workDir, 'mdk-sdk')
    if (!(await isSdk(extracted, sdk))) {
      throw new Error(`Downloaded archive did not contain mdk-sdk/include and mdk-sdk/${sdk.library}`)
    }
    await rm(installDir, { recursive: true, force: true })
    await rename(extracted, installDir)
  } finally {
    await rm(workDir, { recursive: true, force: true })
  }
  console.log(`[mdk] Installed ${installDir}`)
}

async function isSdk(dir, sdk) {
  try {
    await stat(join(dir, 'include', 'mdk', 'Player.h'))
    await stat(join(dir, sdk.library))
    return true
  } catch (error) {
    if (error.code === 'ENOENT') return false
    throw error
  }
}

async function extract(archivePath, destination) {
  if (process.platform !== 'win32') return run('tar', ['-xf', archivePath, '-C', destination])
  // Windows' own tar is libarchive, which reads 7z where it was built with LZMA; Windows 10's was
  // not. Git's tar does not. 7-Zip's installer does not put it on PATH.
  try {
    await run(join(process.env.SystemRoot, 'System32', 'tar.exe'), ['-xf', archivePath, '-C', destination])
    return
  } catch {}
  const errors = []
  for (const sevenZip of ['7z', join(process.env.ProgramFiles, '7-Zip', '7z.exe')]) {
    try {
      await run(sevenZip, ['x', '-y', `-o${destination}`, archivePath])
      return
    } catch (error) {
      errors.push(error.message)
    }
  }
  throw new Error(
    `Could not extract the .7z with Windows' tar or with 7-Zip (${errors.join('; ')}). Install 7-Zip, or extract it to ${installDir}.`,
  )
}
