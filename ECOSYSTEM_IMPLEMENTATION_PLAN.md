# IGRIS Ecosystem — Commercial Implementation Plan

## Vision

A **paid, proprietary, cross-platform device ecosystem** built in Rust — inspired by the *problem space* of KDE Connect but with clean-room architecture, original protocol design, and commercial polish. Core performance and networking in Rust; UI layer adaptable per-platform.

---

## 0. Legal Foundation: Clean-Room Compliance

**KDE Connect is GPL-licensed. You cannot copy, translate, or derive from its source code.** This plan treats KDE Connect as a *feature-reference only*:

| Permitted | Not Permitted |
|---|---|
| Study KDE Connect's behavior and feature set | Read/copy KDE Connect source code |
| Implement the same *features* from scratch | Port/translate C++ code to Rust |
| Design your own protocol and architecture | Replicate KDE Connect's internal architecture |
| Use the same *user-facing concepts* (device list, clipboard sync) | Use KDE Connect's packet format, APIs, or D-Bus interface designs |
| Use independently designed packet structures | Reference KDE Connect plugin class hierarchy |

### Clean Room Procedure

**For each feature, a designated "design spec" person** (who has NEVER seen KDE Connect source) writes a functional spec based on:
- User-facing behavior ("clipboard text from device A appears on device B")
- Platform API documentation (Microsoft docs, Apple docs, freedesktop.org)
- Network protocol design from first principles (what data needs to flow?)

A **separate implementation team** (who also have never seen KDE Connect source) builds from spec only. All external crate dependencies are audited to be MIT/Apache/BSD-licensed.

---

## 0.1 Commercial Strategy

### Licensing Model: Proprietary Closed-Source

| Component | License | Distribution |
|---|---|---|
| `libigris-core` (Rust) | Proprietary — compiled binary only | No source disclosure |
| Desktop UI (Dioxus/Tauri) | Proprietary | Compiled binary |
| Mobile companion | Proprietary | App Store/Play Store binary |
| Protocol specification | Trade secret | No public documentation |

### Monetization Tiers

| Tier | Price | Features | Device Limit |
|---|---|---|---|
| **Free** | $0 | Pairing, file transfer (basic), clipboard sync | 2 devices |
| **Pro** | $9.99 lifetime | Everything: remote input, media control, notification sync, remote commands, all plugins | 10 devices |
| **Family** | $19.99 lifetime | Pro + mobile companion (telephony/SMS bridge) | 25 devices |
| **Enterprise** | $5/dev/year | Family + SSO, audit logging, MDM, priority support | Unlimited |

### Revenue Streams

1. **Direct sales** — Website, Gumroad, Paddle, App Store
2. **Enterprise licensing** — Per-seat annual, self-hosted auth server
3. **Trial → Paid funnel** — 14-day full-feature trial, then feature-locked to Free tier
4. **Mobile companion** — Free with ads or $2.99 ad-free; Pro desktop users get it free

### Anti-Piracy Measures

- **Trial period enforced via local SQLite + timestamp** (with HMAC signing to prevent tampering)
- **License key** validated by Ed25519 signature (offline; no phone-home required)
- **Optional phone-home** for concurrent device count enforcement (enterprise tier)
- **Code obfuscation**: `stringencrypt` for license strings, strip debug symbols
- **Binary hardening**: UPX pack optional, detect debugger/virtualization (optional)

---

## 1. Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                    UI LAYER (per-platform)                       │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │  Dioxus  │  │  Native  │  │  Flutter │  │  Web     │       │
│  │ Desktop  │  │ (Qt/Swift)│  │  (mobile)│  │ (Tauri)  │       │
│  └────┬─────┘  └────┬─────┘  └────┬─────┘  └────┬─────┘       │
│       │             │             │             │               │
├───────┴─────────────┴─────────────┴─────────────┴───────────────┤
│              COMMUNICATION LAYER (Rust)                         │
│  ┌────────────────────────────────────────────────────────────┐ │
│  │  IPC: In-Process API  │  gRPC (tonic)  │  C FFI (cbindgen)│ │
│  └────────────────────────────────────────────────────────────┘ │
├─────────────────────────────────────────────────────────────────┤
│                    CORE LAYER (Rust — Proprietary)               │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────────┐  │
│  │  Networking │  │  Crypto/TLS  │  │  Device Discovery    │  │
│  │  (axum +    │  │  (rustls +   │  │  (subnet scan    │  │
│  │   reqwest)  │  │   rcgen)     │  │   HTTP/TLS)      │  │
│  ├─────────────┤  ├──────────────┤  ├──────────────────────┤  │
│  │  File       │  │  Clipboard   │  │  Notification Sync  │  │
│  │  Transfer   │  │  Sync        │  │  (platform APIs)    │  │
│  ├─────────────┤  ├──────────────┤  ├──────────────────────┤  │
│  │  Remote     │  │  Media       │  │  Telephony / SMS    │  │
│  │  Input      │  │  Control     │  │  (mobile bridge)    │  │
│  ├─────────────┤  ├──────────────┤  ├──────────────────────┤  │
│  │  Plugin     │  │  Device DB   │  │  Licensing / Auth   │  │
│  │  Loader     │  │  (SQLite)    │  │  (Ed25519 keys)     │  │
│  └─────────────┘  └──────────────┘  └──────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│                 PLATFORM ABSTRACTION LAYER (Rust)               │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐       │
│  │ Windows  │  │  macOS   │  │  Linux   │  │  Mobile  │       │
│  │ (windows │  │ (objc2 / │  │ (zbus /  │  │ (jni /   │       │
│  │  crate)  │  │ core-found)│  │  dbus)   │  │  kotlin) │       │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘       │
└─────────────────────────────────────────────────────────────────┘
```

### Key Design Principle

**Rust Core = single shared library (`libigris_core`)** compiled as `cdylib` + `lib`.

All networking, crypto, device mgmt, protocol, licensing, and platform abstraction compiles into one Rust binary. This can be consumed by:

- **Dioxus Desktop** (in-process, zero IPC overhead)
- **Tauri v2** (IPC via Tauri commands)
- **Flutter** (via `flutter_rust_bridge` v2)
- **Native C++/Qt/Swift** (via C ABI with `cbindgen`)

**License validation lives in the core** — the UI never sees the license key or validation logic.

---

## 2. Layer Breakdown

### 2.1 Core Layer (`libigris_core` — Rust `cdylib`)

#### What already exists (can be reused/refactored):

| Module | Current Location | License Audit |
|---|---|---|
| **Eco networking** | `src/eco/` — original Rust design | ✅ Clean |
| **FastSwap file transfer** | `src/fastswap/` — original Rust design | ✅ Clean (LocalSend protocol compatibility is fine — it's a published spec, not KDE code) |
| **Platform clipboard** | `src/platform/ecosystem/` | ✅ Clean (uses OS CLI tools) |
| **Platform notifications** | `src/platform/ecosystem/notifications/` | ✅ Clean |
| **Pairing** | `src/eco/pairing.rs` | ✅ Clean |
| **Crypto/TLS** | `src/eco/crypto.rs` + `src/fastswap/tls.rs` | ✅ Clean |
| **Device model** | `src/eco/device.rs` | ✅ Clean |
| **Events** | `src/eco/events.rs` | ✅ Clean |
| **Storage** | `src/eco/storage.rs` | ✅ Clean (JSON file storage) |
| **Config** | `src/eco/config.rs` | ✅ Clean |

#### What needs to be built new:

| Module | Description | Priority | Clean-Room Note |
|---|---|---|---|
| **EcoPlugin trait + registry** | Plugin trait for implementing features | P0 | Original Rust design |
| **Remote Input** | Mouse/keyboard/touchpad forwarding | P0 | *Problem*: capture OS input events, send as packets, replay on remote. *Implementation*: OS-specific input APIs, original packet format |
| **Media Player Control** | MPRIS/WinRT playback control | P0 | *Problem*: control media players remotely. *Implementation*: platform media APIs directly |
| **Notification Sync** | Proper WinRT/macOS/D-Bus listeners | P0 | *Problem*: read notifications on one device, show on another. *Implementation*: platform notification APIs directly |
| **Battery/Device Info** | Battery level reporting | P1 | Platform power APIs |
| **System Volume Control** | Cross-platform audio volume | P1 | Platform audio APIs |
| **Screensaver Inhibit** | Prevent sleep during sessions | P1 | Platform power management APIs |
| **Remote Commands** | Execute predefined commands on peer | P1 | Configurable command list sent as messages |
| **License Manager** | Offline license validation (Ed25519) | P1 | Original — signing + verification |
| **Telephony Bridge** | Call state forwarding (mobile companion) | P2 | Requires companion app |
| **SMS Bridge** | SMS send/read via mobile | P2 | Requires companion app |
| **Find My Device** | Ring/alert remote device | P2 | Send command, play sound |
| **Presenter** | Presentation remote control | P2 | Keyboard shortcut simulation |
| **Contacts** | Shared contact list | P2 | Via mobile companion |
| **Virtual Monitor** | Remote display | P3 | Original — display capture + network streaming |

### 2.2 Communication Layer

Three channels, all go through the **License Gate** which checks if the calling feature is paid:

```
UI Request ─→ License Gate (allowed?) ─Yes─→ Core API
                              │ No
                              └──→ Error("Upgrade required")
```

#### Option A: In-Process Direct API (Dioxus/Tauri)

```rust
use igris_core::prelude::*;

let core = Core::new(config_path)?;
core.activate_license("XXXX-XXXX-XXXX-XXXX")?; // returns Ok or LicenseError
core.start().await?;
```

#### Option B: gRPC API (cross-process UIs)

```protobuf
service EcoService {
  rpc GetDevices(Empty) returns (DeviceList);
  rpc ActivateLicense(LicenseKey) returns (LicenseStatus);
  rpc PairDevice(PairRequest) returns (PairResponse);
  rpc SendFile(SendFileRequest) returns (TransferSession);
  rpc TransferProgress(ProgressRequest) returns (stream ProgressUpdate);
  rpc SendClipboard(ClipboardPayload) returns (Empty);
  rpc RemoteInput(InputEvent) returns (Empty);
  rpc MediaCommand(MediaCommand) returns (Empty);
  rpc ExecuteRemoteCommand(RemoteCommandRequest) returns (CommandResult);
  rpc FindDevice(DeviceId) returns (Empty);
  rpc SubscribeEvents(Empty) returns (stream EcoEvent);
}
```

#### Option C: C ABI FFI (native UIs)

```c
EcoHandle* eco_create(const char* config_dir);
void eco_destroy(EcoHandle*);
int eco_activate_license(EcoHandle*, const char* key); // 0 = OK, -1 = invalid, -2 = expired
int eco_start(EcoHandle*);
```

### 2.3 UI Layer

**Primary: Dioxus Desktop (Phase 0-1) → Tauri v2 (Phase 2+)**

Tauri v2 recommended long-term for:
- Binary size (2-15MB vs Dioxus WebView bundle)
- Native system tray, notifications, menus
- Mobile targets (iOS/Android)
- Security sandboxing
- App Store readiness (DMG/MSI/AppImage packaging)

**UI component tree:**
```
App
├── DevicePanel (device list, pairing, status icons)
├── TransferPanel (file send/receive, progress, history)
├── ClipboardPanel (sync toggle, history viewer)
├── NotificationCenter (local + remote notification list)
├── RemoteInputPanel (keyboard/mouse/touchpad controls)
├── MediaControlPanel (now playing, play/pause/next/vol)
├── SettingsPanel
│   ├── General (auto-start, discovery, port config)
│   ├── License (activate, status, upgrade)
│   ├── Plugins (enable/disable per plugin)
│   └── About (version, EULA)
└── SystemTray (minimize-to-tray, quick actions)
```

---

## 3. Crates & Dependencies — License Audit

### Allowed Licenses (commercially safe)

| License | Can Use? | Notes |
|---|---|---|
| MIT | ✅ Yes | Unrestricted |
| Apache-2.0 | ✅ Yes | Unrestricted |
| BSD-2/3-Clause | ✅ Yes | Unrestricted |
| Unlicense / CC0 | ✅ Yes | Public domain |
| MPL-2.0 | ✅ Yes | File-level copyleft OK for proprietary |
| ISC | ✅ Yes | Unrestricted |
| **GPL-2/3** | **❌ No** | **Strong copyleft — would force GPL on entire product** |
| **LGPL-3.0** | **❌ No** | **Would force GPL on library if modified** |
| **AGPL-3.0** | **❌ No** | **Network use = distribution = source disclosure** |

### Existing Dependency Audit

| Current Crate | License | Status |
|---|---|---|
| `dioxus` / `dioxus-desktop` | MIT/Apache-2.0 | ✅ Safe |
| `sherpa-onnx` | Apache-2.0 | ✅ Safe |
| `tokio` | MIT | ✅ Safe |
| `axum` | MIT | ✅ Safe |
| `reqwest` | MIT/Apache-2.0 | ✅ Safe |
| `rustls` | MIT/Apache-2.0/ISC | ✅ Safe |
| `serde` / `serde_json` | MIT/Apache-2.0 | ✅ Safe |
| `tonic` | MIT | ✅ Safe |
| `prost` | Apache-2.0 | ✅ Safe |
| `tower` / `tower-http` | MIT | ✅ Safe |
| `windows` crate | MIT/Apache-2.0 | ✅ Safe |
| `objc2` | MIT | ✅ Safe |
| `zbus` | MIT | ✅ Safe |
| `candle` | MIT/Apache-2.0 | ✅ Safe |
| `tracing` | MIT | ✅ Safe |

### **Explicitly Avoid:**
| Crate | License | Why |
|---|---|---|
| `notify-rust` | **GPL-3.0** | **BLOCKED** — use platform-native instead |
| Any `qt` binding | LGPL/GPL | Complex licensing; just use FFI if needed |
| `libpulse-binding` | **GPL-2.0** | **BLOCKED** — use `zbus` PulseAudio interface instead |
| `libxdo-sys` | **GPL-2.0** | **BLOCKED** — use `xdotool` CLI (MIT) or implement XTest yourself |

### New Crates Required (License-Cleared)

| Crate | License | Phase | Purpose |
|---|---|---|---|
| `zbus` | MIT | P0 | Linux D-Bus (notifications, MPRIS, UPower) |
| `mpris` | MIT | P0 | Linux MPRIS client |
| `windows` (expand features) | MIT/Apache-2.0 | P0 | WinRT notifications, media, power |
| `objc2` + `objc2-foundation` + `objc2-app-kit` | MIT | P0 | macOS native APIs |
| `mac-usernotifications` | Apache-2.0/MIT | P0 | macOS notification reading |
| `rusqlite` | MIT | P0 | Persistent device DB |
| `bincode` | MIT | P0 | Binary payload encoding |
| `ed25519-dalek` | BSD-3-Clause | P0 | License key signing |
| `base64` | MIT/Apache-2.0 | P0 | License key encoding |
| `hex` | MIT/Apache-2.0 | P0 | Hex encoding |
| `core-graphics` | MIT/Apache-2.0 | P1 | macOS remote input (CGEvent) |
| `core-foundation` | MIT/Apache-2.0 | P1 | macOS system APIs |
| `tonic` | MIT | P2 | gRPC server (optional, for cross-process) |
| `prost` | Apache-2.0 | P2 | Protobuf generation (optional) |

### Dependency audit procedure:
- Run `cargo deny` in CI to block GPL/AGPL/LGPL-3 crates
- Maintain a `deny.toml` with the allowed license list
- Review every new crate's full license tree (`cargo license`)

---

## 4. Feature Mapping: KDE Connect → IGRIS Ecosystem

The left column lists a feature *category* that KDE Connect also happens to implement. The right column is our independently designed implementation approach.

### P0 — Launch Features (MVP) — Free Tier (2 devices)

| Feature | Our Approach (Independent Implementation) |
|---|---|
| **Device discovery** | Original Rust: IPv4 subnet scan probing HTTP 53327 / TLS 53328, `GET /api/ecosystem/v1/info` |
| **Pairing/trust** | Original Rust: OTP 6-digit + Ed25519 key exchange. QR code optional. |
| **File transfer** | Original Rust: FastSwap (existing). REST API + HTTPS TLS, LocalSend v2.0 compatible protocol (published open spec). **LocalSend protocol is MIT-licensed, used by many apps** |
| **Clipboard sync** | Original Rust (existing). SHA-256 dedup, SHA-256 hashed polling, JSON payloads. |
| **Notification sync** | Platform APIs directly: WinRT (`windows` crate) on Windows, `UNUserNotificationCenter` (`mac-usernotifications`) on macOS, `org.freedesktop.Notifications` via `zbus` on Linux. |
| **Remote keyboard** | Original Rust: capture via platform input APIs (`SendInput` on Windows, `CGEvent` on macOS, `xdotool`/`libei` on Linux), packetize in our format, replay on remote |
| **Remote mouse/touchpad** | Same as keyboard — independent capture + replay design |
| **Ping/Find** | JSON message over HTTP: `{ "type": "FindRequest", "device_id": "..." }` — remote plays sound |

### P1 — Pro Tier Features ($9.99)

| Feature | Our Approach |
|---|---|
| **Media player control** | Linux: MPRIS via `mpris` crate (D-Bus spec, not KDE-specific). Windows: `Windows.Media.Control` WinRT. macOS: `MPNowPlayingInfoCenter` / ScriptingBridge |
| **System volume** | Linux: PulseAudio via `zbus` (PulseAudio D-Bus interface). Windows: `IAudioEndpointVolume` via `windows` crate. macOS: `CoreAudio` |
| **Battery reporting** | Linux: `UPower` via `zbus`. Windows: WinRT `Battery` API. macOS: `IOKit` `IOPowerSources` |
| **Screensaver inhibit** | Linux: `org.freedesktop.ScreenSaver` D-Bus. Windows: `SetThreadExecutionState`. macOS: `IOPMAssertionCreate` |
| **Remote commands** | Original: configurable JSON list of commands with names, sent to peer, executed via `std::process::Command` |
| **Transfer history** | SQLite persistent storage — original schema |
| **System tray** | Tauri v2 tray API or custom Win32/NSStatusBar/Libappindicator |

### P2 — Family Tier Features ($19.99)

| Feature | Our Approach |
|---|---|
| **Telephony bridge** | Requires mobile companion (Flutter) — bridges Android `TelephonyManager` / iOS `CallKit` to desktop via our protocol |
| **SMS bridge** | Mobile companion: Android `SmsManager` / iOS `MFMessageComposeViewController` |
| **Presenter remote** | Arrow key/W control simulation via platform input APIs |
| **Find this device** | GPS location from mobile companion; map display on desktop |
| **Lock device** | Remote lock via platform lock APIs (existing in code) |
| **Clipboard image sync** | Extended clipboard: check for image data, encode as PNG/base64, transmit via bincode |

### P3 — Enterprise Tier ($5/dev/year)

| Feature | Our Approach |
|---|---|
| **Virtual monitor** | Desktop capture (`DXGI` Windows, `CGDisplayStream` macOS, `PipeWire` Linux) → H.264 encoding → network stream |
| **SSO / LDAP** | Enterprise auth backend integration |
| **Audit logging** | Structured event log per device pair; export to SIEM |
| **MDM enrollment** | Configuration profile push (JAMF, Intune) |
| **Priority support** | Email/Slack-based SLA |

---

## 5. Protocol Design (Original)

### 5.1 Device Discovery — Own Design

```
Probe: GET /api/ecosystem/v1/info over HTTP (53327) / TLS (53328)
Response: { device_id, name, version, platform, caps }

Subnet scan (IPv4 /24, up to 50 concurrent) every 30s; peers time out after 120s
```

### 5.2 Core Protocol — Own Design

```rust
/// Protocol envelope — original design
pub struct EcoMessage {
    pub version: String,         // "1.0.0"
    pub msg_type: MessageType,
    pub sender_id: String,       // UUID v4
    pub timestamp: i64,          // unix millis
    pub payload: Vec<u8>,        // bincode-serialized
    pub hmac: [u8; 32],          // HMAC-SHA256 over payload + timestamp + sender_id
}
```

**Not KDE Connect's packet format.** KDE uses JSON with fields like `"type"`, `"body"`, `"id"`. Our layout is different: typed enum + bincode binary + HMAC-signed instead of RSA-signed.

### 5.3 Encryption — Own Design

- **Transport**: TLS 1.3 with self-signed certs (rcgen) — cert fingerprint exchanged during pairing
- **Payload**: HMAC-SHA256 for integrity + pairing-based shared secret
- **No RSA** (KDE uses RSA); we use Ed25519 for key exchange and HMAC for payload auth

### 5.4 API Endpoints — Own Design

```
POST /api/eco/v1/message           # Original endpoint naming
GET  /api/eco/v1/info
POST /api/eco/v1/pair/request
POST /api/eco/v1/pair/verify
POST /api/eco/v1/pair/untrust

# FastSwap file transfer (existing, not derived from KDE)
GET  /api/localsend/v2/info         # LocalSend protocol (MIT spec)
POST /api/localsend/v2/prepare-upload
POST /api/localsend/v2/confirm-upload
POST /api/localsend/v2/upload

# gRPC service (original protobuf definition)
eco.EcoService/GetDevices
eco.EcoService/SubscribeEvents (server-streaming)
eco.EcoService/SendFile
eco.EcoService/RemoteInput
eco.EcoService/MediaCommand
...
```

---

## 6. Licensing System — Design

### Offline License Validation

```
┌──────────────┐    License Key (ed25519 signed)
│  Customer    │──→ "IGRIS-PRO-{base64(user_id + tier + expiry)}-{base64(signature)}"
└──────────────┘
     │
     ▼
┌──────────────────────────────────────┐
│  Core Library                        │
│  ┌──────────────────────────────────┐│
│  │  LicenseManager                  ││
│  │  - verify_ed25519_signature()    ││
│  │  - check_expiry()                ││
│  │  - enforce_device_limit()        ││
│  │  - return LicenseLevel {Free,   ││
│  │    Pro, Family, Enterprise}      ││
│  └──────────────────────────────────┘│
│  ┌──────────────────────────────────┐│
│  │  FeatureGate                     ││
│  │  - require_level(Feature) → bool ││
│  │  - Placed at EVERY public API    ││
│  └──────────────────────────────────┘│
└──────────────────────────────────────┘
```

### Feature Gating Matrix

| Feature | Free | Pro | Family | Enterprise |
|---|---|---|---|---|
| Device pairing | ✅ (max 2) | ✅ (max 10) | ✅ (max 25) | ✅ (unlimited) |
| File transfer | ✅ | ✅ | ✅ | ✅ |
| Clipboard sync | ✅ | ✅ | ✅ | ✅ |
| Notification sync | ❌ | ✅ | ✅ | ✅ |
| Remote input | ❌ | ✅ | ✅ | ✅ |
| Media control | ❌ | ✅ | ✅ | ✅ |
| Battery/Volume | ❌ | ✅ | ✅ | ✅ |
| Remote commands | ❌ | ✅ | ✅ | ✅ |
| Mobile companion | ❌ | ❌ | ✅ | ✅ |
| Telephony/SMS | ❌ | ❌ | ✅ | ✅ |
| Virtual monitor | ❌ | ❌ | ❌ | ✅ |
| Enterprise SSO | ❌ | ❌ | ❌ | ✅ |
| Audit logging | ❌ | ❌ | ❌ | ✅ |

### License Key Format

```
Format: IGRIS-{TIER}-{BASE64_USER}-{BASE64_EXPIRY}-{BASE64_SIG}

Example:
IGRIS-PRO-VXNlclVJRD0xMjM0-AyMDI2LTEyLTMx-4sV8C3...

Where:
- TIER = FREE | PRO | FAMILY | ENTERPRISE
- USER = base64( user_id )
- EXPIRY = base64( ISO8601 date or "never" )
- SIG = base64( ed25519::Signature over tier + user + expiry )
```

### Trial Implementation

- First launch: generate random device UUID, store in SQLite
- Trial ends after 14 days or 10 app-starts, whichever comes first
- On trial expiry: degrade to Free tier features (2 devices, file+clipboard only)
- "Activate License" button in Settings panel; validates offline instantly
- No phone-home required for personal tiers; Enterprise can opt-in for concurrent-count enforcement

---

## 7. Implementation Phases

### Phase 0: Foundation + Licensing (4-6 weeks)

**Goal**: Workspace refactor, license system, core networking.

```
Steps:
1. Create workspace crates:
   igrisv4/
   ├── Cargo.toml              # workspace root (proprietary, no public publish)
   ├── libigris-core/          # Core cdylib
   │   ├── src/
   │   │   ├── lib.rs
   │   │   ├── license/        # Ed25519 key verification, feature gate
   │   │   ├── network/        # axum HTTP server + reqwest client
   │   │   ├── discovery/      # IPv4 subnet scan
   │   │   ├── crypto/         # TLS, key exchange
   │   │   ├── protocol/       # message types, bincode ser/de
   │   │   ├── pairing/        # OTP, trust mgmt
   │   │   ├── storage/        # SQLite (devices, transfers, license)
   │   │   ├── device/         # device model + manager
   │   │   ├── clipboard/      # clipboard sync logic
   │   │   ├── notification/   # notification sync (platform traits)
   │   │   ├── transfer/       # FastSwap moved here
   │   │   ├── remote_input/   # capture traits (impl in P1)
   │   │   ├── media/          # media control traits (impl in P1)
   │   │   ├── commands/       # remote command executor
   │   │   ├── platform/       # OS abstraction traits
   │   │   └── ffi/            # C ABI exports
   │   └── Cargo.toml
   ├── eco-ui/                 # Dioxus Desktop UI
   │   └── Cargo.toml
   └── igrisv4/                # legacy voice assistant (unmodified)

2. Move eco/ + fastswap/ + platform/ecosystem/ into libigris-core
3. Implement license module (Ed25519 key gen, sign, verify)
4. Implement FeatureGate trait at every public API
5. Implement subnet discovery scan (IPv4 /24, HTTP/TLS probes)
6. Add SQLite storage (rusqlite)
7. Wire license check into start() — refuse if trial expired
```

### Phase 1: MVP Desktop App (6-8 weeks) — Free Tier Launch

**Goal**: Ship a working desktop app with 2-device free tier.

```
Steps:
1. Build Dioxus Desktop UI shell
2. Device list panel with pairing flow (OTP display/enter)
3. File transfer (send button, progress bars, receive popup)
4. Clipboard sync toggle + history viewer
5. Notification sync (read remote notifications, reply inline)
6. Remote keyboard + mouse input forwarding
7. Ping/find device command (play sound on remote)
8. System tray with connection status
9. Trial/license management in Settings panel
10. Windows MSI + macOS DMG packaging (no GPL bundling)
```

### Phase 2: Pro Features (6-8 weeks) — Pro Tier Launch ($9.99)

```
Steps:
1. Media player control (play/pause/next/prev/volume seek)
2. System volume sync between devices
3. Battery level display (local + remote)
4. Screensaver inhibit during active sessions
5. Remote commands (10 predefined, send + execute)
6. Transfer history in SQLite
7. SQLCipher encryption for local storage (enterprise feature)
8. Feature gate everything behind Pro license check
```

### Phase 3: Family + Mobile Companion (8-10 weeks) — Family Tier ($19.99)

```
Steps:
1. Flutter mobile companion app (Android first, iOS second)
2. Rust core cross-compiled: cargo-ndk (Android), cargo-lipo/cargo-xcframework (iOS)
3. Telephony state bridge (call notifications on desktop)
4. SMS send/read via mobile bridge
5. Presenter remote (PowerPoint/Keynote via keyboard simulation)
6. Contacts sync (read-only from mobile)
7. Mobile find-my-device (ring phone from desktop)
8. Pause music when phone call is active
```

### Phase 4: Enterprise + Polish (ongoing)

```
Steps:
1. Virtual monitor (DXGI/CGDisplayStream/PipeWire → H.264)
2. Enterprise license server (optional phone-home, concurrent count)
3. SSO integration (LDAP, SAML, OIDC)
4. Audit logging to file/SIEM
5. Performance optimization (connection multiplexing, streaming)
6. UI polish: animations, drag-drop file send, dark/light theme
7. Store presence: Microsoft Store, Mac App Store, Flathub
```

---

## 8. Platform-Specific Considerations

### 8.1 Windows

| Feature | API | License Safe? |
|---|---|---|
| Clipboard | PowerShell (existing) or WinRT via `windows` crate | ✅ MIT |
| Notifications | `windows` crate → `Windows.UI.Notifications` | ✅ MIT |
| Remote input | `SendInput` via `windows` crate | ✅ MIT |
| Media control | `Windows.Media.Control` WinRT | ✅ MIT |
| Volume | `IAudioEndpointVolume` via `windows` crate | ✅ MIT |
| Battery | WinRT `Battery` API | ✅ MIT |
| Screensaver | `SetThreadExecutionState` via `windows` crate | ✅ MIT |
| System tray | Win32 via `windows` crate or Tauri tray | ✅ MIT |
| Lock screen | `LockWorkStation` via `windows` crate | ✅ MIT |
| File dialogs | `rfd` crate (MIT) | ✅ MIT |

### 8.2 macOS

| Feature | API | License Safe? |
|---|---|---|
| Clipboard | `pbpaste`/`pbcopy` (existing) | ✅ No license |
| Notifications | `mac-usernotifications` crate | ✅ Apache-2.0/MIT |
| Remote input | `CGEvent` via `core-graphics` crate | ✅ MIT |
| Media control | `MediaPlayer` framework via `objc2` | ✅ MIT |
| Volume | `CoreAudio` via `core-foundation` + `objc2` | ✅ MIT |
| Battery | `IOKit` via `core-foundation` | ✅ MIT |
| Screensaver | `IOPMAssertionCreate` via `IOKit` | ✅ MIT |
| System tray | `NSStatusBar` via `objc2-app-kit` | ✅ MIT |
| Lock screen | `SACLockScreenImmediate` via Security framework | ✅ MIT |
| Permissions | `TCC` — Accessibility + Notifications + Screen Recording | ✅ System API |

### 8.3 Linux

| Feature | API | License Safe? |
|---|---|---|
| Clipboard | `xclip`/`wl-clipboard` (existing) | ✅ Unrestricted |
| Notifications | `zbus` → `org.freedesktop.Notifications` | ✅ MIT |
| Remote input (X11) | `xdotool` CLI (MIT) or `libX11` via `x11rb` crate (MIT) | ✅ MIT |
| Remote input (Wayland) | `libei` client (MIT) or `uinput` via `/dev/uinput` | ✅ MIT |
| Media control | `mpris` crate → D-Bus | ✅ MIT |
| Volume | PulseAudio D-Bus interface via `zbus` (NOT `libpulse-binding` which is GPL) | ✅ MIT |
| Battery | `UPower` D-Bus via `zbus` | ✅ MIT |
| Screensaver | `org.freedesktop.ScreenSaver` via `zbus` | ✅ MIT |
| System tray | Tauri tray icon or `libappindicator` (LGPL-2.1 — *review needed*) | ⚠️ LGPL OK if dynamically linked |
| Lock screen | `loginctl lock-session` via `zbus` → `org.freedesktop.login1` | ✅ MIT |

### 8.4 Mobile (Phase 3 — Android/iOS)

| Platform | UI | Core Bridge | License |
|---|---|---|---|
| Android | Flutter | `flutter_rust_bridge` → `libigris_core.so` via JNI | Flutter: BSD-3, Bridge: MIT |
| iOS | Flutter / SwiftUI | `flutter_rust_bridge` → `libigris_core.a` via C FFI | Same |

---

## 9. Distribution & Packaging

### Desktop

| Platform | Format | Store | Signing |
|---|---|---|---|
| Windows | MSI (Wix/NSIS) | Microsoft Store optional | Authenticode cert |
| macOS | DMG + notarized .app | Mac App Store optional | Apple Developer cert |
| Linux | AppImage / .deb / .rpm | Flathub optional | GPG sign |

**Anti-tamper**: Binaries strip debug symbols. License validation is HMAC-signed SQLite entry + Ed25519 key check. No environment variable overrides.

### Mobile

| Platform | Format | Store | Distribution |
|---|---|---|---|
| Android | APK/AAB | Google Play | Managed Play (Enterprise) or public |
| iOS | IPA | App Store | Enterprise cert (MDM) or public |

### Trial Enforcement

- First-run writes `trial_start` + `launch_count` to SQLite
- HMAC-SHA256 over the values with a hardcoded key (obfuscated) to prevent tampering
- Trial ends at 14 days OR 10 launches
- After expiry: FeatureGate returns `Free` level (2 devices, basic features only)
- "Activate License" input field + button activates permanent key

---

## 10. Summary: Complete Dependency Table

```toml
# libigris-core/Cargo.toml

[package]
name = "igris-core"
version = "0.1.0"
edition = "2021"
license = "Proprietary"

[lib]
crate-type = ["lib", "cdylib"]

[dependencies]
# Async
tokio = { version = "1", features = ["full"] }
async-io = "2"

# HTTP
axum = "0.7"
reqwest = { version = "0.11", features = ["stream", "json"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["cors", "add-extension"] }
bytes = "1.5"
hyper-util = { version = "0.1", features = ["tokio"] }
tokio-util = { version = "0.7", features = ["io"] }
tokio-stream = { version = "0.1", features = ["sync"] }

# Discovery
local-ip-address = "0.6"

# TLS
rustls = { version = "0.23", features = ["ring"] }
tokio-rustls = "0.26"
rcgen = "0.13"

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
bincode = "2"

# Identifiers
uuid = { version = "1", features = ["v4", "serde"] }
whoami = "1.5"

# Storage
rusqlite = { version = "0.32", features = ["bundled"] }
chrono = { version = "0.4", features = ["serde"] }

# Crypto
sha2 = "0.10"
ed25519-dalek = "2"
hmac = "0.12"
base64 = "0.22"
hex = "0.4"

# Error
anyhow = "1"
thiserror = "2"

# Logging
tracing = "0.1"
tracing-subscriber = "0.3"

# gRPC (optional)
tonic = { version = "0.12", optional = true }
prost = { version = "0.13", optional = true }

# Linux
[target.'cfg(target_os = "linux")'.dependencies]
zbus = { version = "5", features = ["tokio"] }
mpris = "2"

# Windows
[target.'cfg(target_os = "windows")'.dependencies]
windows = { version = "0.60", features = [
    "Win32_Foundation",
    "Win32_UI_Notifications",
    "Win32_Media",
    "Win32_System_Power",
    "Win32_System_Com",
    "Win32_System_Threading",
] }

# macOS
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = "0.6"
objc2-app-kit = "0.6"
mac-usernotifications = "1"
core-foundation = "0.10"
core-graphics = "0.24"

[features]
default = []
grpc = ["tonic", "prost"]
```

```toml
# eco-ui/Cargo.toml
[package]
name = "eco-ui"
version = "0.1.0"
edition = "2021"
license = "Proprietary"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon", "dialog", "notification"] }
tauri-plugin-shell = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
igris-core = { path = "../libigris-core" }
```

---

## 11. Key Architectural Decisions

| Decision | Choice | Rationale |
|---|---|---|
| **Core format** | `cdylib` + `lib` | Consumable from any language; linkable from Rust |
| **Protocol** | bincode binary (payload) + JSON (metadata) | Compact, fast: bincode for bulk; JSON for debug |
| **Transport** | JSON over HTTP (existing) | Simplest cross-platform approach |
| **Discovery** | IPv4 subnet HTTP scan | Zero-config on LAN |
| **Storage** | SQLite (rusqlite) | Embedded, encrypted via SQLCipher for enterprise |
| **Crypto** | TLS 1.3 + HMAC-SHA256 | Defense in depth; no RSA patents |
| **Licensing** | Ed25519 offline | No phone-home for privacy; offline activation |
| **UI (P0-1)** | Dioxus Desktop | Already in project, fastest MVP |
| **UI (P2+)** | Tauri v2 | Smaller binary, mobile targets, app store ready |
| **Mobile** | Flutter + flutter_rust_bridge | Best cross-platform mobile story for Rust FFI |
| **Anti-piracy** | HMAC-signed SQLite + Ed25519 keys | Tamper-evident, no network required |
| **Dependency policy** | MIT/Apache/BSD only | No GPL/AGPL/LGPL-3; enforced by `cargo-deny` |
| **Clean room** | Design spec writer ≠ implementer | Legal firewall against GPL contamination |

---

## 12. Risk Mitigation

| Risk | Mitigation |
|---|---|
| **GPL contamination** | Clean-room procedure + `cargo-deny` CI blocking GPL crates |
| **Clean-room legal challenge** | Maintain written design specs + independent implementation audit trail |
| **Protocol reverse-engineering** | Protocol is original design, not KDE's; but even if similar, APIs are not copyrightable (per Google v. Oracle) |
| **Piracy / key sharing** | Ed25519 offline validation + per-device lock (1 key = 3 device activations) |
| **Enterprise adoption friction** | Offer 30-day enterprise trial with phone-home count; no data collection by default |
| **Competition from free tools** | Focus on polish + cross-platform + privacy-first (no cloud needed) |
| **App Store rejection** | No GPL dependencies; no functionality hidden behind IAP on mobile (separate companion) |

---

*End of plan. Total proprietary Rust codebase, zero GPL dependencies, clean-room designed, commercially packaged.*
