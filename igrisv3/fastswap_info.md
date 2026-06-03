# FastSwap — File Sharing Module

FastSwap is a full LocalSend v2.0 protocol implementation for cross-platform file sharing over LAN. Devices running LocalSend-compatible apps (Android, iOS, Windows, macOS, Linux) discover each other automatically and transfer files directly (peer-to-peer, no cloud relay).

## How It Works

### Architecture (`src/fastswap/`)

```
fastswap/
├── crypto.rs           # AES-256-GCM encrypt/decrypt helpers (key generation, per-chunk encryption)
├── mod.rs              # FastSwapManager, global state (pending transfers, approval, progress tracker)
├── models/
│   ├── device.rs       # Device, DeviceType, RegisterRequest/Response
│   ├── transfer.rs     # FileInfo, TransferState, handshake request/response types
│   └── progress.rs     # FileProgress, TransferProgress, ProgressTracker, formatting helpers
└── network/
    ├── server.rs       # Axum HTTP server (port 53317+), 5 endpoints
    ├── client.rs       # TransferClient — three-way handshake, streaming upload
    └── discovery.rs    # DiscoveryService — subnet scan (254 IPs, 50 concurrent)
```

### Transfer Flow

1. **Device Discovery** — UI calls `DiscoveryService::scan_network()` which probes all 254 IPs in the local subnet at port 53317 via `GET /api/localsend/v2/info`. Devices that respond are shown in a grid.

2. **File Selection** — User picks files or a folder (recursive). Selected files are shown with count and total size. User clicks a device card to send.

3. **Three-Way Handshake** — `TransferClient` (client.rs) executes:
   - `POST /prepare-upload` — sends file metadata, server creates a session + generates a random AES-256-GCM key, creates pending transfer (for user approval), returns the key in the response
   - `POST /confirm-upload` — server polls every 500ms for up to 60s waiting for receiver to accept/deny
   - `POST /upload?sessionId=&fileId=&token=` — streams the file body (64KB chunks) to the receiver

4. **Encryption** — The sender encrypts each 64KB chunk with AES-256-GCM using the key from `prepare-upload`. Each chunk gets a fresh random 12-byte nonce prepended (nonce || ciphertext || 16-byte tag). On the receiver side, each chunk is decrypted before writing to disk.

5. **Receiver Approval** — When `prepare-upload` is received, a `PendingTransfer` is added to global state. The UI polls this state and shows an Accept/Deny dialog. `confirm-upload` blocks until the user decides (or 60s timeout). If accepted, progress tracking is initialized.

6. **File Reception** — The receiver streams the body to a temp file (chunk-by-chunk via `BodyStream`), decrypting each chunk with the session key, then atomically renames to the Downloads folder. Progress is tracked in plaintext bytes.

7. **Progress Tracking** — Both sender and receiver update a global `TransferProgress` object. The UI polls it every 200ms and renders progress bars, speed (MB/s), and ETA.

### Server Lifecycle

FastSwap starts on-demand (voice command "open FastSwap" or menu button click). `ensure_fastswap_running()` in `main.rs` checks the `FASTSWAP_MANAGER` singleton and launches the Axum server if not already running. The server tries ports 53317-53326 if the default is occupied.

## Pros

- **No internet required** — works entirely over LAN, no cloud dependency
- **LocalSend compatible** — interoperable with LocalSend apps on any platform
- **Full duplex** — can send and receive simultaneously
- **Real-time progress** — live progress bars, speed, ETA for both sender and receiver
- **Receiver consent** — transfer doesn't start until user explicitly accepts (with 60s timeout)
- **Streaming receive** — files are written to disk chunk-by-chunk, not buffered in RAM
- **Path traversal protection** — sender-provided filenames are sanitized before save
- **AES-256-GCM encryption** — each chunk encrypted with a per-session random key; no plaintext on the wire
- **On-demand server** — FastSwap only runs when explicitly opened (no background resource use)
- **Port fallback** — auto-tries ports 53317-53326 if the default is taken
- **Cancel support** — active transfers can be cancelled from the UI

## Cons

- **Sender-side cancellation only** — only the sender can cancel; receiver cannot cancel an incoming transfer
- **No pause/resume** — partial transfers lost on cancel; must restart from scratch
- **No transfer history** — completed/failed transfers are not persisted; gone when tracker is cleared
- **Single file per upload HTTP request** — each file is sent as one POST body; very large files (>2GB) may hit client/server limits
- **Key transmitted in plaintext** — the AES key is sent inside the `prepare-upload` JSON response over HTTP; a MitM on the LAN could capture it and decrypt the transfer
- **No auto-accept whitelist** — every transfer must be manually approved; no trusted-device list
- **No folder mirroring** — directory structure is flattened; files from subfolders lose their relative paths
- **No bandwidth throttling** — transfers use full available bandwidth with no speed cap
- **No background transfers** — transfers only visible while FastSwap panel is open; no system tray or notification integration
- **No QR pairing** — relies solely on subnet scanning; no QR code for direct connection
