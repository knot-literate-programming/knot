//! Cooperative cancellation at compilation boundaries.
use anyhow::{Result, bail};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Shared cancellation flag. In-flight interpreter calls finish (or time out),
/// but subsequent nodes and publication can be skipped.
#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);

impl Cancellation {
    /// Mark this compilation as superseded.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
    /// Whether a newer request has superseded this compilation.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
    /// Stop at a safe compilation boundary.
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            bail!("Compilation superseded");
        }
        Ok(())
    }
}
