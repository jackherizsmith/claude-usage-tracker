const { app, BrowserWindow, Tray, ipcMain, nativeImage, screen } = require('electron')
const path = require('path')
const fs = require('fs')
const os = require('os')
const https = require('https')
const zlib = require('zlib')
const { execSync } = require('child_process')

function crc32(buf) {
  let c = 0xFFFFFFFF
  for (const b of buf) { c ^= b; for (let j = 0; j < 8; j++) c = (c >>> 1) ^ (c & 1 ? 0xEDB88320 : 0) }
  return (~c) >>> 0
}

function pngChunk(type, data) {
  const t = Buffer.from(type)
  const l = Buffer.alloc(4); l.writeUInt32BE(data.length)
  const crc = Buffer.alloc(4); crc.writeUInt32BE(crc32(Buffer.concat([t, data])))
  return Buffer.concat([l, t, data, crc])
}

function createTrayIcon() {
  const W = 36, H = 36
  const stride = 1 + W * 4
  const px = Buffer.alloc(H * stride, 0)

  function fill(x, y, w, h) {
    for (let dy = 0; dy < h; dy++) for (let dx = 0; dx < w; dx++) {
      const i = (y + dy) * stride + 1 + (x + dx) * 4
      if (i + 3 < px.length) { px[i] = px[i+1] = px[i+2] = 0; px[i+3] = 255 }
    }
  }

  fill(0,  0,  4, 36)  // spine: 2 logical px wide, full height
  fill(4,  6, 24, 10)  // top bar: 12 logical px wide, 5 tall (stickout: 3px above)
  fill(4, 20, 14, 10)  // bottom bar: 7 logical px wide, 5 tall (stickout: 3px below)

  const ihdr = Buffer.alloc(13)
  ihdr.writeUInt32BE(W, 0); ihdr.writeUInt32BE(H, 4); ihdr[8] = 8; ihdr[9] = 6

  const buf = Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk('IHDR', ihdr),
    pngChunk('IDAT', zlib.deflateSync(px)),
    pngChunk('IEND', Buffer.alloc(0))
  ])

  const img = nativeImage.createFromBuffer(buf, { scaleFactor: 2.0 })
  img.setTemplateImage(true)
  return img
}

let tray = null
let popupWindow = null
let refreshTimer = null
let cachedData = null
let polling = false

let analyticsCache = null
let analyticsCacheTime = 0
const ANALYTICS_CACHE_MS = 5 * 60 * 1000

async function getAnalytics(days = 30) {
  const now = Date.now()
  if (analyticsCache?.days === days && now - analyticsCacheTime < ANALYTICS_CACHE_MS) {
    return analyticsCache
  }

  const base = path.join(os.homedir(), '.claude', 'projects')
  const cutoff = now - days * 24 * 60 * 60 * 1000

  const byProject = {}, byBranch = {}, byHour = new Array(24).fill(0), byDay = {}, byTool = {}, byModel = {}, bySkill = {}
  const seenLeafs = new Set()

  let dirs
  try { dirs = await fs.promises.readdir(base) } catch { return null }

  await Promise.all(dirs.map(async (dir) => {
    const dirPath = path.join(base, dir)
    try { if (!(await fs.promises.stat(dirPath)).isDirectory()) return } catch { return }
    let files
    try { files = await fs.promises.readdir(dirPath) } catch { return }

    await Promise.all(files.filter(f => f.endsWith('.jsonl')).map(async (file) => {
      try {
        const filePath = path.join(dirPath, file)
        if ((await fs.promises.stat(filePath)).mtimeMs < cutoff) return
        const text = await fs.promises.readFile(filePath, 'utf8')
        const lines = text.split('\n')

        // First pass: build uuid→timestamp map for dating last-prompt entries
        const uuidTs = {}
        for (const line of lines) {
          try {
            const e = JSON.parse(line)
            if (e.uuid && e.timestamp) uuidTs[e.uuid] = new Date(e.timestamp).getTime()
          } catch {}
        }

        const skillRe = /^\/([^/\s]+)(?:\s|$)/
        for (const line of lines) {
          if (!line.trim()) continue
          let e
          try { e = JSON.parse(line) } catch { continue }

          // Skill invocations live in last-prompt entries
          if (e.type === 'last-prompt' && e.leafUuid) {
            const ts = uuidTs[e.leafUuid]
            if (ts && ts < cutoff) continue
            const m = (e.lastPrompt || '').trimStart().match(skillRe)
            if (m && !seenLeafs.has(e.leafUuid)) {
              seenLeafs.add(e.leafUuid)
              bySkill[m[1]] = (bySkill[m[1]] || 0) + 1
            }
            continue
          }

          if (!e.timestamp || e.type !== 'assistant') continue
          const ts = new Date(e.timestamp)
          if (isNaN(ts) || ts.getTime() < cutoff) continue

          const usage = e.message?.usage
          const project = e.cwd ? path.basename(e.cwd) : 'unknown'
          const branch = e.gitBranch || 'unknown'

          if (usage) {
            const input = (usage.input_tokens || 0) + (usage.cache_creation_input_tokens || 0) + (usage.cache_read_input_tokens || 0)
            const output = usage.output_tokens || 0

            if (!byProject[project]) byProject[project] = { input: 0, output: 0 }
            byProject[project].input += input
            byProject[project].output += output

            if (!byBranch[branch]) byBranch[branch] = { input: 0, output: 0 }
            byBranch[branch].input += input
            byBranch[branch].output += output

            byHour[ts.getHours()] += output
            const day = ts.toISOString().split('T')[0]
            byDay[day] = (byDay[day] || 0) + output

            const model = e.message?.model
            if (model) byModel[model] = (byModel[model] || 0) + 1
          }

          const content = e.message?.content
          if (Array.isArray(content)) {
            for (const block of content) {
              if (block?.type === 'tool_use' && block.name) {
                byTool[block.name] = (byTool[block.name] || 0) + 1
              }
            }
          }
        }
      } catch {}
    }))
  }))

  analyticsCache = { byProject, byBranch, byHour, byDay, byTool, bySkill, byModel, days }
  analyticsCacheTime = now
  return analyticsCache
}

function extractAccessToken(blob) {
  blob = blob.trim()
  if (!blob) return null
  try {
    const data = JSON.parse(blob)
    if (typeof data?.accessToken === 'string') return data.accessToken
    for (const v of Object.values(data)) {
      if (v && typeof v === 'object' && typeof v.accessToken === 'string') return v.accessToken
    }
  } catch {}
  const m = blob.match(/"accessToken"\s*:\s*"([^"]+)"/)
  return m ? m[1] : null
}

function readToken() {
  if (process.platform === 'darwin') {
    try {
      const blob = execSync(
        `security find-generic-password -s "Claude Code-credentials" -a "${os.userInfo().username}" -w`,
        { encoding: 'utf8', timeout: 10000 }
      ).trim()
      return extractAccessToken(blob)
    } catch { return null }
  }
  const credPath = path.join(os.homedir(), '.claude', '.credentials.json')
  try {
    return extractAccessToken(fs.readFileSync(credPath, 'utf8'))
  } catch { return null }
}

function pollUsage(token) {
  return new Promise((resolve, reject) => {
    const body = JSON.stringify({
      model: 'claude-haiku-4-5-20251001',
      max_tokens: 1,
      messages: [{ role: 'user', content: 'hi' }]
    })

    const req = https.request({
      hostname: 'api.anthropic.com',
      path: '/v1/messages',
      method: 'POST',
      headers: {
        'anthropic-version': '2023-06-01',
        'anthropic-beta': 'oauth-2025-04-20',
        'Content-Type': 'application/json',
        'Content-Length': Buffer.byteLength(body),
        'User-Agent': 'claude-code/2.1.5',
        'Authorization': `Bearer ${token}`
      },
      timeout: 20000
    }, (res) => {
      res.resume()

      if (res.statusCode >= 400) {
        reject(new Error(`API returned ${res.statusCode}`))
        return
      }

      const h = res.headers

      function hdr(name, def = '') { return h[name] || def }
      function pct(name) {
        const v = parseFloat(hdr(name))
        return isNaN(v) ? null : Math.round(v * 100)
      }
      function resetIso(name) {
        const v = parseFloat(hdr(name))
        return isNaN(v) ? null : new Date(v * 1000).toISOString()
      }

      resolve({
        session: {
          pct: pct('anthropic-ratelimit-unified-5h-utilization'),
          resetAt: resetIso('anthropic-ratelimit-unified-5h-reset'),
          status: hdr('anthropic-ratelimit-unified-5h-status', 'normal')
        },
        weekly: {
          pct: pct('anthropic-ratelimit-unified-7d-utilization'),
          resetAt: resetIso('anthropic-ratelimit-unified-7d-reset'),
          status: hdr('anthropic-ratelimit-unified-7d-status', 'normal')
        },
        overage: {
          pct: pct('anthropic-ratelimit-unified-overage-utilization'),
          resetAt: resetIso('anthropic-ratelimit-unified-overage-reset'),
          status: hdr('anthropic-ratelimit-unified-overage-status', 'allowed')
        },
        fallback: {
          available: hdr('anthropic-ratelimit-unified-fallback') === 'available',
          pct: Math.round(parseFloat(hdr('anthropic-ratelimit-unified-fallback-percentage') || '0') * 100)
        },
        updatedAt: new Date().toISOString(),
        error: null
      })
    })

    req.on('error', reject)
    req.on('timeout', () => { req.destroy(); reject(new Error('Request timed out')) })
    req.write(body)
    req.end()
  })
}

async function fetchAndCache() {
  if (polling) return
  polling = true
  try {
    const token = readToken()
    if (!token) {
      cachedData = { error: 'No Claude Code token found in Keychain', updatedAt: new Date().toISOString() }
      updateTrayTitle()
      return
    }
    const result = await pollUsage(token)
    cachedData = result
    updateTrayTitle()
    if (popupWindow?.isVisible()) {
      popupWindow.webContents.send('refresh')
    }
  } catch (e) {
    cachedData = {
      ...(cachedData || {}),
      error: e.message,
      updatedAt: new Date().toISOString()
    }
    updateTrayTitle()
  } finally {
    polling = false
  }
}

function remainingDots(resetAt, unitMs) {
  if (!resetAt) return ''
  const remaining = new Date(resetAt).getTime() - Date.now()
  if (remaining <= 0) return ''
  return '.'.repeat(Math.floor(remaining / unitMs) + 1)
}

function updateTrayTitle() {
  if (!tray) return
  const s = cachedData?.session?.pct
  const w = cachedData?.weekly?.pct
  if (s == null) { tray.setTitle(' –'); return }
  const sDots = remainingDots(cachedData.session.resetAt, 60 * 60 * 1000)
  const wDots = remainingDots(cachedData.weekly.resetAt, 24 * 60 * 60 * 1000)
  tray.setTitle(w != null ? ` ${s}%${sDots} (${w}%${wDots})` : ` ${s}%${sDots}`)
}

function positionWindow(win) {
  const trayBounds = tray.getBounds()
  const display = screen.getDisplayNearestPoint({ x: trayBounds.x, y: trayBounds.y })
  const winSize = win.getSize()
  const x = Math.round(trayBounds.x + trayBounds.width / 2 - winSize[0] / 2)
  const y = trayBounds.y > display.bounds.height / 2
    ? trayBounds.y - winSize[1] - 6
    : trayBounds.y + trayBounds.height + 6
  win.setPosition(x, Math.max(y, display.bounds.y + 6))
}

function createPopupWindow() {
  popupWindow = new BrowserWindow({
    width: 380,
    height: 680,
    show: false,
    frame: false,
    resizable: true,
    alwaysOnTop: true,
    skipTaskbar: true,
    vibrancy: 'under-window',
    visualEffectState: 'active',
    transparent: true,
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false
    }
  })

  popupWindow.loadFile('index.html')

}

app.whenReady().then(async () => {
  app.dock.hide()
  app.setLoginItemSettings({ openAtLogin: true, openAsHidden: true })

  tray = new Tray(createTrayIcon())
  tray.setToolTip('Claude Usage')

  createPopupWindow()

  // Initial poll
  await fetchAndCache()

  let positioned = false
  tray.on('click', () => {
    if (popupWindow.isVisible()) {
      popupWindow.hide()
    } else {
      if (!positioned) { positionWindow(popupWindow); positioned = true }
      popupWindow.show()
      popupWindow.focus()
      popupWindow.webContents.send('refresh')
    }
  })

  // Poll every 60s
  refreshTimer = setInterval(() => fetchAndCache(), 60_000)
})

ipcMain.on('set-height', (_, h) => {
  if (popupWindow && !popupWindow.isDestroyed()) {
    const [w] = popupWindow.getSize()
    popupWindow.setSize(w, Math.round(h))
  }
})

ipcMain.handle('get-usage', () => cachedData)
ipcMain.handle('get-analytics', (_, days) => getAnalytics(days || 30))

ipcMain.handle('refresh-now', async () => {
  await fetchAndCache()
  return cachedData
})

ipcMain.on('quit', () => app.quit())

app.on('window-all-closed', () => {})
app.on('before-quit', () => clearInterval(refreshTimer))
