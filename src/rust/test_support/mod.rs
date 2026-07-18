//! Shared deterministic helpers for tests.

use crate::core::model::{
    FamilyConstraintProfile, FamilyPrevalence, FeatureConstraint, FeatureConstraintOrigin,
    FeatureConstraintSemantics, TypedUnknown, UnknownClass, UnknownObligation, UnknownReasonCode,
    VariationConstraint,
};
use crate::ports::family_store::{
    GenerationWriteSession, IndexedFamilyConstraintProfileRecord, IndexedFamilyEvidenceRecord,
    IndexedFamilyMemberRecord, IndexedFamilyRecord, IndexedVariationSlotRecord, StoreError,
    WriteSessionStats,
};
use crate::ports::index_store::{
    GenerationHandle, IndexStoreError, IndexedCodeUnitRecord, IndexedFileRecord,
    IndexedIrEdgeRecord, IndexedIrNodeRecord, IndexedSemanticFactRecord,
};
use std::cell::RefCell;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;
use std::rc::Rc;

/// A deterministic in-memory [`GenerationWriteSession`] fake for pipeline and
/// application-boundary tests. It records nothing to a database: each record call
/// appends the target generation id to a shared log and returns `Ok`, unless an
/// indexed-file failure message is configured, in which case `record_indexed_file`
/// returns that typed error. This lets tests drive the session-based storage
/// wrappers (which keep the application-boundary validation) and observe write
/// ordering or inject a mid-build record failure without a real store.
pub struct FakeWriteSession {
    generation: GenerationHandle,
    log: Rc<RefCell<Vec<String>>>,
    fail_indexed_file_message: Option<String>,
    rows_written: usize,
    transactions: usize,
    checkpoints: usize,
}

impl FakeWriteSession {
    pub fn new(generation: GenerationHandle) -> Self {
        Self::with_log(generation, Rc::new(RefCell::new(Vec::new())))
    }

    /// Build a session that shares its recorded-generation log with the caller,
    /// so a fake store can hand out sessions and later assert what was written.
    pub fn with_log(generation: GenerationHandle, log: Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            generation,
            log,
            fail_indexed_file_message: None,
            rows_written: 0,
            transactions: 0,
            checkpoints: 0,
        }
    }

    /// Configure `record_indexed_file` to fail with the given message.
    pub fn failing_indexed_file(mut self, message: &str) -> Self {
        self.fail_indexed_file_message = Some(message.to_string());
        self
    }

    fn note(&mut self) {
        self.log
            .borrow_mut()
            .push(self.generation.generation_id.clone());
        self.rows_written += 1;
    }
}

impl GenerationWriteSession for FakeWriteSession {
    fn generation(&self) -> &GenerationHandle {
        &self.generation
    }

    fn record_indexed_file(&mut self, _file: &IndexedFileRecord) -> Result<(), IndexStoreError> {
        if let Some(message) = &self.fail_indexed_file_message {
            return Err(IndexStoreError::InvalidRecord(message.clone()));
        }
        self.note();
        Ok(())
    }

    fn remove_indexed_file(&mut self, _path: &str) -> Result<(), IndexStoreError> {
        self.note();
        Ok(())
    }

    fn record_code_unit(&mut self, _unit: &IndexedCodeUnitRecord) -> Result<(), IndexStoreError> {
        self.note();
        Ok(())
    }

    fn record_ir_node(&mut self, _node: &IndexedIrNodeRecord) -> Result<(), IndexStoreError> {
        self.note();
        Ok(())
    }

    fn record_ir_edge(&mut self, _edge: &IndexedIrEdgeRecord) -> Result<(), IndexStoreError> {
        self.note();
        Ok(())
    }

    fn record_semantic_fact(
        &mut self,
        _fact: &IndexedSemanticFactRecord,
    ) -> Result<(), IndexStoreError> {
        self.note();
        Ok(())
    }

    fn record_family(&mut self, _family: &IndexedFamilyRecord) -> Result<(), StoreError> {
        self.note();
        Ok(())
    }

    fn record_family_member(
        &mut self,
        _member: &IndexedFamilyMemberRecord,
    ) -> Result<(), StoreError> {
        self.note();
        Ok(())
    }

    fn record_variation_slot(
        &mut self,
        _slot: &IndexedVariationSlotRecord,
    ) -> Result<(), StoreError> {
        self.note();
        Ok(())
    }

    fn record_family_evidence(
        &mut self,
        _evidence: &IndexedFamilyEvidenceRecord,
    ) -> Result<(), StoreError> {
        self.note();
        Ok(())
    }

    fn record_family_constraint_profile(
        &mut self,
        _record: &IndexedFamilyConstraintProfileRecord,
    ) -> Result<(), StoreError> {
        self.note();
        Ok(())
    }

    fn record_python_module_interface(
        &mut self,
        _path: &str,
        _interface_hash: &str,
    ) -> Result<(), IndexStoreError> {
        self.note();
        Ok(())
    }

    fn checkpoint(&mut self) -> Result<(), IndexStoreError> {
        self.transactions += 1;
        self.checkpoints += 1;
        Ok(())
    }

    fn finish(&mut self) -> Result<(), IndexStoreError> {
        self.transactions += 1;
        Ok(())
    }

    fn abandon(&mut self) -> Result<(), IndexStoreError> {
        Ok(())
    }

    fn stats(&self) -> WriteSessionStats {
        WriteSessionStats {
            transactions: self.transactions,
            rows_written: self.rows_written,
            checkpoints: self.checkpoints,
        }
    }
}

/// A deterministic dominant-shaped [`FamilyPrevalence`] for tests that need a
/// prevalence value but do not exercise the classifier itself.
pub fn sample_family_prevalence() -> FamilyPrevalence {
    FamilyPrevalence {
        eligible_peer_count: 2,
        supported_member_count: 2,
        coverage_ratio: Some(1.0),
        competing_ready_family_count: 0,
        largest_competing_support: 0,
        blocked_peer_count: 0,
        unsupported_peer_count: 0,
        classification_reason: "coverage 2/2 with no competing ready family".to_string(),
    }
}

/// A deterministic fastapi-shaped [`FamilyConstraintProfile`] for tests that need
/// a profile value but do not exercise the derivation itself. It deliberately
/// covers every constraint kind so persistence round-trips are not limited to
/// `Equal` semantics: the framework-role identity (`Equal`), the support-family
/// core (`MustContain`, `SupportFamilyIntersection`), a non-empty characteristic
/// (`Equal`), an empty-set characteristic (`EqualEmpty`), a prohibited-presence
/// blocker (`ProhibitedPresence`, `IncompatibilityBlocker`), one observed-only
/// variation, and the always-present runtime-equivalence obligation.
pub fn sample_family_constraint_profile() -> FamilyConstraintProfile {
    let obligation: UnknownObligation = TypedUnknown {
        class: UnknownClass::NonBlocking,
        reason: UnknownReasonCode::FrameworkMagic,
        affected_claim: "family:example:runtime_equivalence".to_string(),
        recovery: Some("add semantic-worker or framework adapter evidence".to_string()),
    };
    FamilyConstraintProfile {
        required_equal_features: vec![
            FeatureConstraint {
                prefix: "framework_role:".to_string(),
                values: vec!["framework_fastapi_route".to_string()],
                origin: FeatureConstraintOrigin::FrameworkRoleIdentity,
                semantics: FeatureConstraintSemantics::Equal,
            },
            FeatureConstraint {
                prefix: "support_family:".to_string(),
                values: vec!["fastapi_route_decorator".to_string()],
                origin: FeatureConstraintOrigin::SupportFamilyIntersection,
                semantics: FeatureConstraintSemantics::MustContain,
            },
            FeatureConstraint {
                prefix: "decorator_shape:".to_string(),
                values: vec!["fastapi_route_decorator".to_string()],
                origin: FeatureConstraintOrigin::CharacteristicProfile,
                semantics: FeatureConstraintSemantics::Equal,
            },
            FeatureConstraint {
                prefix: "effect_marker:".to_string(),
                values: Vec::new(),
                origin: FeatureConstraintOrigin::CharacteristicProfile,
                semantics: FeatureConstraintSemantics::EqualEmpty,
            },
        ],
        allowed_variations: vec![VariationConstraint {
            dimension: "python_import_context".to_string(),
            observed_profiles: vec!["alpha".to_string(), "beta".to_string()],
            observed_profiles_truncated: false,
            includes_absent_profile: false,
            representative_member_ids: vec![
                "unit:src/a.py#function:0-1".to_string(),
                "unit:src/b.py#function:0-1".to_string(),
            ],
            observed_only: true,
        }],
        prohibited_or_blocking_features: vec![FeatureConstraint {
            prefix: "unknown_blocker:".to_string(),
            values: Vec::new(),
            origin: FeatureConstraintOrigin::IncompatibilityBlocker,
            semantics: FeatureConstraintSemantics::ProhibitedPresence,
        }],
        unresolved_obligations: vec![obligation],
    }
}

#[derive(Debug)]
pub struct TempWorkspace {
    path: PathBuf,
}

impl TempWorkspace {
    pub fn new(prefix: &str) -> Self {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "repogrammar-{prefix}-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&path).expect("create temp workspace");
        Self { path }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempWorkspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time after unix epoch")
        .as_nanos()
}

pub fn create_test_symlink_file(target: &Path, link: &Path) -> bool {
    create_test_symlink(
        || {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(target, link)
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_file(target, link)
            }
        },
        "file",
        target,
        link,
    )
}

pub fn create_test_symlink_dir(target: &Path, link: &Path) -> bool {
    create_test_symlink(
        || {
            #[cfg(unix)]
            {
                std::os::unix::fs::symlink(target, link)
            }
            #[cfg(windows)]
            {
                std::os::windows::fs::symlink_dir(target, link)
            }
        },
        "directory",
        target,
        link,
    )
}

fn create_test_symlink(
    create: impl FnOnce() -> io::Result<()>,
    kind: &str,
    target: &Path,
    link: &Path,
) -> bool {
    match create() {
        Ok(()) => true,
        Err(error) if symlink_creation_unavailable(&error) => false,
        Err(error) => panic!(
            "create {kind} symlink from {} to {}: {error}",
            link.display(),
            target.display()
        ),
    }
}

fn symlink_creation_unavailable(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        const ERROR_NOT_SUPPORTED: i32 = 50;
        const ERROR_PRIVILEGE_NOT_HELD: i32 = 1314;
        matches!(
            error.raw_os_error(),
            Some(ERROR_NOT_SUPPORTED | ERROR_PRIVILEGE_NOT_HELD)
        ) || error.kind() == io::ErrorKind::Unsupported
    }
    #[cfg(not(windows))]
    {
        error.kind() == io::ErrorKind::Unsupported
    }
}
