// Writes the notices file installed with the Flatpak. FFmpeg is not bundled here; the
// runtime provides it, so describe that build instead of the downloaded release binaries.
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'

const output = process.argv[2]
if (!output) throw new Error('Usage: node make-third-party-notices.mjs <output file>')

const ffmpeg = (...args) => execFileSync('ffmpeg', ['-hide_banner', ...args], { encoding: 'utf8' }).trim()

writeFileSync(
  output,
  [
    'THIRD-PARTY NOTICES',
    '',
    'FFmpeg',
    '------',
    'This Flatpak does not bundle FFmpeg. OVRLEY runs the unmodified ffmpeg and ffprobe',
    'command-line tools as provided by the Flatpak runtime as separate processes.',
    '',
    'Project: https://ffmpeg.org/',
    'Source code: https://ffmpeg.org/download.html',
    'License information: https://ffmpeg.org/legal.html',
    '',
    'ffmpeg -version (runtime build)',
    '-------------------------------',
    ffmpeg('-version'),
    '',
    'ffmpeg -L',
    '---------',
    ffmpeg('-L'),
    '',
    '',
    readFileSync('vendor/skia-wasm/THIRD_PARTY_NOTICES.txt', 'utf8'),
  ].join('\n'),
)
