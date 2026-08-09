# igrisv4 — Full Project Directory Structure

## Overview

A hybrid offline/online AI voice assistant built with **Rust** and **Dioxus 0.7**. Contains a bundled **kdeconnect-kde** source tree for reference/integration.

---

## Root Files

```
D:\igrisv4\
├── .env                          # Environment variables (NVIDIA API key, etc.)
├── .gitignore
├── abc.txt
├── build.rs                      # Rust build script (embed-resource, tonic-build)
├── Cargo.lock
├── Cargo.toml                    # Rust dependencies & project config
├── Dioxus.toml                   # Dioxus framework configuration
├── igris.html
├── igrisv3.exe.manifest
├── igrisv3.rc
├── implementation.md
├── README.md
├── tailwind.css
├── description.md                # This file
```

---

## `assets/` — Web/UI assets

```
assets/
├── main.css
└── tailwind.css
```

---

## `icons/` — Application icons

```
icons/
├── icon.ico
├── igris_icon.icns
├── igris_icon.ico
├── igris_icon.svg
└── temp_icon.png
```

---

## `proto/riva/proto/` — gRPC Protobuf definitions (NVIDIA Riva)

```
proto/riva/proto/
├── riva_asr.proto
├── riva_audio.proto
└── riva_common.proto
```

---

## `pkg/` — Runtime packages & data

```
pkg/
├── audio/
│   └── tts_output.wav            # Cached TTS output
├── certs/
│   ├── fastswap_cert.der         # FastSwap self-signed TLS cert
│   └── fastswap_key.der          # FastSwap TLS private key
├── config.json                   # User settings config
├── ecosystem/
│   ├── clipboard_history.json
│   ├── ecosystem_config.json
│   ├── ecosystem_key.pem
│   ├── ecosystem_key.pub
│   └── notification_history.json
├── ffmpeg/
│   ├── bin/
│   │   ├── ffmpeg.exe
│   │   ├── ffplay.exe
│   │   └── ffprobe.exe
│   ├── doc/
│   ├── LICENSE.txt
│   └── presets/
├── models/
│   ├── bold_voice/
│   │   ├── en_US-libritts_r-medium.onnx
│   │   └── en_US-libritts_r-medium.onnx.json
│   ├── sbert/
│   │   ├── config.json
│   │   ├── pytorch_model.bin
│   │   └── tokenizer.json
│   └── sense-voice/
│       ├── model.int8.onnx
│       ├── model.onnx
│       └── tokens.txt
├── permissions.json
└── piper/
    ├── espeak-ng-data/
    ├── espeak-ng.dll
    ├── libtashkeel_model.ort
    ├── onnxruntime.dll
    ├── onnxruntime_providers_shared.dll
    ├── piper
    ├── piper.exe
    ├── piper_phonemize.dll
    └── pkgconfig/
```

---

## `src/` — Main Rust source code

### Root source files

```
src/
├── main.rs              # Dioxus UI entry point + voice loop + LLM routing
├── lib.rs               # Library exports & global state
├── config.rs            # JSON configuration (personality, TTS, hotkey, UI)
├── state.rs             # Global app state
├── processor.rs         # Core processor pipeline
├── voice.rs             # Voice interaction orchestration
├── platform_utils.rs    # Cross-platform utility helpers
```

### `src/core/` — Core engine

```
src/core/
├── about.rs             # Self-presentation data
├── audio_capture.rs     # CPAL audio capture with VAD integration
├── local_llm.rs         # Candle ML local LLM inference (feature-gated)
├── mod.rs
├── stt.rs               # SenseVoice offline STT (sherpa-onnx)
├── tts.rs               # Piper TTS with audio caching
├── vad.rs               # FFT-based Voice Activity Detection
└── wake_word.rs         # Wake word detection with Levenshtein fallback
```

### `src/ui/` — Frontend panels

```
src/ui/
├── alarm_reminder_panel.rs
├── camera_panel.rs          # Camera preview + photo/video controls
├── chat_panel.rs
├── eco_device_panel.rs      # Ecosystem device management
├── fastswap_panel.rs        # File sharing panel with device discovery
├── file_picker.rs           # Native file/folder selector
├── incoming_transfer_popup.rs # Full-screen transfer approval dialog
├── menu_button.rs           # Top-right hamburger menu
├── mod.rs
├── notification_panel.rs
├── presentation/
│   ├── mod.rs               # Presentation state management
│   ├── panel.rs             # Full-screen slide viewer
│   └── slides.rs            # Slide content & TTS narration
├── search_results.rs
├── settings.rs              # Settings modal
├── sidebar.rs
└── system_info_panel.rs
```

### `src/commands/` — Voice command handlers

```
src/commands/
├── about.rs             # Self-presentation command
├── app_utils.rs         # Running app utilities
├── browser_automation.rs
├── ffmpeg_camera.rs     # FFmpeg camera control
├── files.rs             # File create/delete/search
├── mod.rs
├── reminders.rs         # Alarms & reminders scheduler
├── system.rs            # Volume, brightness, WiFi, power, clipboard, etc.
└── web.rs               # Web search, weather, jokes, facts
```

### `src/plugins/` — Plugin system

```
src/plugins/
├── builtin/
│   ├── browsers.rs      # Chrome, Firefox, Edge, Brave, Opera, Safari
│   ├── camera.rs        # Camera open/close/photo/recording
│   ├── communication.rs # Discord, Slack, Teams, Zoom, Skype, Telegram
│   ├── creative.rs      # Photoshop, Premiere, Blender, etc.
│   ├── editors.rs       # VSCode, Sublime, Atom, Vim, Notepad++
│   ├── files.rs         # File create/delete/open
│   ├── gaming.rs        # Steam, Epic, etc.
│   ├── media.rs         # Spotify, VLC, YouTube
│   ├── mod.rs
│   ├── office.rs        # Word, Excel, PowerPoint, Outlook
│   ├── reminders.rs     # Alarm/reminder set/show/cancel
│   ├── system_control.rs # Volume, brightness, wifi, bt, power
│   └── utilities.rs     # Calculator, Notepad, Explorer, Terminal
├── mod.rs
└── system.rs            # Plugin manager (load, match, execute)
```

### `src/nlu/` — Natural Language Understanding

```
src/nlu/
├── context.rs           # Conversation context & reference resolution
├── engine.rs            # Intent engine (SBERT + keyword + Jaccard)
├── mod.rs
├── ner.rs               # Named Entity Recognition
├── nlu_processor.rs     # End-to-end NLU processor
├── sbert.rs             # SBERT sentence embeddings (all-MiniLM-L6-v2)
└── sentence_splitter.rs
```

### `src/eco/` — Ecosystem device mesh (clipboard sync)

```
src/eco/
├── clipboard.rs         # ClipboardManager — polling + diff
├── config.rs            # Ecosystem configuration
├── constants.rs         # Ports, timeouts
├── crypto.rs            # TLS helpers
├── device.rs            # Device identity
├── discovery.rs         # HTTP server (53327) + HTTPS subnet scan (53328)
├── errors.rs            # Error types
├── events.rs            # ClipboardChanged, ClipboardReceived, ClipboardApplied
├── manager.rs           # EcoManager lifecycle (init, start, stop)
├── mod.rs
├── notification.rs
├── pairing.rs
├── permissions.rs       # Permission checks
├── protocol.rs          # ClipboardSyncPayload (text-only)
├── storage.rs           # ClipboardEntry storage
├── sync.rs              # SyncManager — clipboard broadcast
└── transport.rs         # HTTPS clipboard push to peers
```

### `src/fastswap/` — Encrypted P2P file transfer

```
src/fastswap/
├── models/
│   ├── device.rs        # Device, RegisterRequest, Announce
│   ├── mod.rs
│   ├── progress.rs      # FileProgress, TransferProgress with speed/ETA
│   └── transfer.rs      # FileInfo, PrepareUpload, ConfirmUpload
├── mod.rs               # FastSwap manager, approval, cancellation, progress
├── network/
│   ├── client.rs        # HTTPS client (send files, poll server)
│   ├── discovery.rs     # HTTPS subnet scanning (LocalSend protocol)
│   ├── mod.rs
│   └── server.rs        # Axum HTTP server + TLS proxy
└── tls.rs               # Self-signed TLS cert generation (rcgen + ring)
```

### `src/online/` — Online mode (NVIDIA NIM APIs)

```
src/online/
├── intent_router.rs
├── mod.rs               # Online mode state toggle + init_from_env()
├── reasoning.rs         # NVIDIA NIM chat completions + tool calling
├── stt.rs               # Parakeet ASR via tonic gRPC
└── task_planner.rs
```

### `src/platform/` — Platform-specific abstractions

```
src/platform/
├── app_launcher.rs      # OS-specific app launch/close
├── ecosystem/
│   ├── linux.rs
│   ├── macos.rs
│   ├── mod.rs
│   ├── notifications/
│   └── windows.rs
├── file_system.rs       # File operations abstraction
├── mod.rs
├── process_builder.rs   # Cross-platform command builder
└── system_control.rs    # Volume, brightness, WiFi, Bluetooth, power
```

### `src/setup_manager/` — First-run setup

```
src/setup_manager/
├── downloader.rs        # Concurrent model downloads with progress
├── extractor.rs         # Zip extraction
├── gui.rs               # Setup progress GUI
├── mod.rs               # Setup orchestrator, uninstall, verification
├── permissions.rs       # Module-level permission system
├── permissions_ui.rs    # Permission grant UI
├── platforms/
│   ├── linux.rs
│   ├── macos.rs
│   ├── mod.rs
│   └── windows.rs
└── validator.rs         # File integrity checks (SHA256, size)
```

### `src/tools/` — Tool registry

```
src/tools/
├── command_modifier.rs
├── mod.rs
└── registry.rs
```

### `src/utils/` — Utilities

```
src/utils/
├── greetings.rs         # Voice greetings & wake word detection
├── hotkey.rs            # Global hotkey (Ctrl+Shift+Space)
├── mod.rs
├── process_tracker.rs   # Track opened/closed processes
└── shared_memory.rs     # Thread pool for faster processing
```

### `src/media/` — Media modules

```
src/media/
├── ffmpeg_camera/
│   └── mod.rs
└── mod.rs
```

---

## `kdeconnect-kde/` — Bundled KDE Connect source (C++/CMake)

### Root

```
kdeconnect-kde/
├── .craft.ini
├── .editorconfig
├── .gitignore
├── .gitlab-ci.yml
├── .gitlab/
├── .kde-ci.yml
├── CMakeLists.txt
├── CONTRIBUTING.md
├── KDEConnectMacros.cmake
├── README.md
├── REUSE.toml
```

### Core library (`core/`)

```
core/
├── backends/
│   ├── bluetooth/
│   │   ├── bluetoothdevicelink.cpp/h
│   │   ├── bluetoothdownloadjob.cpp/h
│   │   ├── bluetoothlinkprovider.cpp/h
│   │   ├── bluetoothlinkproviderimpl.cpp/h
│   │   ├── bluetoothuploadjob.cpp/h
│   │   ├── CMakeLists.txt
│   │   ├── connectionmultiplexer.cpp/h
│   │   ├── multiplexchannel.cpp/h
│   │   ├── multiplexchannelstate.cpp/h
│   │   └── "Multiplexing protocol.md"
│   ├── devicelink.cpp/h
│   ├── lan/
│   │   ├── avahidiscovery.cpp/h
│   │   ├── CMakeLists.txt
│   │   ├── compositeuploadjob.cpp/h
│   │   ├── landevicelink.cpp/h
│   │   ├── lanlinkprovider.cpp/h
│   │   ├── server.cpp/h
│   │   └── uploadjob.cpp/h
│   ├── linkprovider.cpp/h
│   ├── loopback/
│   │   ├── CMakeLists.txt
│   │   ├── loopbackdevicelink.cpp/h
│   │   └── loopbacklinkprovider.cpp/h
│   └── pairinghandler.cpp/h
├── CMakeLists.txt
├── compositefiletransferjob.cpp/h
├── core_debug.cpp/h
├── daemon.cpp/h
├── dbushelper.cpp/h
├── device.cpp/h
├── deviceinfo.h
├── filetransferjob.cpp/h
├── kdeconnectconfig.cpp/h
├── kdeconnectplugin.cpp/h
├── kdeconnectpluginconfig.cpp/h
├── Messages.sh
├── networkpacket.cpp/h
├── networkpackettypes.h
├── noticationserverinfo.cpp/h
├── openconfig.cpp/h
├── pairstate.h
├── pluginloader.cpp/h
└── sslhelper.cpp/h
```

### Daemon (`daemon/`)

```
daemon/
├── CMakeLists.txt
├── desktop_daemon.cpp/h
├── kdeconnectd.cpp
├── Messages.sh
├── org.kde.kdeconnect.daemon.desktop.cmake
└── org.kde.kdeconnect.service.in
```

### CLI (`cli/`)

```
cli/
├── CMakeLists.txt
├── kdeconnect-cli.cpp
├── kdeconnect.zsh
└── Messages.sh
```

### DBus Interfaces (`dbusinterfaces/`)

```
dbusinterfaces/
├── CMakeLists.txt
├── dbushelpers.h
├── dbusinterfaces.cpp/h
└── systeminterfaces/
    ├── org.freedesktop.Avahi.EntryGroup.xml
    ├── org.freedesktop.Avahi.Server.xml
    ├── org.freedesktop.Avahi.ServiceBrowser.xml
    ├── org.freedesktop.DBus.Properties.xml
    ├── org.freedesktop.login1.xml
    ├── org.freedesktop.portal.InputCapture.xml
    ├── org.freedesktop.portal.RemoteDesktop.xml
    ├── org.freedesktop.portal.Request.xml
    ├── org.freedesktop.portal.Session.xml
    ├── org.freedesktop.ScreenSaver.xml
    ├── org.mpris.MediaPlayer2.Player.xml
    └── org.mpris.MediaPlayer2.xml
```

### App (`app/`)

```
app/
├── CMakeLists.txt
├── main.cpp
├── Messages.sh
├── org.kde.kdeconnect.app.desktop
└── qml/
    ├── BatteryInfo.qml
    ├── ConnectivityInfo.qml
    ├── DevicePage.qml
    ├── Main.qml
    ├── mousepad.qml
    ├── mpris.qml
    ├── MprisSlider.qml
    ├── PluginItem.qml
    ├── PluginSettings.qml
    ├── presentationRemote.qml
    ├── runcommand.qml
    ├── Settings.qml
    ├── volume.qml
    └── WelcomePage.qml
```

### SMS App (`smsapp/`)

```
smsapp/
├── attachmentinfo.cpp/h
├── attachmentshelper.cpp/h
├── CMakeLists.txt
├── conversationlistmodel.cpp/h
├── conversationmodel.cpp/h
├── conversationssortfilterproxymodel.cpp/h
├── gsmasciimap.cpp/h
├── main.cpp
├── Messages.sh
├── org.kde.kdeconnect.sms.desktop
├── qml/
│   ├── AttachmentViewer.qml
│   ├── ChatMessage.qml
│   ├── ConversationDisplay.qml
│   ├── ConversationList.qml
│   ├── Main.qml
│   ├── MessageAttachments.qml
│   └── SendingArea.qml
├── smscharcount.h
├── smshelper.cpp/h
└── thumbnailsprovider.cpp/h
```

### Plasmoid (`plasmoid/`)

```
plasmoid/
├── CMakeLists.txt
├── Messages.sh
└── package/
    ├── contents/
    │   └── ui/
    │       ├── Battery.qml
    │       ├── Clipboard.qml
    │       ├── CompactRepresentation.qml
    │       ├── Connectivity.qml
    │       ├── DeviceDelegate.qml
    │       ├── FindMyPhone.qml
    │       ├── FullRepresentation.qml
    │       ├── main.qml
    │       ├── RemoteCommands.qml
    │       ├── Sftp.qml
    │       ├── Share.qml
    │       ├── SMS.qml
    │       └── VirtualMonitor.qml
    └── metadata.json
```

### Indicator (`indicator/`)

```
indicator/
├── CMakeLists.txt
├── deviceindicator.cpp/h
├── indicatorhelper_mac.cpp
├── indicatorhelper_win.cpp
├── indicatorhelper.cpp/h
├── Info.plist
├── main.cpp
├── Messages.sh
├── org.kde.kdeconnect.nonplasma.desktop
├── serviceregister_mac.h/m.mm
└── systray_actions/
    ├── battery_action.cpp/h
    ├── connectivity_action.cpp/h
    └── systray_actions.h
```

### Models (`models/`)

```
models/
├── CMakeLists.txt
├── commandsmodel.cpp/h
├── conversationmessage.cpp/h
├── devicesmodel.cpp/h
├── devicespluginfilterproxymodel.cpp/h
├── devicessortproxymodel.cpp/h
├── Messages.sh
├── notificationsmodel.cpp/h
├── pluginmodel.cpp/h
├── remotecommandsmodel.cpp/h
└── remotesinksmodel.cpp/h
```

### KIO (`kio/`)

```
kio/
├── CMakeLists.txt
├── kdeconnect-network.desktop
├── kdeconnect.json
├── kiokdeconnect.cpp/h
├── Messages.sh
└── solid_kdeconnect.desktop
```

### Data (`data/`)

```
data/
├── CMakeLists.txt
├── kdeconnect-dde.desktop
├── kdeconnect-thunar.desktop
├── kdeconnect.contract
├── kdeconnect.ufw
└── org.kde.kdeconnect.metainfo.xml
```

### Build helpers (`cmake/`)

```
cmake/
├── DbusActivationMacros.cmake
├── FindLibFakeKey.cmake
└── FindXTest.cmake
```

### Platform integration

```
urlhandler/
├── CMakeLists.txt
├── dialog.ui
├── kdeconnect-handler.cpp
├── Messages.sh
└── org.kde.kdeconnect.handler.desktop

nautilus-extension/
├── CMakeLists.txt
├── kdeconnect-share.py
└── Messages.sh

fileitemactionplugin/
├── CMakeLists.txt
├── kdeconnectsendfile.json
├── Messages.sh
├── sendfileitemaction.cpp/h

declarativeplugin/
├── CMakeLists.txt
├── kdeconnectdeclarativeplugin.cpp/h
├── objectfactory.cpp/h
├── pointerlocker.cpp/h
├── pointerlockerwayland.cpp/h
├── qml/
│   ├── DBusProperty.qml
│   ├── PluginChecker.qml
│   └── RemoteKeyboard.qml
└── responsewaiter.cpp/h
```

### Icons (`icons/`)

```
icons/
├── app/
│   ├── sc-apps-kdeconnect.svg
│   ├── sc-apps-kdeconnectindicator.svg
│   └── sc-apps-kdeconnectindicatordark.svg
├── CMakeLists.txt
├── custom_icons.qrc
├── status/
│   ├── 16-status-{laptop,smartphone,tablet,tv}{connected,disconnected,trusted}.svg (x4)
│   ├── 22-status-{laptop,smartphone,tablet,tv}{connected,disconnected,trusted}.svg (x4)
│   └── 32-status-{laptop,smartphone,tablet,tv}{connected,disconnected,trusted}.svg (x4)
└── windows/
    ├── 128-apps-kdeconnect.png
    ├── 256-apps-kdeconnect.png
    ├── 32-apps-kdeconnect.png
    ├── 48-apps-kdeconnect.png
    ├── 512-apps-kdeconnect.png
    └── 64-apps-kdeconnect.png
```

### Plugins (`plugins/`) — 34 plugin directories

```
plugins/
├── battery/              # Battery status reporting
├── clipboard/            # Clipboard sync
├── connectivity-report/  # Network connectivity info
├── contacts/             # Contact sharing
├── digitizer/            # Tablet input forwarding
├── findmyphone/          # Ring device
├── findthisdevice/       # Find this device
├── lockdevice/           # Lock remote device
├── mmtelephony/          # ModemManager telephony
├── mousepad/             # Remote touchpad (X11/Wayland/macOS/Windows)
├── mpriscontrol/         # Control remote media player
├── mprisremote/          # Expose local media player via MPRIS
├── notifications/        # Receive notifications
├── pausemusic/           # Pause music on call
├── ping/                 # Ping/buzz device
├── presenter/            # Presentation remote
├── remotecommands/       # Execute predefined commands
├── remotecontrol/        # Remote control input
├── remotekeyboard/       # Remote keyboard input
├── runcommand/           # Run commands on remote
├── screensaver-inhibit/  # Inhibit screensaver
├── sendnotifications/    # Forward notifications (DBus/Windows)
├── sftp/                 # Browse remote filesystem
├── share/                # Share files/URLs
├── shareinputdevices/    # Share mouse/keyboard via Portal
├── shareinputdevicesremote/ # Remote side of input sharing
├── sms/                  # SMS messaging
├── systemvolume/         # System volume control
├── telephony/            # Telephony states
├── virtualmonitor/       # Virtual monitor (GNOME/KWin)
├── CMakeLists.txt
├── kdeconnect.notifyrc
├── kdeconnect.schema.json
├── Messages.sh
└── README.txt
```

### Tests (`tests/`)

```
tests/
├── CMakeLists.txt
├── notificationstest.cpp
├── pluginloadtest.cpp
├── sendfiletest.cpp
├── smshelpertest.cpp
├── testdaemon.h
├── testdevice.cpp
└── testdevice.h
```

### Translations (`po/`) — 58 languages

```
po/
├── ar/  ast/  az/  bg/  bs/  ca/  ca@valencia/  cs/  da/  de/
├── el/  en_GB/  eo/  es/  et/  eu/  fi/  fr/  ga/  gl/
├── he/  hi/  hu/  ia/  id/  ie/  is/  it/  ja/  ka/
├── ko/  lt/  lv/  ml/  nb/  nl/  nn/  pa/  pl/  pt/
├── pt_BR/  ro/  ru/  sa/  sk/  sl/  sr/  sr@ijekavian/
├── sr@ijekavianlatin/  sr@latin/  sv/  ta/  tg/  tr/
├── ug/  uk/  zh_CN/  zh_TW/
└── Each language has 13 .po files:
    ├── kdeconnect-app.po
    ├── kdeconnect-cli.po
    ├── kdeconnect-core.po
    ├── kdeconnect-fileitemaction.po
    ├── kdeconnect-indicator.po
    ├── kdeconnect-interfaces.po
    ├── kdeconnect-kded.po
    ├── kdeconnect-kio.po
    ├── kdeconnect-nautilus-extension.po
    ├── kdeconnect-plugins.po
    ├── kdeconnect-sms.po
    ├── kdeconnect-urlhandler.po
    └── plasma_applet_org.kde.kdeconnect.po
```

### Licenses (`LICENSES/`)

```
LICENSES/
├── Apache-2.0.txt
├── BSD-3-Clause.txt
├── CC-BY-SA-4.0.txt
├── CC0-1.0.txt
├── GPL-2.0-only.txt
├── GPL-2.0-or-later.txt
├── GPL-3.0-only.txt
├── GPL-3.0-or-later.txt
├── LGPL-2.1-only.txt
├── LGPL-3.0-only.txt
├── LicenseRef-KDE-Accepted-GPL.txt
├── LicenseRef-KDE-Accepted-LGPL.txt
└── MIT.txt
```

---

## Dependency Graph (Rust `Cargo.toml`)

| Category | Crates |
|---|---|
| **UI** | `dioxus 0.7` (desktop), `dioxus-desktop 0.7`, `manganis 0.7` |
| **Speech/Audio** | `sherpa-onnx 1.11`, `cpal 0.16`, `rodio 0.17` |
| **Async** | `tokio 1` (full), `async-std 1.12` |
| **Serialization** | `serde 1` (derive), `serde_json 1` |
| **NLU** | `anyhow`, `regex`, `fuzzy-matcher 0.3`, `urlencoding` |
| **FFT** | `rustfft 6.1`, `num-complex 0.4` |
| **Setup/Downloads** | `reqwest 0.11` (stream, blocking, json, multipart), `zip 0.6`, `scraper 0.20`, `indicatif 0.17`, `console 0.16` |
| **Online (NVIDIA)** | `base64`, `dotenv`, `tonic 0.12` (tls, tls-roots), `prost 0.13` |
| **FastSwap** | `axum 0.7`, `tower 0.5`, `tower-http 0.6` (fs, cors), `bytes`, `tokio-stream`, `tokio-util`, `local-ip-address 0.6`, `whoami`, `mime_guess`, `rfd 0.15`, `rcgen 0.13`, `rustls 0.23` (ring), `tokio-rustls 0.26`, `hyper-util 0.1` |
| **Local LLM** (optional) | `candle-core 0.10` (metal), `candle-nn`, `candle-transformers`, `tokenizers 0.22` |
| **Platform-specific** | `winapi 0.3` (Windows), `windows 0.52` (Windows), `libc` (Unix) |
| **Build** | `embed-resource 1.8`, `tonic-build 0.12`, `protoc-bin-vendored 3.0` |
