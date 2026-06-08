# Claude Usage

macOS menu bar app that shows your Claude Code session and weekly usage limits in real time.

![Menu bar showing 47% (70%)](https://github.com/user-attachments/assets/placeholder)

## What it shows

- **Session (5h)** — current utilisation % and time until reset
- **Weekly (7d)** — weekly utilisation % and time until reset  
- **Overage** — billing-cycle overage if you have a cap configured
- **Analytics** (toggle) — local breakdown of token usage by project, branch, time of day, tool use, and skills

The menu bar title shows `47% (70%)` — session % and weekly % at a glance.

## Requirements

- macOS 12 or later
- [Claude Code](https://claude.ai/code) installed and signed in (the app reads your OAuth token from Keychain — no separate auth needed)

## Install

1. Download `Claude Usage-x.x.x-universal.dmg` from the [latest release](https://github.com/jackherizsmith/claude-usage-tracker/releases/latest)
2. Open the DMG and drag **Claude Usage** to your Applications folder
3. Right-click the app → **Open** (required on first launch since the app is unsigned)
4. If macOS says "app is damaged": open Terminal and run:
   ```
   sudo xattr -cr "/Applications/Claude Usage.app"
   ```
   Then try opening again.

The app lives in your menu bar and auto-starts on login.

## Build from source

```bash
git clone https://github.com/jackherizsmith/claude-usage-tracker
cd claude-usage-tracker
npm install
npm start          # run in dev mode
npm run build      # build DMG + ZIP to dist/
```

Requires Node.js 18+ and npm.
