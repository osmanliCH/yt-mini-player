# ⚡ YT Mini Player

A lightning-fast, highly polished desktop media controller built with **Rust**, **Tauri v2**, and modern HTML/CSS. Designed for Windows with zero-latency integration.

![YT Mini Player Showcase](app-icon.png)

## ✨ Features
- **Native Windows Audio Integration:** Direct control via Windows CoreAudio (`IAudioEndpointVolume`, `ISimpleAudioVersion`) and Media Transport Controls (`SMTC`).
- **Liquid Fire & Plasma Visualizer:** Real-time 60-FPS audio-reactive flame simulation responding to actual beats and vocals.
- **Global Hotkeys:** 
  - `Ctrl + Alt + M`: Master Show / Hide (with smooth dissolve animation)
  - `Ctrl + Alt + P`: Play / Pause
  - `Ctrl + Alt + N`: Next Track
  - `Ctrl + Alt + B`: Previous Track
- **Featherlight Performance:** Consumes ~2 MB disk space and almost zero RAM by leveraging Windows Edge WebView2.
- **Interactive HUD:** Mouse-wheel volume control, clickable timeline scrubbing, and an expandable song history drawer.

## 🚀 Tech Stack
- **Backend:** Rust, Tauri v2, Windows API (`windows` crate)
- **Frontend:** HTML5, CSS3, Canvas API, JavaScript

## 📥 Installation & Build
Make sure you have [Rust](https://www.rust-lang.org/) and [Node.js](https://nodejs.org/) installed.

```bash
# Clone the repository
git clone [https://github.com/osmanliCH/yt-mini-plyer.git](https://github.com/osmanliCH/yt-mini-plyer.git)
cd yt-mini-player

# Install dependencies
npm install

# Run in development mode
npm run tauri dev

# Build the release .exe
npm run tauri build
