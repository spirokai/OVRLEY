const MIB = 1024 * 1024
const GIB = 1024 * MIB

/**
 * Formats a cache size in binary units: whole megabytes below one gigabyte, otherwise gigabytes with one decimal.
 *
 * @param {number} bytes - Size in bytes.
 * @returns {string} Display size such as "512 MB", "1.0 GB" or "1.5 GB".
 */
export function formatCacheBytes(bytes) {
  if (bytes < GIB) return `${Math.round(bytes / MIB)} MB`
  return `${(bytes / GIB).toFixed(1)} GB`
}
