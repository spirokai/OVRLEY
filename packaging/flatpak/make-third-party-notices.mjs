// Writes the notices file installed with the Flatpak, describing the bundled FFmpeg build.
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'

const output = process.argv[2]
if (!output) throw new Error('Usage: node make-third-party-notices.mjs <output file>')

const ffmpeg = (...args) => execFileSync('vendor/ffmpeg/bin/ffmpeg', ['-hide_banner', ...args], { encoding: 'utf8' }).trim()

writeFileSync(
  output,
  [
    'THIRD-PARTY NOTICES',
    '',
    'FFmpeg',
    '------',
    'This Flatpak includes unmodified FFmpeg and FFprobe command-line binaries as separate',
    'components. OVRLEY runs them as subprocesses; they are not linked into the OVRLEY executable.',
    '',
    'Project: https://ffmpeg.org/',
    'Source code: https://ffmpeg.org/download.html',
    'License information: https://ffmpeg.org/legal.html',
    '',
    'The binaries are downloaded from BtbN FFmpeg builds:',
    'https://github.com/BtbN/FFmpeg-Builds',
    '',
    'ffmpeg -version',
    '---------------',
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
