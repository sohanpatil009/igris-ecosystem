//! Phase 1 event envelope.
//!
//! Everything in the ecosystem communicates through events; future
//! modules must subscribe, never call each other directly. Phase 1 only
//! defines the envelope + payload shapes and an in-process broadcast bus.
//! No persistence, no network fan-out yet.
//!
//! TODO Phase 2: durable event log + cross-device fan-out behind
//! capability checks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::types::{DeviceId, RiskLevel, TaskId, ToolId};

/// Event envelope: unique id + timestamp + typed payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IgrisEvent {
    /// Event id (UUID v4).
    pub id: Uuid,
    /// UTC emission time.
    pub timestamp: DateTime<Utc>,
    /// Typed payload.
    pub kind: EventKind,
}

impl IgrisEvent {
    /// Wrap a payload with a fresh id + current timestamp.
    pub fn new(kind: EventKind) -> Self {
        Self {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            kind,
        }
    }

    /// Short label for logs (`device.discovered`, ...).
    pub fn label(&self) -> &'static str {
        self.kind.label()
    }
}

/// All Phase 1 event payloads.
///
/// Variants required by the task: `DeviceDiscovered`, `ClipboardChanged`,
/// `TaskProgress`, `PermissionRequest`, `ReflexDecision`, `SecurityEvent`,
/// `MemoryActivity`, `AgentActivity`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    /// A peer answered discovery (`GET /api/ecosystem/v1/info`).
    DeviceDiscovered {
        device_id: DeviceId,
        name: String,
        platform: String,
    },
    /// Local clipboard content changed (hash + size only — never the
    /// raw text, which may contain secrets).
    ClipboardChanged {
        device_id: DeviceId,
        /// SHA-256 hex of the content (dedup key).
        content_hash: String,
        /// Byte length of the content.
        content_len: u64,
    },
    /// Progress for a long-running task (0.0..=1.0).
    TaskProgress {
        task_id: TaskId,
        /// 0.0..=1.0 fraction complete.
        progress: f32,
        /// Optional human-readable phase (`download`, `verify`, ...).
        phase: Option<String>,
    },
    /// A tool requested an elevated action; awaiting approval / token.
    PermissionRequest {
        task_id: TaskId,
        tool: ToolId,
        risk: RiskLevel,
        reason: String,
    },
    /// Reflex layer verdict stub (no reflex logic in Phase 1).
    ReflexDecision {
        task_id: TaskId,
        /// `allow` / `deny` / `escalate`.
        decision: String,
        risk: RiskLevel,
    },
    /// Security-relevant occurrence (auth failure, revoked token use, ...).
    SecurityEvent {
        /// Machine code (`auth.failed`, `token.revoked`, ...).
        code: String,
        detail: String,
        risk: RiskLevel,
    },
    /// Shared-memory read/write activity (payload sizes only).
    MemoryActivity {
        task_id: TaskId,
        operation: String,
        bytes: u64,
    },
    /// Agent lifecycle activity (spawn / step / finish).
    AgentActivity {
        task_id: TaskId,
        agent: String,
        state: String,
    },
}

impl EventKind {
    /// Short label for logs / metrics.
    pub fn label(&self) -> &'static str {
        match self {
            Self::DeviceDiscovered { .. } => "device.discovered",
            Self::ClipboardChanged { .. } => "clipboard.changed",
            Self::TaskProgress { .. } => "task.progress",
            Self::PermissionRequest { .. } => "permission.request",
            Self::ReflexDecision { .. } => "reflex.decision",
            Self::SecurityEvent { .. } => "security.event",
            Self::MemoryActivity { .. } => "memory.activity",
            Self::AgentActivity { .. } => "agent.activity",
        }
    }
}

/// In-process broadcast bus (tokio broadcast over [`IgrisEvent`]).
///
/// Least-privilege note: the bus is unbounded-trust *within* the process
/// only. Cross-device fan-out must re-validate the sender's capability
/// token (TODO Phase 2) — subscribers must not assume remote events are
/// authorized just because they arrived.
#[derive(Debug, Clone)]
pub struct EventBus {
    tx: tokio::sync::broadcast::Sender<IgrisEvent>,
}

impl EventBus {
    /// Create a bus with the given lag buffer capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(capacity.max(1));
        Self { tx }
    }

    /// Publish an event. Returns the subscriber count.
    /// Lagged / absent receivers are not errors (fire-and-forget bus).
    pub fn publish(&self, event: IgrisEvent) -> usize {
        self.tx.send(event).unwrap_or(0)
    }

    /// Subscribe to the bus.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<IgrisEvent> {
        self.tx.subscribe()
    }

    /// Current subscriber count.
    pub fn receiver_count(&self) -> usize {
        self.tx.receiver_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_cover_all_variants() {
        let device = DeviceId::generate();
        let task = TaskId::generate();
        let tool = ToolId::new("clipboard.push").unwrap();
        let kinds = vec![
            EventKind::DeviceDiscovered {
                device_id: device,
                name: "x".into(),
                platform: "windows".into(),
            },
            EventKind::ClipboardChanged {
                device_id: device,
                content_hash: "abc".into(),
                content_len: 3,
            },
            EventKind::TaskProgress {
                task_id: task,
                progress: 0.5,
                phase: None,
            },
            EventKind::PermissionRequest {
                task_id: task,
                tool,
                risk: RiskLevel::High,
                reason: "demo".into(),
            },
            EventKind::ReflexDecision {
                task_id: task,
                decision: "escalate".into(),
                risk: RiskLevel::Medium,
            },
            EventKind::SecurityEvent {
                code: "auth.failed".into(),
                detail: "bad otp".into(),
                risk: RiskLevel::High,
            },
            EventKind::MemoryActivity {
                task_id: task,
                operation: "write".into(),
                bytes: 12,
            },
            EventKind::AgentActivity {
                task_id: task,
                agent: "planner".into(),
                state: "step".into(),
            },
        ];
        for k in &kinds {
            assert!(!k.label().is_empty());
            let env = IgrisEvent::new(k.clone());
            let json = serde_json::to_string(&env).unwrap();
            let back: IgrisEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(back.kind.label(), k.label());
        }
    }
}
