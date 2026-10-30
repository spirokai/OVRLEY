// Throwaway: reference frames from MapLibre Native, for comparing style fidelity.
// Usage: reference.mjs <out.png> <style> <lat> <lon> <zoom> <pitch> <bearing> <size> <strip_3d>
import mbgl from '@maplibre/maplibre-gl-native'
import { writeFileSync } from 'node:fs'
import { deflateSync } from 'node:zlib'

const [, , out, styleName, lat, lon, zoom, pitch, bearing, size, strip] = process.argv
const ua = { 'User-Agent': 'OVRLEY-spike' }
const request = async (req, cb) => {
  try {
    const r = await fetch(req.url, { headers: ua })
    if (r.status === 204 || r.status === 404) return cb(null, {})
    if (!r.ok) return cb(new Error(String(r.status)))
    cb(null, { data: Buffer.from(await r.arrayBuffer()) })
  } catch (e) {
    cb(e)
  }
}
const table = Array.from({ length: 256 }, (_, n) => {
  let c = n
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1
  return c >>> 0
})
const crc = (b) => {
  let c = ~0
  for (const x of b) c = table[(c ^ x) & 255] ^ (c >>> 8)
  return ~c >>> 0
}
const chunk = (t, d) => {
  const l = Buffer.alloc(4)
  l.writeUInt32BE(d.length)
  const b = Buffer.concat([Buffer.from(t), d])
  const r = Buffer.alloc(4)
  r.writeUInt32BE(crc(b))
  return Buffer.concat([l, b, r])
}
const png = (w, h, a) => {
  const hd = Buffer.alloc(13)
  hd.writeUInt32BE(w, 0)
  hd.writeUInt32BE(h, 4)
  hd[8] = 8
  hd[9] = 6
  const raw = Buffer.alloc((w * 4 + 1) * h)
  for (let y = 0; y < h; y++) a.copy(raw, y * (w * 4 + 1) + 1, y * w * 4, (y + 1) * w * 4)
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk('IHDR', hd),
    chunk('IDAT', deflateSync(raw, { level: 1 })),
    chunk('IEND', Buffer.alloc(0)),
  ])
}
const style = await (await fetch(`https://tiles.openfreemap.org/styles/${styleName}`, { headers: ua })).json()
style.layers = style.layers.filter(
  (l) => l['source-layer'] !== 'poi' && l['source-layer'] !== 'aerodrome_label' && !(strip === '1' && l.type === 'fill-extrusion'),
)
const map = new mbgl.Map({ request, ratio: 1, mode: 'static' })
map.load(style)
const buf = await new Promise((ok, no) =>
  map.render({ width: +size, height: +size, zoom: +zoom, pitch: +pitch, bearing: +bearing, center: [+lon, +lat] }, (e, b) => (e ? no(e) : ok(b))),
)
writeFileSync(out, png(+size, +size, Buffer.from(buf)))
map.release()
