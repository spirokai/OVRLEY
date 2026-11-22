// What the vendor installers share (install-ffmpeg.mjs, install-mdk.mjs).
import { spawn } from 'node:child_process'
import { createWriteStream } from 'node:fs'
import { get } from 'node:https'

/** Downloads `url`, following redirects; progress is logged under `label`. */
export function download(url, destination, label) {
  return new Promise((resolvePromise, reject) => {
    const request = get(url, (response) => {
      if ([301, 302, 303, 307, 308].includes(response.statusCode ?? 0)) {
        const location = response.headers.location
        response.resume()
        if (!location) {
          reject(new Error(`Redirect from ${url} did not include a location`))
          return
        }
        download(new URL(location, url).toString(), destination, label).then(resolvePromise, reject)
        return
      }

      if (response.statusCode !== 200) {
        reject(new Error(`Failed to download ${url}: HTTP ${response.statusCode}`))
        return
      }

      const file = createWriteStream(destination)
      const totalBytes = Number(response.headers['content-length'] ?? 0)
      let downloadedBytes = 0
      let lastProgressAt = 0

      response.on('data', (chunk) => {
        downloadedBytes += chunk.length
        const now = Date.now()
        if (now - lastProgressAt < 500) {
          return
        }
        lastProgressAt = now
        writeDownloadProgress(label, downloadedBytes, totalBytes)
      })

      response.pipe(file)
      file.on('finish', () => {
        writeDownloadProgress(label, downloadedBytes, totalBytes)
        process.stdout.write('\n')
        file.close(resolvePromise)
      })
      file.on('error', reject)
    })
    request.on('error', reject)
  })
}

function writeDownloadProgress(label, downloadedBytes, totalBytes) {
  const downloadedMb = bytesToMiB(downloadedBytes)
  if (totalBytes > 0) {
    const totalMb = bytesToMiB(totalBytes)
    const percent = Math.min(100, (downloadedBytes / totalBytes) * 100)
    process.stdout.write(`\r[${label}] Downloaded ${downloadedMb} / ${totalMb} MiB (${percent.toFixed(1)}%)`)
    return
  }

  process.stdout.write(`\r[${label}] Downloaded ${downloadedMb} MiB`)
}

function bytesToMiB(bytes) {
  return (bytes / 1024 / 1024).toFixed(1)
}

/** Runs a command with inherited output; fails on a non-zero exit. */
export function run(command, args) {
  return new Promise((resolvePromise, reject) => {
    const child = spawn(command, args, { stdio: 'inherit' })
    child.on('error', reject)
    child.on('close', (code) => {
      if (code === 0) resolvePromise()
      else reject(new Error(`${command} exited with ${code}`))
    })
  })
}
