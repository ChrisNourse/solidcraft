//! Bodies another kernel holds in its own form ([`solidcraft_kernel_api::HeldBody`]), and the
//! count of every read of their record.
//!
//! A held body has no record until something reads it ([`crate::Body::record`], or the kernel's
//! own code reading the body's faces and edges): the first read exports the body
//! ([`solidcraft_kernel_api::HeldBody::export`]) and the record is kept. Every read is counted
//! by the line that asked ([`record_reads`]), the first one of each body as a conversion, so a
//! measurement shows which callers still need the record of a body another kernel holds. A
//! solidcore body is never held: it is its record, and reading it counts nothing.

use std::collections::BTreeMap;
use std::panic::Location;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use solidcore_record::brep::Brep;
use solidcraft_kernel_api::HeldBody;

/// A held body and its record once made ([`crate::body::RecordCell::Held`]).
pub(crate) struct HeldCell {
    pub(crate) body: Arc<dyn HeldBody>,
    /// The record once made, and why the body did not give one when it did not.
    made: OnceLock<(Arc<Brep>, Option<String>)>,
    reads: AtomicU64,
}

impl std::fmt::Debug for HeldCell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeldCell").field("body", &self.body).field("made", &self.made.get().is_some()).finish_non_exhaustive()
    }
}

impl HeldCell {
    pub(crate) fn new(body: Arc<dyn HeldBody>) -> HeldCell {
        HeldCell { body, made: OnceLock::new(), reads: AtomicU64::new(0) }
    }

    /// The record, exported the first time it is read; every read counted at `at`. A body that
    /// does not export (or panics exporting), or exports a record that fails the record check,
    /// reads as an empty record, and every operation on it is refused with why
    /// ([`HeldCell::failure`], [`crate::Body::require_brep`]): never taken as a body with nothing
    /// in it.
    pub(crate) fn record(&self, at: &'static Location<'static>) -> &Arc<Brep> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        note(at, false);
        &self
            .made
            .get_or_init(|| {
                note(at, true);
                let backend = self.body.backend();
                log::trace!("the record of a body {backend} holds is made, read at {}:{}", at.file(), at.line());
                let made = crate::guard("held body export", || {
                    let out = self.body.export().map_err(crate::KernelError::from)?;
                    crate::body::solid_record(out.record)
                });
                match made {
                    Ok(record) => (Arc::new(record), None),
                    Err(e) => {
                        let why = format!("the body {backend} holds gives no record ({e})");
                        log::error!("{why}");
                        (Arc::new(crate::body::empty_record()), Some(why))
                    }
                }
            })
            .0
    }

    /// Why the body gave no record, once its record was read and it did not.
    pub(crate) fn failure(&self) -> Option<&str> {
        self.made.get().and_then(|(_, why)| why.as_deref())
    }

    /// Whether the record was made.
    pub(crate) fn made(&self) -> bool {
        self.made.get().is_some()
    }

    /// How many times the record was read.
    pub(crate) fn reads(&self) -> u64 {
        self.reads.load(Ordering::Relaxed)
    }
}

/// Where the record of held bodies was read, and how often ([`record_reads`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecordReads {
    /// Records made from held bodies: one for each body whose record was read.
    pub conversions: u64,
    /// Reads of a held body's record, a body's first included.
    pub reads: u64,
    /// By the line that read: `(file, line, reads, conversions)`, by file and line.
    pub sites: Vec<RecordReadSite>,
}

/// One line that read the record of held bodies ([`RecordReads::sites`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordReadSite {
    pub file: &'static str,
    pub line: u32,
    pub reads: u64,
    pub conversions: u64,
}

static SITES: Mutex<BTreeMap<(&'static str, u32), (u64, u64)>> = Mutex::new(BTreeMap::new());

/// Counts a read at `at`, or the conversion it made (`converts`).
fn note(at: &'static Location<'static>, converts: bool) {
    let mut sites = SITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let site = sites.entry((at.file(), at.line())).or_default();
    if converts {
        site.1 += 1;
    } else {
        site.0 += 1;
    }
}

/// Every read of a held body's record so far in this process, by the line that read it: what
/// the record still stands in for when another kernel holds the body. Empty when every body is
/// solidcore's.
pub fn record_reads() -> RecordReads {
    let sites = SITES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let sites: Vec<RecordReadSite> =
        sites.iter().map(|(&(file, line), &(reads, conversions))| RecordReadSite { file, line, reads, conversions }).collect();
    RecordReads { conversions: sites.iter().map(|s| s.conversions).sum(), reads: sites.iter().map(|s| s.reads).sum(), sites }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Body;
    use solidcore_record::Vec3;
    use solidcraft_kernel_api::{BackendError, BackendId, Exported};

    /// A kernel that holds a solidcore record as its own form and gives it back when asked: the
    /// least a held body can be, so every read of its record is one the router counted.
    #[derive(Debug)]
    struct Loopback(Brep);

    impl HeldBody for Loopback {
        fn backend(&self) -> BackendId {
            BackendId("loopback")
        }
        fn export(&self) -> Result<Exported, BackendError> {
            Ok(Exported { record: self.0.clone(), history: None })
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    fn held_box() -> (Body, Body) {
        let solid = crate::box_solid(Vec3::new(0.0, 0.0, 0.0), Vec3::new(10.0, 20.0, 30.0)).unwrap();
        let held = Body::held(Arc::new(Loopback(solid.record().clone())));
        (solid, held)
    }

    #[test]
    fn a_held_body_has_no_record_until_one_is_read() {
        let (_, held) = held_box();
        assert_eq!(held.held_record_reads(), Some((false, 0)));
        assert!(held.held_body().is_some_and(|b| b.backend() == BackendId("loopback")));
        let clone = held.clone();
        assert!(clone.same(&held), "a clone is the same body");
        assert_eq!(held.held_record_reads(), Some((false, 0)), "cloning and comparing read nothing");
    }

    #[test]
    fn a_held_body_reads_as_the_record_it_exports_made_once() {
        let (solid, held) = held_box();
        let before = record_reads();
        assert_eq!(held.record(), solid.record());
        let line = line!() - 1;
        assert_eq!(held.record().faces.len(), 6);
        assert_eq!(held.held_record_reads(), Some((true, 2)), "made at the first read and kept");
        let after = record_reads();
        assert!(after.conversions > before.conversions);
        let here = after.sites.iter().find(|s| s.file == file!() && s.line == line);
        assert!(here.is_some_and(|s| s.conversions == 1 && s.reads == 1), "the read is counted where it was asked: {:?}", after.sites);
    }

    #[test]
    fn a_held_body_measures_as_its_record() {
        let (solid, held) = held_box();
        let (a, b) = (crate::measure_mass(&solid).unwrap(), crate::measure_mass(&held).unwrap());
        assert_eq!(a.volume.to_bits(), b.volume.to_bits());
        assert!(held.held_record_reads().is_some_and(|(made, _)| made));
    }

    #[test]
    fn a_solidcore_body_is_not_held_and_counts_nothing() {
        let (solid, _) = held_box();
        assert!(solid.held_body().is_none());
        assert_eq!(solid.held_record_reads(), None);
    }

    #[test]
    fn a_held_body_that_does_not_export_reads_as_empty() {
        #[derive(Debug)]
        struct Refuses;
        impl HeldBody for Refuses {
            fn backend(&self) -> BackendId {
                BackendId("refuses")
            }
            fn export(&self) -> Result<Exported, BackendError> {
                Err(BackendError::Exchange("no record".into()))
            }
            fn as_any(&self) -> &dyn std::any::Any {
                self
            }
        }
        let held = Body::held(Arc::new(Refuses));
        assert!(held.record().faces.is_empty());
        assert!(crate::measure_mass(&held).is_err());
        let refused = crate::boolean(&held, &held_box().0, crate::BoolOp::Union);
        assert!(refused.as_ref().is_err_and(|e| e.to_string().contains("gives no record")), "{refused:?}");
    }
}
