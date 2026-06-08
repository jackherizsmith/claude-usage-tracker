// Generates build/icon.png (1024x1024) — run once before packaging
const fs = require('fs')
const path = require('path')
const zlib = require('zlib')

const W = 1024, H = 1024
const stride = 1 + W * 4
const px = Buffer.alloc(H * stride, 0)

function setPixel(x, y, r, g, b, a) {
  const i = y * stride + 1 + x * 4
  px[i] = r; px[i+1] = g; px[i+2] = b; px[i+3] = a
}

function fillRect(x, y, w, h, r, g, b, a = 255) {
  for (let dy = 0; dy < h; dy++) for (let dx = 0; dx < w; dx++) {
    const px_x = x + dx, py = y + dy
    if (px_x >= 0 && px_x < W && py >= 0 && py < H)
      setPixel(px_x, py, r, g, b, a)
  }
}

// Background: dark purple
for (let y = 0; y < H; y++) for (let x = 0; x < W; x++) setPixel(x, y, 22, 14, 42, 255)

// Glyph: white bar chart — same proportions as tray icon, scaled to inner 768×768
const M = 128   // margin
const S = 43    // px per logical unit (768px / 18 logical units ≈ 43)

fillRect(M,           M,           S*2,  S*18, 255, 255, 255) // spine, full height
fillRect(M + S*2,     M + S*3,     S*12, S*5,  255, 255, 255) // top bar
fillRect(M + S*2,     M + S*10,    S*7,  S*5,  255, 255, 255) // bottom bar

// PNG encode
function crc32(buf) {
  let c = 0xFFFFFFFF
  for (const b of buf) { c ^= b; for (let j = 0; j < 8; j++) c = (c >>> 1) ^ (c & 1 ? 0xEDB88320 : 0) }
  return (~c) >>> 0
}

function chunk(type, data) {
  const t = Buffer.from(type)
  const l = Buffer.alloc(4); l.writeUInt32BE(data.length)
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(Buffer.concat([t, data])))
  return Buffer.concat([l, t, data, crc])
}

const ihdr = Buffer.alloc(13)
ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4); ihdr[8] = 8; ihdr[9] = 6

const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  chunk('IHDR', ihdr),
  chunk('IDAT', zlib.deflateSync(px)),
  chunk('IEND', Buffer.alloc(0))
])

const out = path.join(__dirname, '..', 'build', 'icon.png')
fs.writeFileSync(out, png)
console.log(`Icon written: ${out} (${(png.length / 1024).toFixed(0)} KB)`)
