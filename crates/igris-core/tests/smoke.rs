//! Phase 1 smoke tests: foundation boots, validates, and refuses safely.
//!
//! Run with: `cargo test -p igris-core --test smoke`
//! These tests never bind sockets, never touch `pkg/device_id`,
//! `pkg/ecosystem/*.json`, or certs.

use std::sync::Arc;

use chrono::{TimeDelta, Utc};
use igris_core::{
    config::AppConfig,
    error::IgrisError,
    events::{EventBus, EventKind, IgrisEvent},
    logging::{redacted, sanitize_message},
    runtime::{CancellationNode, CancellationTier, IgrisRuntime},
    types::{CapabilityToken, DeviceId, RiskLevel, TaskId, ToolId},
};

#[test]
fn config_registry_defaults_validate() {
    let cfg = AppConfig::default();
    cfg.validate().unwrap();
    assert_eq!(cfg.eco.http_port, 53327);
    assert_eq!(cfg.eco.tls_port, 53328);
    assert_eq!(cfg.eco.fastswap_http_port, 53317);
    assert_eq!(cfg.eco.fastswap_tls_port, 53318);
    assert!(cfg.models.stt_model.ends_with("sense-voice/model.onnx"));
    assert!(cfg.models.stt_tokens.ends_with("sense-voice/tokens.txt"));
    // JSON schema round-trip.
    let json = cfg.to_json().unwrap();
    let back = AppConfig::from_json(&json).unwrap();
    assert_eq!(cfg, back);
}

#[test]
fn error_model_carries_sec30_fields() {
    let err = IgrisError::forbidden("deny-by-default")
        .with_step("pair-verify")
        .with_tool("eco.pair");
    assert_eq!(err.error_code(), "E_FORBIDDEN");
    assert!(!err.message.is_empty());
    assert!(!err.is_retryable());
    assert_eq!(err.error_step_id(), Some("pair-verify"));
    assert_eq!(err.error_tool(), Some("eco.pair"));
}

#[test]
fn risk_levels_parse_and_gate() {
    assert_eq!("HIGH".parse::<RiskLevel>().unwrap(), RiskLevel::High);
    assert!(RiskLevel::Critical.requires_capability_token());
    assert!(!RiskLevel::Low.requires_approval());
    assert!("nope".parse::<RiskLevel>().is_err());
}

#[test]
fn capability_token_is_deny_by_default() {
    let task = TaskId::generate();
    let device = DeviceId::generate();
    let tool = ToolId::new("clipboard.push").unwrap();
    let mut token = CapabilityToken::issue(
        vec!["clipboard.write".to_string()],
        Utc::now() + TimeDelta::minutes(15),
        task,
        device,
        tool.clone(),
    )
    .unwrap();
    token
        .validate("clipboard.write", &task, &device, &tool)
        .unwrap();
    // Wrong scope denies.
    assert!(token.validate("shell.exec", &task, &device, &tool).is_err());
    // Revocation denies.
    token.revoke();
    assert!(token
        .validate("clipboard.write", &task, &device, &tool)
        .is_err());
}

#[test]
fn events_bus_fanout() {
    let bus = EventBus::new(16);
    let mut rx = bus.subscribe();
    let event = IgrisEvent::new(EventKind::TaskProgress {
        task_id: TaskId::generate(),
        progress: 0.25,
        phase: Some("verify".to_string()),
    });
    assert_eq!(bus.receiver_count(), 1);
    bus.publish(event.clone());
    let got = rx.try_recv().unwrap();
    assert_eq!(got.id, event.id);
}

#[test]
fn cancellation_propagates_ui_to_device() {
    let ui = CancellationNode::root_ui();
    let task = CancellationNode::child(&ui, CancellationTier::Task).unwrap();
    let agent = CancellationNode::child(&task, CancellationTier::Agent).unwrap();
    let tool = CancellationNode::child(&agent, CancellationTier::Tool).unwrap();
    let device = CancellationNode::child(&tool, CancellationTier::Device).unwrap();
    ui.cancel();
    for node in [&ui, &task, &agent, &tool, &device] {
        assert!(node.is_cancelled());
    }
    // Illegal upward edge is rejected.
    assert!(CancellationNode::child(&task, CancellationTier::Ui).is_err());
}

#[test]
fn logging_never_emits_secrets() {
    let msg = sanitize_message("api_key=TOPSECRET device ok");
    assert!(!msg.contains("TOPSECRET"));
    assert_eq!(format!("{:?}", redacted("TOPSECRET")), "[REDACTED]");
}

#[test]
fn runtime_builds_and_runs_cancellable_work() {
    let rt = IgrisRuntime::new().unwrap();
    let task = CancellationNode::child(rt.root(), CancellationTier::Task)
        .map(|n| Arc::clone(&n))
        .unwrap();
    let handle = rt.spawn_cancellable(task, async { 7u32 });
    let out = rt.block_on(handle).unwrap();
    assert_eq!(out, Some(7));
    rt.shutdown();
}
