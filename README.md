# Claude Usage

macOS, Windows, and Linux menu bar / system tray app that shows your Claude Code session and weekly usage limits in real time.

## What it shows

- **Session (5h)** — current utilisation % and time until reset
- **Weekly (7d)** — weekly utilisation % and time until reset
- **Overage** — billing-cycle overage if you have a cap configured
- **Analytics** (toggle) — local breakdown of token usage by project, branch, time of day, tool use, and skills — for the last 24h, 7d, or 30d

On macOS the menu bar title shows `47% (70%)` — session % and weekly % at a glance. On Windows and Linux usage is visible in the popup window.

## Requirements

- [Claude Code](https://claude.ai/code) installed and signed in on the same machine

## Install

Download the latest release from [Releases](https://github.com/jackherizsmith/claude-usage-tracker/releases/latest).

### macOS (Apple Silicon)

1. Download `Claude.Usage_*_aarch64.dmg`
2. Open the DMG and drag **Claude Usage** to Applications
3. Remove the quarantine flag — required because the app is not notarised:
   ```
   sudo xattr -cr "/Applications/Claude Usage.app"
   ```
4. Open the app — it appears in the menu bar

### macOS (Intel)

Same as above, but download `Claude.Usage_*_x86_64.dmg`.

### Windows

1. Download `Claude.Usage_*_x64-setup.exe`
2. Run the installer
3. If Windows SmartScreen blocks it: click **More info**, then **Run anyway**

The app appears in the system tray (bottom-right of the taskbar). Click the icon to open the popup.

### Linux

1. Download `claude-usage_*_amd64.AppImage`
2. Make it executable and run:
   ```
   chmod +x claude-usage_*.AppImage
   ./claude-usage_*.AppImage
   ```

The app appears in the system tray. Click the icon to open the popup.

> **GNOME users:** GNOME 42+ hides tray icons by default. Install the [AppIndicator and KStatusNotifierItem Support](https://extensions.gnome.org/extension/615/appindicator-support/) extension, then log out and back in.

KDE, XFCE, and most other desktop environments show the tray icon without any extra steps.

## How it works

The app reads your OAuth token from the same place Claude Code stores it:

| Platform | Location |
|----------|----------|
| macOS | Keychain (`Claude Code-credentials`) |
| Windows | `%USERPROFILE%\.claude\.credentials.json` |
| Linux | `~/.claude/.credentials.json` |

It then makes a minimal API call to Anthropic and reads the rate-limit response headers — the same technique used by [clawdmeter](https://github.com/nicholasgasior/clawdmeter). No data leaves your machine beyond that single API call.

## Build from source

Requires [Rust](https://rustup.rs), Node.js 20+, and platform-specific dependencies.

**Linux** — install system dependencies first:
```bash
sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev
```

**All platforms:**
```bash
git clone https://github.com/jackherizsmith/claude-usage-tracker
cd claude-usage-tracker
npm install
npm run build        # produces platform bundle in src-tauri/target/release/bundle/
```

For local development with hot reload:
```bash
npm run dev
```
