//! A body held in its own kernel's form ([`HeldBody`]), the record made from it only when
//! something asks for the record.
//!
//! A kernel's result can stay in that kernel: the router keeps it behind the body handle, and
//! the record (`solidcore_record::brep::Brep`) is made from it ([`HeldBody::export`]) the first
//! time a caller reads the record. Every such read is counted by the router, by where it was
//! asked from, so a measurement shows which callers still need the record of a body that
//! another kernel holds.

use crate::{BackendError, BackendId, Exported};

/// A body a kernel holds in its own form, object-safe so the router can keep any kernel's body
/// behind one handle.
pub trait HeldBody: Send + Sync + std::fmt::Debug + 'static {
    /// The kernel holding the body.
    fn backend(&self) -> BackendId;

    /// The body as a record in canonical order, with the history that made it. Called once per
    /// body at most: the router keeps what it gives.
    fn export(&self) -> Result<Exported, BackendError>;

    /// The body's own form, for the kernel's adapter to find its own type again.
    fn as_any(&self) -> &dyn std::any::Any;
}
