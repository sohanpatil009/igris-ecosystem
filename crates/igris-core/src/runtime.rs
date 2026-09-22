//! Async runtime + cancellation hierarchy (master Sec31).
//!
//! Cancellation propagates strictly top-down:
//! `UI -> Task -> Agent -> Tool -> Device`.
//!
//! - Cancelling a parent cancels ALL descendants (recursive).
//! - A child can never outlive its parent: [`CancellationNode::child`]
//!   rejects tiers that do not go strictly downward.
//! - Polling is via [`CancellationNode::check`] / `is_cancelled()`;
//!   executors must poll at every await point that can block.
//!
//! TODO Phase 2: wire Task/Agent/Tool/Device executors to these nodes
//! (tokio `select!` on `cancelled()` future + timeout budgets).

use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
};

use crate::error::{IgrisError, Result};

/// Cancellation tier, ordered top-down.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CancellationTier {
    Ui = 0,
    Task = 1,
    Agent = 2,
    Tool = 3,
    Device = 4,
}

impl CancellationTier {
    /// Short label for logs.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::Task => "task",
            Self::Agent => "agent",
            Self::Tool => "tool",
            Self::Device => "device",
        }
    }

    /// The only legal child tier is a strictly deeper one.
    fn allows_child(self, child: Self) -> bool {
        child > self
    }
}

/// A node in the cancellation tree.
///
/// Thread-safe (`Send + Sync`) so UI, task, agent, tool, and device loops
/// can share handles across threads.
#[derive(Debug)]
pub struct CancellationNode {
    tier: CancellationTier,
    cancelled: AtomicBool,
    parent: Option<Arc<CancellationNode>>,
    children: Mutex<Vec<Weak<CancellationNode>>>,
    notify: tokio::sync::Notify,
}

impl CancellationNode {
    /// Create a root UI node (top of the tree).
    pub fn root_ui() -> Arc<Self> {
        Arc::new(Self {
            tier: CancellationTier::Ui,
            cancelled: AtomicBool::new(false),
            parent: None,
            children: Mutex::new(Vec::new()),
            notify: tokio::sync::Notify::new(),
        })
    }

    /// Spawn a child node. Fails when `tier` is not strictly deeper than
    /// the parent (prevents orphan / upward edges).
    pub fn child(parent: &Arc<Self>, tier: CancellationTier) -> Result<Arc<Self>> {
        if !parent.tier.allows_child(tier) {
            return Err(IgrisError::validation(format!(
                "illegal cancellation edge: {} -> {} (must go UI->Task->Agent->Tool->Device)",
                parent.tier.as_str(),
                tier.as_str()
            )));
        }
        // A cancelled parent can never gain live children (deny-by-default).
        if parent.is_cancelled() {
            return Err(IgrisError::cancelled(format!(
                "cannot spawn {} under cancelled {}",
                tier.as_str(),
                parent.tier.as_str()
            )));
        }
        let node = Arc::new(Self {
            tier,
            cancelled: AtomicBool::new(false),
            parent: Some(Arc::clone(parent)),
            children: Mutex::new(Vec::new()),
            notify: tokio::sync::Notify::new(),
        });
        parent
            .children
            .lock()
            .expect("cancellation children lock")
            .push(Arc::downgrade(&node));
        Ok(node)
    }

    /// This node's tier.
    pub fn tier(&self) -> CancellationTier {
        self.tier
    }

    /// `true` when this node or any ancestor was cancelled.
    pub fn is_cancelled(&self) -> bool {
        if self.cancelled.load(Ordering::SeqCst) {
            return true;
        }
        match &self.parent {
            Some(p) => p.is_cancelled(),
            None => false,
        }
    }

    /// Cancel this node and ALL descendants, recursively.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.notify.notify_waiters();
        // Clone live children out of the lock, then recurse (no deadlock).
        let live: Vec<Arc<CancellationNode>> = self
            .children
            .lock()
            .expect("cancellation children lock")
            .iter()
            .filter_map(Weak::upgrade)
            .collect();
        for child in live {
            child.cancel();
        }
    }

    /// Future that resolves when this node (or an ancestor) is cancelled.
    /// Executors should `select!` on this alongside work.
    pub async fn cancelled(&self) {
        loop {
            if self.is_cancelled() {
                return;
            }
            self.notify.notified().await;
        }
    }

    /// Return `E_CANCELLED` when cancelled, else `Ok(())`.
    /// Call at every fallible await boundary.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(IgrisError::cancelled(format!(
                "{} cancelled",
                self.tier.as_str()
            )));
        }
        Ok(())
    }
}

/// Owned multi-thread tokio runtime plus the root UI cancellation node.
///
/// Phase 1 owns a runtime so Phase 2 executors have a single init /
///
/// shutdown point. The Dioxus 0.7 desktop host keeps its own runtime;
/// bridge it by sharing [`CancellationNode`] handles, not by nesting
/// runtimes.
pub struct IgrisRuntime {
    runtime: tokio::runtime::Runtime,
    root: Arc<CancellationNode>,
}

impl IgrisRuntime {
    /// Build a multi-thread runtime + fresh UI root.
    pub fn new() -> Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .thread_name("igris-core")
            .enable_all()
            .build()
            .map_err(|e| IgrisError::config(format!("tokio init failed: {e}")))?;
        Ok(Self {
            runtime,
            root: CancellationNode::root_ui(),
        })
    }

    /// Borrow the root UI cancellation node.
    pub fn root(&self) -> &Arc<CancellationNode> {
        &self.root
    }

    /// Block the current thread on a future (convenience for `main`).
    pub fn block_on<F>(&self, fut: F) -> F::Output
    where
        F: std::future::Future,
    {
        self.runtime.block_on(fut)
    }

    /// Spawn a future bound to a cancellation node. The wrapper polls
    /// cancellation first: already-cancelled nodes never start work.
    pub fn spawn_cancellable<F, T>(
        &self,
        node: Arc<CancellationNode>,
        fut: F,
    ) -> tokio::task::JoinHandle<Option<T>>
    where
        F: std::future::Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        self.runtime.spawn(async move {
            if node.is_cancelled() {
                return None;
            }
            tokio::select! {
                out = fut => Some(out),
                _ = node.cancelled() => None,
            }
        })
    }

    /// Cancel the whole tree, then shut the runtime down in the background.
    /// Idempotent: safe to call twice.
    pub fn shutdown(self) {
        self.root.cancel();
        self.runtime.shutdown_background();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn illegal_edges_rejected() {
        let ui = CancellationNode::root_ui();
        let err = CancellationNode::child(&ui, CancellationTier::Ui).unwrap_err();
        assert_eq!(err.error_code(), "E_VALIDATION");
    }

    #[test]
    fn cancel_propagates_ui_to_device() {
        let ui = CancellationNode::root_ui();
        let task = CancellationNode::child(&ui, CancellationTier::Task).unwrap();
        let agent = CancellationNode::child(&task, CancellationTier::Agent).unwrap();
        let tool = CancellationNode::child(&agent, CancellationTier::Tool).unwrap();
        let device = CancellationNode::child(&tool, CancellationTier::Device).unwrap();

        for n in [&task, &agent, &tool, &device] {
            assert!(!n.is_cancelled());
            n.check().unwrap();
        }
        task.cancel();
        assert!(task.is_cancelled());
        assert!(agent.is_cancelled());
        assert!(tool.is_cancelled());
        assert!(device.is_cancelled());
        assert!(!ui.is_cancelled());
        assert!(device.check().unwrap_err().error_code() == "E_CANCELLED");
    }

    #[test]
    fn no_children_under_cancelled_parent() {
        let ui = CancellationNode::root_ui();
        ui.cancel();
        let err = CancellationNode::child(&ui, CancellationTier::Task).unwrap_err();
        assert_eq!(err.error_code(), "E_CANCELLED");
    }

    #[test]
    fn runtime_shutdown_is_idempotent_path() {
        let rt = IgrisRuntime::new().unwrap();
        let root = Arc::clone(rt.root());
        let task = CancellationNode::child(&root, CancellationTier::Task).unwrap();
        let h = rt.spawn_cancellable(task, async { 42u32 });
        let out = rt.block_on(h).unwrap();
        assert_eq!(out, Some(42));
        rt.shutdown();
    }
}
