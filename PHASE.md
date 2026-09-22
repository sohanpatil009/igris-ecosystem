# IGRIS Ecosystem — PHASE.md (Master Prompt Phase 1-12 Tracker)

Purpose: single tracker mapping Master Prompt Phases 1-12 to real repo state.
Stack: Rust + Dioxus 0.7 desktop (`igrisecosystem` binary + `igris-core` crate).
Ports (central registry `crates/igris-core/src/config.rs`, live impl `src/eco/constants.rs`):
`53317` FastSwap HTTP / `53318` FastSwap TLS / `53327` Eco HTTP / `53328` Eco TLS.
Knowledge graph: 2898 nodes / 6736 edges / 115 communities (ref `graphify-out/GRAPH_REPORT.md`, built from commit `e955c645`).
Refs: `graphify-out/GRAPH_REPORT.md`, `ECOSYSTEM_IMPLEMENTATION_PLAN.md`, `README.md`, `description.md`.
Rules: deny-by-default everywhere; no fake builds; stubs carry TODOs; never bind ports from trackers/tests.

## 0. Status Dashboard

| Phase | Name | Status | Evidence |
|---|---|---|---|
| P1 | Foundation | done | `crates/igris-core` 7 modules + smoke; `cargo test -p igris-core` 16+8 green |
| P2 | Models | in-progress | `ModelPaths` paths verified on disk except laya (manual-config); see §2 |
| P3 | Voice | in-progress | `src/voice.rs`, `src/core/stt.rs` SenseVoice live; `cargo check` green |
| P4 | Reflex | todo | concept only; `EventKind::ReflexDecision` stub in `igris-core` |
| P5 | Brain | todo | `src/nlu/sbert.rs` + `engine.rs` + `bake_nlu.py` exist, not wired to core |
| P6 | Tasks | todo | `execute_task_plan` live in app, not behind `CapabilityToken` gate |
| P7 | Permissions | partial (UI done) | `permission_dialog.rs` + `PermissionsConfig` + `EcoPermissions`; enforce gate TODO |
| P8 | Tools/MCP | todo | `tools/registry.rs` + `src/mcp/server.rs` `handle_request`/`tools/list` live, conformance TODO |
| P9 | Memory | todo | `ContextMemory`, `SharedMemory`, `EcoStorage` live; retention TODO |
| P10 | Device Mesh | partial (trust-gates fixed) | `deny_untrusted` + `is_trusted_sync` on 6 routes; panels live |
| P11 | Agents | todo | `route_llm_tool`, `PluginManager` live; agent spawn/cancel via core TODO |
| P12 | SENTINEL | partial (status done) | `sentinel_status.rs` strip live; full audit TODO |

Build badge: `igris-core` 16 unit + 8 smoke green (2026-09-22, ran locally). `cargo check` (workspace) green, 6 warnings.

## 1. Phase 1 Foundation (done)

Files: `crates/igris-core/src/{lib,error,config,types,events,runtime,logging}.rs` + `crates/igris-core/tests/smoke.rs`.
Concepts: central port/path registry (`EcoConfig`, `ModelPaths`); `ErrorKind` + `E_*` codes (Sec30 fields);
`RiskLevel` SAFE<LOW<MEDIUM<HIGH<CRITICAL + `CapabilityToken` deny-by-default (scope/expiry/task/device/tool/revoke);
`EventKind` 8 variants + in-process `EventBus`; `CancellationNode` UI->Task->Agent->Tool->Device + `IgrisRuntime`;
secret-redacting logging. Tests never bind sockets or touch `pkg/device_id`, `pkg/ecosystem/*.json`, certs.
Build: `cargo test -p igris-core` → 16 passed + smoke 8 passed (verified 2026-09-22).

## 2. Phase 2 Models (in-progress)

Registry: `ModelPaths` (`crates/igris-core/src/config.rs:124`) — `base_dir pkg/models`, whisper guard
(`whisper_legacy_model` must not alias `stt_model`), relative-path-only validation.

| Slot | Config | On disk (`pkg/models/`, verified 2026-09-22) | Status |
|---|---|---|---|
| sense-voice (sherpa-onnx) | `stt_model` `sense-voice/model.onnx`, `stt_tokens` `sense-voice/tokens.txt` | `model.onnx` ~938 MB + `tokens.txt` ~316 KB | ready |
| piper TTS | `piper_exe` `pkg/piper/piper.exe`, `tts_model` `bold_voice/en_US-libritts_r-medium.onnx` | `piper.exe` + `espeak-ng-data/` + voice `.onnx` 78 MB + `.onnx.json` | ready |
| sbert (ort) | `pkg/models/sbert` | `config.json` (MiniLM-L6-H384-uncased, 384-dim, 6-layer) + `model.onnx` 91 MB + `pytorch_model.bin` + `tokenizer.json` | ready |
| qwen2.5-1.5b | `qwen_path` `pkg/models/qwen` (manual-config: no filename in registry) | `pkg/models/qwen2.5-1.5b-instruct-q4_k_m.gguf` ~1.1 GB at models root, no `qwen/` dir | path-mismatch TODO |
| laya | `laya_dir` `pkg/models/laya` | absent on disk | unverified slot (manual-config) |
| whisper | `whisper_legacy_model: Option<PathBuf>` only (default `None`) | no whisper model on disk; `install_whisper_model` stubs in `setup_manager/platforms/*` | alias-guard only, no engine |

Risks: qwen registry points at dir slot while file sits at root — resolve before wiring loader.
Next: `pkg/models` inventory reconciliation (confirm qwen filename, laya layout) before any loader work.

## 3. Phase 3 Voice (in-progress)

Files: `src/voice.rs` (orchestration: NLU init → TTS init → STT loop), `src/core/stt.rs` (SenseVoice
`OfflineRecognizer`, 16 kHz, `language auto`, `num_threads 2`, paths `pkg/models/sense-voice/model.onnx`+`tokens.txt`),
`src/core/audio_capture.rs` (cpal VAD capture), `src/core/piper_ffi.rs` (cfg `windows+piper_ffi` only),
`src/core/tts.rs` (FFI fast-path + `piper.exe` spawn fallback, espeak-ng-data search).
Deps: `sherpa-onnx 1.11`, `cpal 0.16`, `rodio 0.17` (`Cargo.toml`).
Bundle: `src/main.rs:153-155` copies `piper.exe` DLLs + `espeak-ng-data` next to exe on desktop bundle.
Deny-by-default: STT missing-model returns `Err` (never silent); TTS init warning falls back, never fakes readiness.
TODO: `!Send` recognizer stays on dedicated blocking thread pool (registry comment `config.rs:18-25`).

## 4. Phase 4 Reflex (todo)

Goal: <300 ms local reflex verdict (allow/deny/escalate) ahead of full brain pass.
Present: `EventKind::ReflexDecision { task_id, decision, risk }` stub only — no reflex logic in `igris-core` (by design).
Rules: recognizer is `!Send` — never share across threads (see §3 TODO). Reflex must poll `CancellationNode::check()`.
TODO: reflex executor + latency budget + `select!` on `cancelled()` (cf. `runtime.rs` pattern).

## 5. Phase 5 Brain (todo)

Present (app-side, not core-wired): `src/nlu/sbert.rs` (`SbertEngine`, MiniLM ONNX via `ort` load-dynamic,
TF-IDF fallback when `model.onnx` absent), `src/nlu/engine.rs` (MiniLM centroid tier + keyword/Jaccard tiers,
`use_sbert`/`sbert_initialized`), `src/nlu/intent_vectors.rs` (baked centroids), `src/online/task_planner.rs`
(LLM task planner), `bake_nlu.py` (maintainer-only bake: `pkg/models/sbert` → `intent_vectors.rs` + `model.onnx`).
TODO: wire brain behind `CapabilityToken` + `CancellationNode`; offline/online router stays in app until gated.

## 6. Phase 6 Tasks (todo)

Present: `src/tools/mod.rs::execute_task_plan` (step loop, param inheritance, per-step `add_task_step`),
`src/nlu/context.rs::TaskStepRecord` + `ContextMemory::task_history`, `src/nlu/planner.rs::TaskPlan`.
Cancel propagation: `crates/igris-core/src/runtime.rs:246` (`cancel_propagates_ui_to_device` test pins
UI->Task->Agent->Tool->Device fan-out; `CancellationNode::cancel()` recurses, `check()` yields `E_CANCELLED`).
TODO: bind `execute_task_plan` steps to task-scoped `CancellationNode`s + capability-checked tool calls.

## 7. Phase 7 Permissions (in-progress UI done)

Done (UI): `src/ui/permission_dialog.rs` — `PendingPermissionRequest` (display-only) + `PermissionDialog`
(Action/Location/Risk/Reason + Allow-Once/For-Task/Deny/Always-Deny), `validate_voice_approval` (context-gated:
bare "yes" never approves; wrong-action rejected; unit-tested).
Present: `src/setup_manager/permissions.rs::PermissionsConfig` (module-level grants incl. `sbert_nlu`),
`src/eco/permissions.rs::EcoPermissions` (trust read/write over `EcoStorage`).
Deny-by-default: dialog is intent-capture only — enforcement lives in backend (`RiskLevel`,
`EventKind::PermissionRequest`, `CapabilityToken`, `CancellationNode`); UI never grants.
Next: enforce gate — route `PermissionDecision` into backend approval queue issuing short-lived `CapabilityToken`s.

## 8. Phase 8 Tools/MCP (todo)

Present: `src/tools/registry.rs` (dynamic registry, `execute_registered_tool`, `GLOBAL_TOOL_REGISTRY`),
`src/mcp/server.rs::handle_request` (JSON-RPC 2.0 stdio: `initialize`, `tools/list`, `tools/call`, `ping`;
`tools/call` forwards optional `command` string; stdout is protocol-only, logs to stderr).
README documents `igrisecosystem --mcp` → `tools/list` (28 tools + schemas) → `tools/call`.
TODO: MCP conformance pass (error codes, schema completeness) + capability-gated `tools/call`.

## 9. Phase 9 Memory (todo)

Present: `src/nlu/context.rs::ContextMemory` (`task_history: VecDeque<TaskStepRecord>`, `GLOBAL_CONTEXT`),
`src/utils/shared_memory.rs::SharedMemory` (`SHARED_MEMORY` lazy static, stats-gated),
`src/eco/storage.rs::EcoStorage` (JSON trust/clipboard/notification stores under `pkg/ecosystem/`),
`EventKind::MemoryActivity` envelope in core.
Retention: caps exist (`CLIPBOARD_HISTORY_MAX` 50, `NOTIFICATION_HISTORY_MAX` 100 in `src/eco/constants.rs`).
TODO: explicit retention/eviction policy + redaction audit (payload sizes only on bus, never bodies).

## 10. Phase 10 Device Mesh (in-progress trust-gates fixed)

Runtime: `src/eco/*` (`discovery.rs` axum server + multi-interface /24 scan, `pairing.rs` trust store,
`clipboard.rs`, `notification.rs`, `transport.rs`, `crypto.rs`, `storage.rs`, `events.rs`, `manager.rs`),
`src/fastswap/*` (axum receiver `network/server.rs`, sender `network/client.rs`, `network/discovery.rs`,
`tls.rs`, `models/` progress/transfer), panels `src/ui/fastswap_panel.rs` + `src/ui/eco_device_panel.rs`.
Trust-gates (deny-by-default, fail closed): `deny_untrusted()` + `is_trusted_sync()` enforced on
`/clipboard/sync`, `/notification/{sync,reply,dismiss,action,request}` (audit-log `E_FORBIDDEN`, never emit);
stale-trust demotion + `persist_trust`/`persist_untrust` symmetric; self-IP skip on all interfaces.
Ports: README networking map `53317` HTTP / `53318` TLS FastSwap, `53327` HTTP / `53328` TLS eco.
TLS: 1.3 via `rcgen` self-signed (`igris.local`) + `rustls`; LAN-only `AcceptAnyCertVerifier` (see §12).
Discovery fix: `local_subnets()` scans every usable IPv4 /24 (private first, max 4), skips loopback/link-local/rmnet.

## 11. Phase 11 Agents (todo)

Present: `src/tools/mod.rs::route_llm_tool` (LLM tool-call router over registry + `general_chat` fallback),
`src/plugins/system.rs::PluginManager` (12 builtin plugin groups), `src/online/*` (reasoning/intent/task planner).
Core handles ready: `IgrisRuntime::spawn_cancellable` (cancel-first `select!`), `CapabilityToken::validate`
(task/device/tool/scope/expiry/revoke deny-by-default).
TODO: agent spawn/cancel lifecycle bound to `IgrisRuntime` + per-agent `CapabilityToken`; route all agent tool
calls through `route_llm_tool` with token check.

## 12. Phase 12 SENTINEL (in-progress status done)

Done (status): `src/ui/sentinel_status.rs::SentinelStatus` — ONLINE/OFFLINE badge, `MODEL :: provider/short-id`,
`DEVICE :: active_device`, `TASK :: current || status`, CANCEL entry point (emits `on_cancel`; host fans out
UI->Task->Agent->Tool->Device incl. `CancellationNode::cancel()` — UI never owns cancellation).
Present: `src/eco/pairing.rs` (`PAIRING_MANAGER`, `persist_trust/untrust`, `is_trusted_sync`, local device id/name),
`src/eco/manager.rs` lifecycle, `src/fastswap/tls.rs::AcceptAnyCertVerifier` (LAN-only, no hostname/chain/expiry
checks; MITM-possible off-LAN — TOFU/CA-pinning TODO before any internet use).
Next: full SENTINEL audit — live Task/Tool/Security/Memory/Agent event tabs on the `igris-core` bus, token-expiry
surface, backend approval queue wiring (§7).

## A. File List

Core: `crates/igris-core/src/lib.rs`, `error.rs`, `config.rs`, `types.rs`, `events.rs`, `runtime.rs`, `logging.rs`,
`crates/igris-core/tests/smoke.rs`, `crates/igris-core/Cargo.toml`.
Voice/models: `src/voice.rs`, `src/core/{stt,audio_capture,piper_ffi,tts,vad,wake_word}.rs`, `bake_nlu.py`,
`src/nlu/{engine,sbert,intent_vectors,planner,context}.rs`, `src/online/task_planner.rs`, `src/setup_manager/mod.rs`.
Perms/UI: `src/ui/permission_dialog.rs`, `src/ui/sentinel_status.rs`, `src/ui/mod.rs`,
`src/ui/{fastswap_panel,eco_device_panel}.rs`, `src/setup_manager/permissions.rs`.
Mesh/security: `src/eco/{discovery,pairing,manager,permissions,storage,clipboard,notification,constants}.rs`,
`src/fastswap/tls.rs`, `src/fastswap/network/{server,client,discovery}.rs`.
Tools/agents/memory: `src/tools/{mod,registry}.rs`, `src/mcp/server.rs`, `src/plugins/system.rs`,
`src/utils/shared_memory.rs`, `src/nlu/context.rs`.
Docs: `README.md`, `ECOSYSTEM_IMPLEMENTATION_PLAN.md`, `description.md`, `graphify-out/GRAPH_REPORT.md`.

## B. Build Results

- `cargo test -p igris-core` (2026-09-22, ran locally): 16 unit passed + `smoke.rs` 8 passed, 0 failed.
- `cargo check` workspace (2026-09-22, ran locally): green, 6 warnings (incl. `piper_ffi` cfg + unused `mut` in `fastswap_panel.rs:552`).
- No ports bound, app never launched (per rules).

## C. Risks

- `53329+` fallback invisible: server/proxy retry across 10-port ranges but scanners probe canonical ports only —
  a fallback instance is undiscoverable. Mitigation: single-instance check before launch (see §D).
- TLS self-signed expected: LAN peers use `rcgen` self-signed certs; clients accept-any by design on LAN.
  Never expose `AcceptAnyCertVerifier` to the internet (MITM); TOFU/CA-pinning TODO stands.
- Multi-interface/VPN scan + firewall: discovery scans all usable /24s but still needs same-subnet + firewall
  allows for `53317/53318/53327/53328`; cross-subnet needs manual IPs / relay (roadmap).
- Whisper alias guard: `whisper_legacy_model` must never alias `stt_model` (validated); no whisper engine exists —
  do not invent one.
- `!Send` recognizer: sherpa `OfflineRecognizer` must stay on one blocking thread; never share across threads.
- `PAIRING_MANAGER None` no-op: `is_trusted_sync` returns `false` when manager uninitialized (fail closed by design);
  ensure `init_pairing_manager` runs before serving (cf. `src/eco/manager.rs:78` comment).

## D. Next Steps

1. Re-run `cargo test -p igris-core` + `cargo check` after each phase change (docs-only tracker never binds ports).
2. `pkg/models` inventory reconciliation: confirm qwen filename vs `qwen_path` dir slot, laya layout (manual-config).
3. Single-instance check (kill duplicate `igrisecosystem` before relaunch; canonical ports only).
4. Enforce permissions gate (§7): backend approval queue + short-lived `CapabilityToken` issuance.
5. MCP conformance (§8): schemas + error codes + capability-gated `tools/call`.
6. SENTINEL audit (§12): full dashboard on `igris-core` bus + token-expiry + approval-queue wiring.
7. `graphify update .` after code changes (AST-only, docs-only runs need no graph write).
