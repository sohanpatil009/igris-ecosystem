# IGRIS v3 - Offline AI Voice Assistant

A fully offline, privacy-focused voice assistant built with **Rust** and **Dioxus 0.7**. Wake word "Arise" activates hands-free desktop control.

![IGRIS](icons/igris_icon.svg)

## Key Features

### Voice Pipeline
- **Wake Word** - Say "Arise" (or "hey igris", "hi igris") to activate
- **STT** - Whisper (small-q8_0 → base-q8_0 → base) via `whisper-rs`
- **VAD** - Real-time energy + zero-crossing rate detection (configurable)
- **TTS** - Piper TTS with LibriTTS voice, audio caching
- **NLU** - Fallback SBERT (character trigram hashing, not real model) + keyword matching + Jaccard similarity

### Multi-Personality
- **Igris** - Purple theme, deep male voice (speaker 051)
- **Alita** - Lavender/pink theme, female voice (speaker 001)
- **Custom** - Configurable name/wake word

### Plugin System (12 built-in categories)
- Browsers, Editors, Communication, Media, Office, Creative, Gaming, Utilities, Camera, Files, Reminders, System Control
- 5-pass matching: exact trigger → exact example → contains → fuzzy → all-keywords
- Custom JSON plugins supported

### System Control
- Volume (set/increase/decrease/mute), Brightness, WiFi, Bluetooth
- Shutdown, Restart, Lock Screen, Sleep

### File Operations
- Create, delete, read, write files
- Multi-threaded async file/folder search across all drives
- Results displayed in search results UI panel

### FFmpeg Camera
- Take photos, record video+audio (H.264 + AAC)
- Platform: DirectShow (Win), V4L2 (Linux), AVFoundation (macOS)

### Alarms & Reminders
- Time-based alarms, duration-based reminders
- Background scheduler (10s check interval)

### Web Search
- Google scraping with featured snippet extraction
- Search in specific browsers

### FastSwap File Sharing
- LocalSend v2 compatible (port 53317)
- LAN discovery via HTTP, AES-GCM encryption
- Incoming transfer popup with accept/deny
- Real-time progress (sender + receiver)
- 60s approval timeout

### Self-Presentation Mode
- Animated slides with TTS narration
- Voice command: "Tell me about yourself"

### Setup Manager
- Auto-downloads Whisper, Piper, FFmpeg models
- Permission system for modules
- Platform-specific downloads (zip/tar)

## Quick Start

```bash
git clone https://github.com/sohanpatil009/igrisv3.git
cd igrisv3
cargo run 
```

### Prerequisites
- Rust 1.70+, Windows/macOS/Linux, 4GB RAM
- **Models auto-download** on first launch (~200MB total):
  Whisper STT, Piper TTS + espeak-ng, FFmpeg

### First Use
1. Wait for setup to complete
2. Say **"Arise"** (or press **Ctrl+Shift+Space**) to wake IGRIS
3. Give commands: "Open Chrome", "Increase volume", "Tell me about yourself"

## Voice Commands

```
"Open Chrome" / "Close Firefox" / "Close all apps"
"Increase volume by 20" / "Set brightness to 80" / "Mute"
"Enable WiFi" / "Disable Bluetooth"
"Lock screen" / "Shutdown" / "Restart"
"Search for *.pdf files" / "Create file notes.txt"
"Take a photo" / "Start recording" / "Stop recording"
"Set alarm for 7 am" / "Remind me in 30 minutes"
"Search for Rust programming" / "What is AI?"
"Open FastSwap" / "Share files"
"Tell me about yourself" / "Sleep" / "Exit"
```

## Architecture

```
src/
├── main.rs              # Dioxus UI + voice loop
├── config.rs            # JSON config (Personality, TTS, Hotkey, UI)
├── core/                # Voice pipeline
│   ├── stt.rs, tts.rs, vad.rs, wake_word.rs, audio_capture.rs
│   └── about.rs         # Self-introduction module
├── nlu/                 # NLU (engine, sbert-fallback, ner, context)
├── commands/            # Handlers (system, files, web, camera, reminders, about)
├── plugins/             # Plugin system + 12 built-in plugin modules
├── ui/                  # Dioxus components (settings, camera, fastswap, presentation, search)
├── fastswap/            # File transfer (HTTP server/client, discovery, AES-GCM)
├── media/               # FFmpeg camera abstraction
├── platform/            # Cross-platform (app_launcher, system_control, file_system)
├── setup_manager/       # First-run setup (downloader, extractor, permissions)
└── utils/               # Hotkey, greetings, process_tracker, shared_memory
```

### Data Flow
```
Audio → VAD → Whisper STT → NLU (hash-based SBERT + keyword) → NER → Plugin System → Action → TTS
```

## Configuration

Settings saved to `pkg/config.json`:

```json
{
  "personality": "Igris",
  "recognition": { "sensitivity": 0.45, "max_listen_sec": 15 },
  "tts": { "speed": 1.0, "volume": 0.8 },
  "hotkey": { "modifier": "Ctrl+Shift", "key": "Space" }
}
```

## Models

| Model | Size | Purpose |
|-------|------|---------|
| ggml-small-q8_0.bin | ~50MB | Whisper STT (preferred, fast) |
| ggml-base-q8_0.bin | ~81MB | Whisper STT (fallback) |
| en_US-libritts_r-medium.onnx | ~50MB | Piper TTS voice |
| espeak-ng-data | ~12MB | Phoneme data for Piper |
| FFmpeg | ~100MB | Camera capture (Windows only) |

## Development

```bash
cargo build              # Debug build
cargo build --release    # Release build (recommended for STT perf)
cargo test               # Run tests
```

## License

MIT - see [LICENSE](LICENSE)

---

**IGRIS v3** - *Say "Arise" to begin.*
