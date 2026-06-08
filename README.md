# Claude Usage

macOS, Windows, and Linux menu bar / system tray app that shows your Claude Code session and weekly usage limits in real time.

## What it shows

- **Session (5h)** — current utilisation % and time until reset
- **Weekly (7d)** — weekly utilisation % and time until reset
- **Overage** — billing-cycle overage if you have a cap configured
- **Analytics** (toggle) — local breakdown of token usage by project, branch, time of day, tool use, and skills — for the last 24h, 7d, or 30d

On macOS, the tray title shows `47% (70%)` — session % and weekly % at a glance.

## Requirements

- [Claude Code](https://claude.ai/code) installed and signed in on the same machine

## Install

Download the latest release for your platform from [Releases](https://github.com/jackherizsmith/claude-usage-tracker/releases/latest).

### macOS (Apple Silicon)

1. Download `Claude.Usage_*_aarch64.dmg`
2. Open the DMG and drag **Claude Usage** to Applications
3. Remove the quarantine flag (required for unsigned apps):
   ```
   sudo xattr -cr "/Applications/Claude Usage.app"
   ```
4. Open the app — it will appear in your menu bar

### macOS (Intel)

Same as above, but download `Claude.Usage_*_x86_64.dmg`.

### Windows

1. Download `Claude.Usage_*_x64-setup.exe`
2. Run the installer
3. If Windows SmartScreen shows a warning, click **More info → Run anyway**

The app appears in the system tray (bottom-right notification area).

### Linux

1. Download `claude-usage_*_amd64.AppImage`
2. Make it executable and run:
   ```
   chmod +x claude-usage_*.AppImage
   ./claude-usage_*.AppImage
   ```

The app appears in the system tray. Requires a desktop environment with tray support (GNOME, KDE, XFCE, etc.).

## Build from source

Requires [Rust](https://rustup.rs), Node.js 20+, and platform-specific Tauri dependencies.

**Linux** — install system dependencies first:
```bash
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
```

**All platforms:**
```bash
git clone https://github.com/jackherizsmith/claude-usage-tracker
cd claude-usage-tracker
npm install
npm run build        # produces platform-native bundle in src-tauri/target/release/bundle/
```

For local development with hot reload: `npm run dev`
