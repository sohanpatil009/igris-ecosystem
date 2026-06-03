# README vs Actual Code Comparison

## Critical Discrepancies

### 1. SBERT NLU (Misleading)
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Model | "all-MiniLM-L6-v2 \| 80MB \| SBERT embeddings" | **No real SBERT model is used.** `sbert.rs` has a `SbertTokenizer` class but the model path `./pkg/models/sbert/` is checked — if missing, it falls back to **character trigram hashing + word frequency** generating 384-dim vectors from hashed n-grams (`sbert.rs:318-347`). This is NOT semantic understanding, it's a hash-based bag-of-characters approximation. |
| Download | Listed as auto-downloaded (80MB) | The `downloader.rs` does NOT download any SBERT model. |
| Similarity | "SBERT-powered semantic intent recognition" | Uses `cosine_similarity` on hash embeddings + Jaccard similarity + keyword fallback. Works adequately but is NOT SBERT. |
| **Severity** | **HIGH** - This is false advertising of a core AI capability | |

### 2. Missing Personality Documentation
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Personalities | Only "IGRIS" mentioned | Code has FULL **Alita** personality: different wake word ("alita"), speaker ID ("001"), color scheme (lavender/pink), greeting messages, invoke responses (`config.rs:62-102`) |
| Color scheme | "Purple Accent, Cyan Accent" | Code has: IGRIS awake=purple, Alita awake=lavender/pink, standby=cyan/blue (`main.rs:1217-1228`) |
| **Severity** | **MEDIUM** | |

### 3. Self-Presentation Mode Description
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Detail | "Animated Slides, Interactive Diagrams, Visual Flowcharts" | Code has `ui/presentation/` with panel + slides modules. They display text-based slides (about sections). No actual interactive diagrams, no visual flowcharts. Purely text slides with TTS. (`presentation/panel.rs`, `presentation/slides.rs`, `about.rs`) |
| **Severity** | **LOW** - Exaggerated description | |

### 4. Voice Command Examples
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| "Close all applications" | Listed | Code supports it via `CUSTOM_FN:close_all_apps` (`main.rs:630-642`) — **WORKS** |
| "Shutdown" | Listed | Code routes `shutdown` through system commands first, then keyword fallback — but in `process_voice_command` at line 938, shutdown in the fallback triggers `exit(0)` instead of system shutdown. System shutdown only works if NLU routes to `system_control` intent. **PARTIALLY BROKEN** |
| "Enable WiFi" / "Disable Bluetooth" | Listed | Works via `commands/system.rs:53-69` — **WORKS** |
| "Search for *.pdf files" | Listed | Works via `commands/files.rs` with extension filter — **WORKS** |
| "Open downloads folder" | Listed | Works via `commands/files.rs:1052-1058` — **WORKS** |
| "Exit" | Listed as returning to wake word | In code, `exit` causes `std::process::exit(0)` — it **kills the entire app**, not returns to wake word. Sleep/standby returns to wake word. **MISLEADING** |

### 5. Hotkey Not Documented
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Hotkey | Not mentioned in Quick Start | Code registers **Ctrl+Shift+Space** global hotkey (`hotkey.rs`). Fully functional on Windows, stubs on Linux/macOS. |
| **Severity** | **LOW** - Missing convenience info | |

### 6. STT Model Priority
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Model | "ggml-base-q8_0.bin \| 81MB" only listed | Code tries `ggml-small-q8_0.bin` FIRST, then `ggml-base-q8_0.bin`, then `ggml-base.bin` (`stt.rs:15-18`). The small model (~50MB) is preferred. |
| **Severity** | **LOW** - Incomplete model info | |

### 7. FastSwap
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Features | "Cross-Platform Sharing, LocalSend v2 protocol, Approval Flow, Popup, Real-Time Progress, Multi-File, Folder Selection, 60s timeout" | Code confirms all of these via `fastswap/` module (Axum server, HTTP client, AES-GCM crypto, discovery, progress tracking, incoming popup UI) — **ACCURATE** |
| **Severity** | **NONE** - Correctly documented | |

### 8. Plugin System
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| "All commands routed through unified plugin system" | Claims fully plugin-based | This is TRUE — `plugins/builtin/` has 12 modules (browsers, utilities, communication, media, office, creative, gaming, editors, camera, files, reminders, system_control). The 5-pass matching system works. **ACCURATE** |
| "App Aliases | Smart recognition (e.g., Chrome → google chrome)" | Also true — `engine.rs:58-114` has extensive alias maps. **ACCURATE** |
| **Severity** | **NONE** | |

### 9. Setup Manager
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Auto-downloads | "Whisper STT (~81MB), Piper TTS + voice (~50MB), SBERT NLU (~80MB), FFmpeg (~100MB)" | Code downloads Whisper, Piper, espeak-ng-data, and FFmpeg. **SBERT is NOT downloaded** — no real SBERT model. Total is ~200MB as stated, but without 80MB SBERT. |
| Platform-specific | Not mentioned | Code has platform-specific download URLs for Windows (zip), Linux (tar), macOS (tar) (`downloader.rs`). |
| **Severity** | **MEDIUM** - SBERT download is claimed but doesn't happen | |

### 10. Admin Privileges
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Admin requirement | Not mentioned | `igrisv3.exe.manifest` requests `requireAdministrator` level. The app requests admin on Windows. |
| **Severity** | **LOW** - Security-relevant omission | |

### 11. Code Warnings
| Aspect | README Claims | Actual Code |
|--------|---------------|-------------|
| Dead code / unused | Not mentioned | `lib.rs:4-6` and `main.rs:2-5` have `#![allow(dead_code)]`, `#![allow(unused_imports)]`, `#![allow(unused_variables)]` — significant amount of dead/unused code. Many features are WIP. |
| **Severity** | **LOW** - Development transparency | |

## Architecture Match

| README Path | Code Path | Match? |
|-------------|-----------|--------|
| `core/stt.rs` | `core/stt.rs` | ✅ |
| `core/tts.rs` | `core/tts.rs` | ✅ |
| `core/vad.rs` | `core/vad.rs` | ✅ |
| `core/wake_word.rs` | `core/wake_word.rs` | ✅ |
| `nlu/engine.rs` | `nlu/engine.rs` | ✅ |
| `nlu/sbert.rs` | `nlu/sbert.rs` | ✅ (but fallback only) |
| `nlu/ner.rs` | `nlu/ner.rs` | ✅ |
| `nlu/context.rs` | `nlu/context.rs` | ✅ |
| `commands/system.rs` | `commands/system.rs` | ✅ |
| `commands/files.rs` | `commands/files.rs` | ✅ |
| `commands/ffmpeg_camera.rs` | `commands/ffmpeg_camera.rs` | ✅ |
| `commands/reminders.rs` | `commands/reminders.rs` | ✅ |
| `commands/about.rs` | `commands/about.rs` | ✅ |
| `plugins/system.rs` | `plugins/system.rs` | ✅ |
| `plugins/builtin/` | `plugins/builtin/` | ✅ (12 modules) |
| `ui/settings.rs` | `ui/settings.rs` | ✅ |
| `ui/camera_panel.rs` | `ui/camera_panel.rs` | ✅ |
| `ui/fastswap_panel.rs` | `ui/fastswap_panel.rs` | ✅ |
| `ui/incoming_transfer_popup.rs` | `ui/incoming_transfer_popup.rs` | ✅ |
| `ui/presentation/` | `ui/presentation/` | ✅ |
| `fastswap/models/` | `fastswap/models/` | ✅ |
| `fastswap/network/` | `fastswap/network/` | ✅ |
| `setup_manager/downloader.rs` | `setup_manager/downloader.rs` | ✅ |
| `setup_manager/permissions.rs` | `setup_manager/permissions.rs` | ✅ |
| `setup_manager/platforms/` | `setup_manager/platforms/` | ✅ |

## Summary of Fixes Made in New README

1. **SBERT** → Clarified as "fallback SBERT (character trigram hashing, not real model)"
2. **Personalities** → Added Alita documentation
3. **Model table** → Added small-q8_0 as preferred model, removed non-existent SBERT model
4. **Hotkey** → Added Ctrl+Shift+Space to Quick Start
5. **Architecture** → Slightly simplified data flow to reflect hash-based NLU
6. **Exit command** → Clarified that exit kills the app, sleep returns to wake word mode
7. **Removed exaggerated claims** about interactive diagrams/flowcharts
