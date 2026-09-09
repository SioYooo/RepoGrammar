//! Contract types for future Python semantic providers such as Pyrefly and Pyright.

use crate::core::model::{
    CodeUnitId, ContentHash, FactCertainty, SemanticFact, SourceRange, TypedUnknown, UnknownClass,
    UnknownReasonCode,
};
use crate::core::policy::paths::validate_repo_relative_path;
use std::collections::BTreeMap;

const MAX_PROVIDER_METADATA_CHARS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PythonProviderKind {
    Pyrefly,
    Pyright,
    RightTyper,
}

impl PythonProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pyrefly => "pyrefly",
            Self::Pyright => "pyright",
            Self::RightTyper => "righttyper",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PythonProviderOperation {
    ResolveFrameworkIdentity,
    CrossCheckClaim,
    CallHierarchy,
    ObserveRuntimeTypes,
}

impl PythonProviderOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ResolveFrameworkIdentity => "resolve_framework_identity",
            Self::CrossCheckClaim => "cross_check_claim",
            Self::CallHierarchy => "call_hierarchy",
            Self::ObserveRuntimeTypes => "observe_runtime_types",
        }
    }

    /// True when the operation may assign at most one target to a subject.
    ///
    /// Two distinct targets for one subject then contradict each other and the
    /// answer must abstain. Call hierarchy and runtime-type observation are
    /// additive by construction, so many targets per subject are ordinary output
    /// there and must never be read as a conflict.
    pub fn is_single_valued(self) -> bool {
        matches!(self, Self::ResolveFrameworkIdentity | Self::CrossCheckClaim)
    }
}

/// Why an answered provider request yields no usable facts.
///
/// This is the port's whole abstention vocabulary: a caller that cannot map its
/// situation onto one of these states has no authority to mint facts from
/// provider output. Every state carries zero facts and no provenance, so no
/// provider failure can degrade into a confident structural claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum PythonProviderState {
    /// No provider answered: it is absent, unconfigured, or failed to run.
    Absent,
    /// A provider answered, but about source that has since changed or vanished.
    Stale,
    /// A provider answered about current source and contradicted itself.
    Conflicting,
}

impl PythonProviderState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Absent => "absent",
            Self::Stale => "stale",
            Self::Conflicting => "conflicting",
        }
    }

    fn reason(self) -> UnknownReasonCode {
        match self {
            Self::Absent => UnknownReasonCode::MissingDependency,
            Self::Stale => UnknownReasonCode::StaleEvidence,
            Self::Conflicting => UnknownReasonCode::ConflictingFacts,
        }
    }

    /// The legacy class this state projects to.
    ///
    /// Absent and stale name a mechanism that discharges them — install the
    /// provider, re-run it against the current revision — so both are
    /// recoverable. A contradiction inside one answer has no such mechanism:
    /// re-running the same provider over the same source reproduces it, and
    /// nothing else is registered to adjudicate it. Per the UNKNOWN policy an
    /// unregistered mechanism defaults to irreducible rather than to an invented
    /// provider capability.
    fn class(self) -> UnknownClass {
        match self {
            Self::Absent | Self::Stale => UnknownClass::Recoverable,
            Self::Conflicting => UnknownClass::Irreducible,
        }
    }

    fn recovery(self, provider: PythonProviderKind) -> Option<String> {
        match self {
            Self::Absent => Some(format!(
                "install or configure {} provider",
                provider.as_str()
            )),
            Self::Stale => Some(format!(
                "re-run {} provider against the current repository revision",
                provider.as_str()
            )),
            Self::Conflicting => None,
        }
    }
}

/// One provider answer as recorded by a caller, before it becomes facts.
///
/// The request carries the content hash each candidate had when the provider
/// saw it, so freshness is decided against recorded evidence rather than against
/// a caller's belief about it.
#[derive(Debug, Clone, Copy)]
pub struct PythonProviderAnswer<'a> {
    /// The request the provider answered, or would have answered.
    pub request: &'a PythonProviderRequest,
    /// Provider provenance. `None` when no provider answered at all.
    pub provenance: Option<&'a PythonProviderProvenance>,
    /// Facts the provider returned.
    pub facts: &'a [SemanticFact],
    /// The content hash each candidate has at the current revision. A candidate
    /// absent from this map no longer exists.
    pub current_hashes: &'a BTreeMap<CodeUnitId, ContentHash>,
}

/// Decide whether an answered provider request may become facts, and if not, why.
///
/// This is the single entrypoint for that decision. Callers may route, format,
/// persist, or test its result, but must not rederive provider abstention from
/// raw provenance, hash, or fact fields.
///
/// Precedence is absent, then stale, then conflicting, and that order is load-
/// bearing: a contradiction inside an answer about source that has already
/// changed says nothing about the current revision, so reporting it as a
/// conflict would attribute a disagreement to code that may no longer contain
/// one. `None` means the answer is usable; it is not a claim that the facts are
/// correct, only that this port has no reason to abstain.
pub fn classify_python_provider_answer(
    answer: &PythonProviderAnswer<'_>,
) -> Option<PythonProviderState> {
    if answer.provenance.is_none() {
        return Some(PythonProviderState::Absent);
    }
    if answer.request.candidates.iter().any(|candidate| {
        answer
            .current_hashes
            .get(&candidate.code_unit_id)
            .is_none_or(|current| *current != candidate.content_hash)
    }) {
        return Some(PythonProviderState::Stale);
    }
    if answer_contradicts_itself(answer.request.operation, answer.facts) {
        return Some(PythonProviderState::Conflicting);
    }
    None
}

fn answer_contradicts_itself(operation: PythonProviderOperation, facts: &[SemanticFact]) -> bool {
    if facts
        .iter()
        .any(|fact| fact.certainty == FactCertainty::Conflicting)
    {
        return true;
    }
    if !operation.is_single_valued() {
        return false;
    }
    let mut targets: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    for fact in facts {
        let Some(target) = fact.target.as_ref() else {
            continue;
        };
        let claim = (fact.subject.as_str(), fact.kind.as_protocol_str());
        match targets.get(&claim) {
            Some(seen) if *seen != target.as_str() => return true,
            Some(_) => {}
            None => {
                targets.insert(claim, target.as_str());
            }
        }
    }
    false
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderCandidate {
    pub code_unit_id: CodeUnitId,
    pub path: String,
    pub content_hash: ContentHash,
    pub range: SourceRange,
}

impl PythonProviderCandidate {
    pub fn new(
        code_unit_id: CodeUnitId,
        path: impl Into<String>,
        content_hash: ContentHash,
        range: SourceRange,
    ) -> Result<Self, String> {
        let path = path.into();
        validate_repo_relative_path(&path)
            .map_err(|_| "python provider candidate path must be repo-relative".to_string())?;
        Ok(Self {
            code_unit_id,
            path,
            content_hash,
            range,
        })
    }

    fn sort_key(&self) -> (&str, usize, usize, &str) {
        (
            &self.path,
            self.range.start_byte,
            self.range.end_byte,
            self.code_unit_id.as_str(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderRequest {
    pub provider: PythonProviderKind,
    pub operation: PythonProviderOperation,
    pub candidates: Vec<PythonProviderCandidate>,
    pub python_version: String,
    pub provider_config_hash: ContentHash,
    pub environment_fingerprint: String,
}

impl PythonProviderRequest {
    pub fn new(
        provider: PythonProviderKind,
        operation: PythonProviderOperation,
        candidates: impl IntoIterator<Item = PythonProviderCandidate>,
        python_version: impl Into<String>,
        provider_config_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
    ) -> Result<Self, String> {
        let mut candidates = candidates.into_iter().collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err("python provider request must include at least one candidate".to_string());
        }
        candidates.sort_by(|left, right| left.sort_key().cmp(&right.sort_key()));
        for window in candidates.windows(2) {
            if window[0].sort_key() == window[1].sort_key() {
                return Err("python provider request candidates must be unique".to_string());
            }
        }
        Ok(Self {
            provider,
            operation,
            candidates,
            python_version: validate_provider_metadata("python version", python_version)?,
            provider_config_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderProvenance {
    pub provider: PythonProviderKind,
    pub provider_version: String,
    pub python_version: String,
    pub provider_config_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: PythonProviderOperation,
}

impl PythonProviderProvenance {
    pub fn new(
        provider: PythonProviderKind,
        provider_version: impl Into<String>,
        python_version: impl Into<String>,
        provider_config_hash: ContentHash,
        environment_fingerprint: impl Into<String>,
        operation: PythonProviderOperation,
    ) -> Result<Self, String> {
        Ok(Self {
            provider,
            provider_version: validate_provider_metadata("provider version", provider_version)?,
            python_version: validate_provider_metadata("python version", python_version)?,
            provider_config_hash,
            environment_fingerprint: validate_provider_metadata(
                "environment fingerprint",
                environment_fingerprint,
            )?,
            operation,
        })
    }

    pub fn assumptions(&self) -> Vec<String> {
        vec![
            "provider_resolved=true".to_string(),
            format!("provider={}", self.provider.as_str()),
            format!("provider_version={}", self.provider_version),
            format!("python_version={}", self.python_version),
            format!(
                "provider_config_hash={}",
                self.provider_config_hash.as_str()
            ),
            format!("environment_fingerprint={}", self.environment_fingerprint),
            format!("query_operation={}", self.operation.as_str()),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderCacheKey {
    pub provider: PythonProviderKind,
    pub provider_version: String,
    pub python_version: String,
    pub provider_config_hash: ContentHash,
    pub environment_fingerprint: String,
    pub operation: PythonProviderOperation,
    pub candidates: Vec<PythonProviderCandidate>,
}

impl PythonProviderCacheKey {
    pub fn new(
        provenance: PythonProviderProvenance,
        candidates: impl IntoIterator<Item = PythonProviderCandidate>,
    ) -> Result<Self, String> {
        let request = PythonProviderRequest::new(
            provenance.provider,
            provenance.operation,
            candidates,
            provenance.python_version.clone(),
            provenance.provider_config_hash.clone(),
            provenance.environment_fingerprint.clone(),
        )?;
        Ok(Self {
            provider: provenance.provider,
            provider_version: provenance.provider_version,
            python_version: request.python_version,
            provider_config_hash: request.provider_config_hash,
            environment_fingerprint: request.environment_fingerprint,
            operation: provenance.operation,
            candidates: request.candidates,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonProviderOutput {
    pub facts: Vec<SemanticFact>,
    pub unknowns: Vec<TypedUnknown>,
    pub provenance: Option<PythonProviderProvenance>,
}

impl PythonProviderOutput {
    pub fn facts(
        provenance: PythonProviderProvenance,
        facts: Vec<SemanticFact>,
        unknowns: Vec<TypedUnknown>,
    ) -> Self {
        Self {
            facts,
            unknowns,
            provenance: Some(provenance),
        }
    }

    /// Abstain from one provider operation with the typed UNKNOWN its state
    /// requires, carrying no facts and no provenance.
    ///
    /// Every state blocks the same claim, because what is unavailable is the
    /// same conclusion in all three cases; only the reason code, class, and
    /// recovery differ. That keeps a stale or contradicted answer from being
    /// counted as a different obligation than a missing provider.
    pub fn abstained(
        state: PythonProviderState,
        provider: PythonProviderKind,
        operation: PythonProviderOperation,
    ) -> Self {
        Self {
            facts: Vec::new(),
            unknowns: vec![TypedUnknown::new(
                state.class(),
                state.reason(),
                format!(
                    "python_provider:{}:{}",
                    provider.as_str(),
                    operation.as_str()
                ),
                state.recovery(provider),
            )
            .expect("provider abstention UNKNOWN uses non-empty fields")],
            provenance: None,
        }
    }

    pub fn unavailable(provider: PythonProviderKind, operation: PythonProviderOperation) -> Self {
        Self::abstained(PythonProviderState::Absent, provider, operation)
    }
}

fn validate_provider_metadata(
    field: &'static str,
    value: impl Into<String>,
) -> Result<String, String> {
    let value = value.into();
    if value.trim().is_empty() {
        return Err(format!("{field} must not be empty"));
    }
    if value.len() > MAX_PROVIDER_METADATA_CHARS || value.chars().any(char::is_control) {
        return Err(format!("{field} must be sanitized metadata"));
    }
    if value.contains('/') || value.contains('\\') || value.contains("://") {
        return Err(format!("{field} must not contain path-like text"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{
        Evidence, FactOrigin, Provenance, RepositoryRevision, SemanticFactKind, SymbolId,
    };

    fn hash(character: char) -> ContentHash {
        ContentHash::new(format!("sha256:{}", character.to_string().repeat(64)))
            .expect("valid content hash")
    }

    fn candidate(path: &str, start: usize) -> PythonProviderCandidate {
        PythonProviderCandidate::new(
            CodeUnitId::new(format!("unit:{path}:{start}")).expect("valid code unit id"),
            path,
            hash('a'),
            SourceRange::new(start, start + 10).expect("valid range"),
        )
        .expect("valid provider candidate")
    }

    fn request(operation: PythonProviderOperation) -> PythonProviderRequest {
        PythonProviderRequest::new(
            PythonProviderKind::Pyrefly,
            operation,
            [candidate("src/a.py", 0)],
            "3.12.6",
            hash('b'),
            "env-sha256-abc",
        )
        .expect("valid provider request")
    }

    fn provenance(operation: PythonProviderOperation) -> PythonProviderProvenance {
        PythonProviderProvenance::new(
            PythonProviderKind::Pyrefly,
            "0.1.0",
            "3.12.6",
            hash('b'),
            "env-sha256-abc",
            operation,
        )
        .expect("valid provenance")
    }

    /// The hashes the request's candidates still carry at the current revision.
    fn unchanged_hashes(request: &PythonProviderRequest) -> BTreeMap<CodeUnitId, ContentHash> {
        request
            .candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.code_unit_id.clone(),
                    candidate.content_hash.clone(),
                )
            })
            .collect()
    }

    fn fact(subject: &str, target: &str, certainty: FactCertainty) -> SemanticFact {
        SemanticFact {
            kind: SemanticFactKind::FrameworkRole,
            subject: subject.to_string(),
            target: Some(SymbolId::new(target).expect("valid symbol id")),
            origin: FactOrigin {
                engine: "test-provider".to_string(),
                engine_version: "0.1.0".to_string(),
                method: "fixture".to_string(),
            },
            certainty,
            evidence: Evidence::new(
                CodeUnitId::new("unit:src/a.py:0").expect("valid code unit id"),
                SourceRange::new(0, 10).expect("valid range"),
                Provenance::new(
                    "src/a.py",
                    hash('a'),
                    RepositoryRevision::new("0".repeat(40)).expect("valid revision"),
                )
                .expect("valid provenance"),
                "provider fixture fact",
            )
            .expect("valid evidence"),
            assumptions: Vec::new(),
        }
    }

    fn classify(
        request: &PythonProviderRequest,
        provenance: Option<&PythonProviderProvenance>,
        facts: &[SemanticFact],
        current_hashes: &BTreeMap<CodeUnitId, ContentHash>,
    ) -> Option<PythonProviderState> {
        classify_python_provider_answer(&PythonProviderAnswer {
            request,
            provenance,
            facts,
            current_hashes,
        })
    }

    #[test]
    fn fresh_consistent_answer_gives_the_port_no_reason_to_abstain() {
        let request = request(PythonProviderOperation::ResolveFrameworkIdentity);
        let provenance = provenance(PythonProviderOperation::ResolveFrameworkIdentity);
        let facts = [fact(
            "app.get_user",
            "framework:fastapi.route",
            FactCertainty::Semantic,
        )];

        assert_eq!(
            classify(
                &request,
                Some(&provenance),
                &facts,
                &unchanged_hashes(&request)
            ),
            None
        );
    }

    #[test]
    fn an_unanswered_request_is_absent_even_when_every_candidate_is_current() {
        let request = request(PythonProviderOperation::ResolveFrameworkIdentity);

        assert_eq!(
            classify(&request, None, &[], &unchanged_hashes(&request)),
            Some(PythonProviderState::Absent)
        );
    }

    #[test]
    fn changed_or_deleted_candidate_source_makes_an_answer_stale() {
        let request = request(PythonProviderOperation::ResolveFrameworkIdentity);
        let provenance = provenance(PythonProviderOperation::ResolveFrameworkIdentity);
        let facts = [fact(
            "app.get_user",
            "framework:fastapi.route",
            FactCertainty::Semantic,
        )];

        let mut rewritten = unchanged_hashes(&request);
        for value in rewritten.values_mut() {
            *value = hash('f');
        }
        assert_eq!(
            classify(&request, Some(&provenance), &facts, &rewritten),
            Some(PythonProviderState::Stale),
            "a candidate whose content hash moved was answered about different source"
        );

        assert_eq!(
            classify(&request, Some(&provenance), &facts, &BTreeMap::new()),
            Some(PythonProviderState::Stale),
            "a candidate that no longer exists cannot be current"
        );
    }

    #[test]
    fn a_self_contradicting_answer_about_current_source_is_conflicting() {
        let request = request(PythonProviderOperation::ResolveFrameworkIdentity);
        let provenance = provenance(PythonProviderOperation::ResolveFrameworkIdentity);
        let current = unchanged_hashes(&request);

        let two_targets = [
            fact(
                "app.get_user",
                "framework:fastapi.route",
                FactCertainty::Semantic,
            ),
            fact(
                "app.get_user",
                "framework:flask.route",
                FactCertainty::Semantic,
            ),
        ];
        assert_eq!(
            classify(&request, Some(&provenance), &two_targets, &current),
            Some(PythonProviderState::Conflicting),
            "one single-valued subject cannot hold two different targets"
        );

        let self_declared = [fact(
            "app.get_user",
            "framework:fastapi.route",
            FactCertainty::Conflicting,
        )];
        assert_eq!(
            classify(&request, Some(&provenance), &self_declared, &current),
            Some(PythonProviderState::Conflicting),
            "a provider that reports its own fact as conflicting is believed"
        );
    }

    #[test]
    fn repeating_one_target_for_a_subject_is_not_a_contradiction() {
        let request = request(PythonProviderOperation::CrossCheckClaim);
        let provenance = provenance(PythonProviderOperation::CrossCheckClaim);
        let facts = [
            fact(
                "app.get_user",
                "framework:fastapi.route",
                FactCertainty::Semantic,
            ),
            fact(
                "app.get_user",
                "framework:fastapi.route",
                FactCertainty::Semantic,
            ),
        ];

        assert_eq!(
            classify(
                &request,
                Some(&provenance),
                &facts,
                &unchanged_hashes(&request)
            ),
            None
        );
    }

    #[test]
    fn additive_operations_admit_many_targets_for_one_subject() {
        for operation in [
            PythonProviderOperation::CallHierarchy,
            PythonProviderOperation::ObserveRuntimeTypes,
        ] {
            assert!(!operation.is_single_valued(), "{operation:?}");
            let request = request(operation);
            let provenance = provenance(operation);
            let facts = [
                fact("app.get_user", "app.load_user", FactCertainty::Semantic),
                fact("app.get_user", "app.audit_access", FactCertainty::Semantic),
            ];

            assert_eq!(
                classify(
                    &request,
                    Some(&provenance),
                    &facts,
                    &unchanged_hashes(&request)
                ),
                None,
                "{operation:?} produces many targets per subject by construction"
            );
        }
    }

    #[test]
    fn staleness_outranks_a_contradiction_it_cannot_localize() {
        let request = request(PythonProviderOperation::ResolveFrameworkIdentity);
        let provenance = provenance(PythonProviderOperation::ResolveFrameworkIdentity);
        let facts = [
            fact(
                "app.get_user",
                "framework:fastapi.route",
                FactCertainty::Semantic,
            ),
            fact(
                "app.get_user",
                "framework:flask.route",
                FactCertainty::Semantic,
            ),
        ];

        assert_eq!(
            classify(&request, Some(&provenance), &facts, &BTreeMap::new()),
            Some(PythonProviderState::Stale),
            "a disagreement about vanished source is not a claim about the current revision"
        );
        assert_eq!(
            classify(&request, None, &facts, &BTreeMap::new()),
            Some(PythonProviderState::Absent),
            "with no provider answer there is nothing to call stale or conflicting"
        );
    }

    #[test]
    fn every_abstention_blocks_the_same_claim_with_its_own_reason_and_recovery() {
        let expected = [
            (
                PythonProviderState::Absent,
                UnknownClass::Recoverable,
                UnknownReasonCode::MissingDependency,
                Some("install or configure pyrefly provider"),
            ),
            (
                PythonProviderState::Stale,
                UnknownClass::Recoverable,
                UnknownReasonCode::StaleEvidence,
                Some("re-run pyrefly provider against the current repository revision"),
            ),
            (
                PythonProviderState::Conflicting,
                UnknownClass::Irreducible,
                UnknownReasonCode::ConflictingFacts,
                None,
            ),
        ];

        for (state, class, reason, recovery) in expected {
            let output = PythonProviderOutput::abstained(
                state,
                PythonProviderKind::Pyrefly,
                PythonProviderOperation::ResolveFrameworkIdentity,
            );

            assert!(output.facts.is_empty(), "{state:?} must carry no facts");
            assert!(
                output.provenance.is_none(),
                "{state:?} must carry no provenance"
            );
            assert_eq!(output.unknowns.len(), 1, "{state:?}");
            assert_eq!(output.unknowns[0].class, class, "{state:?}");
            assert_eq!(output.unknowns[0].reason, reason, "{state:?}");
            assert_eq!(
                output.unknowns[0].affected_claim,
                "python_provider:pyrefly:resolve_framework_identity",
                "every state blocks the same claim"
            );
            assert_eq!(
                output.unknowns[0].recovery.as_deref(),
                recovery,
                "{state:?}"
            );
        }
    }

    #[test]
    fn provider_states_use_stable_tokens() {
        assert_eq!(PythonProviderState::Absent.as_str(), "absent");
        assert_eq!(PythonProviderState::Stale.as_str(), "stale");
        assert_eq!(PythonProviderState::Conflicting.as_str(), "conflicting");
    }

    #[test]
    fn provider_request_validates_and_sorts_candidate_scope() {
        let request = PythonProviderRequest::new(
            PythonProviderKind::Pyrefly,
            PythonProviderOperation::ResolveFrameworkIdentity,
            [candidate("src/b.py", 10), candidate("src/a.py", 0)],
            "3.12.6",
            hash('b'),
            "env-sha256-abc",
        )
        .expect("valid provider request");

        assert_eq!(request.candidates[0].path, "src/a.py");
        assert_eq!(request.candidates[1].path, "src/b.py");
        assert_eq!(request.provider.as_str(), "pyrefly");
        assert_eq!(request.operation.as_str(), "resolve_framework_identity");
    }

    #[test]
    fn provider_request_rejects_unsafe_or_duplicate_candidate_scope() {
        assert!(PythonProviderCandidate::new(
            CodeUnitId::new("unit").expect("unit"),
            "../secret.py",
            hash('a'),
            SourceRange::new(0, 1).expect("range"),
        )
        .is_err());

        let duplicate = candidate("src/a.py", 0);
        assert!(PythonProviderRequest::new(
            PythonProviderKind::Pyrefly,
            PythonProviderOperation::ResolveFrameworkIdentity,
            [duplicate.clone(), duplicate],
            "3.12.6",
            hash('b'),
            "env-sha256-abc",
        )
        .is_err());
    }

    #[test]
    fn provider_provenance_and_cache_key_record_required_dimensions() {
        let provenance = PythonProviderProvenance::new(
            PythonProviderKind::Pyright,
            "1.1.400",
            "3.12.6",
            hash('c'),
            "env-sha256-def",
            PythonProviderOperation::CrossCheckClaim,
        )
        .expect("valid provenance");

        let assumptions = provenance.assumptions();
        assert!(assumptions.contains(&"provider_resolved=true".to_string()));
        assert!(assumptions.contains(&"provider=pyright".to_string()));
        assert!(assumptions.contains(&format!("provider_config_hash={}", hash('c').as_str())));
        assert!(!assumptions.iter().any(|value| value.contains("src/a.py")));

        let cache_key =
            PythonProviderCacheKey::new(provenance, [candidate("src/a.py", 8)]).expect("cache key");
        assert_eq!(cache_key.provider, PythonProviderKind::Pyright);
        assert_eq!(cache_key.provider_version, "1.1.400");
        assert_eq!(cache_key.candidates[0].range.start_byte, 8);
    }

    #[test]
    fn provider_metadata_rejects_blank_path_like_and_control_text() {
        for value in [
            "",
            "   ",
            "/tmp/pyrefly",
            "file://env",
            "env\\fingerprint",
            "env\nx",
        ] {
            assert!(
                PythonProviderProvenance::new(
                    PythonProviderKind::Pyrefly,
                    value,
                    "3.12.6",
                    hash('c'),
                    "env-sha256-def",
                    PythonProviderOperation::ResolveFrameworkIdentity,
                )
                .is_err(),
                "provider version should reject {value:?}"
            );
        }
    }

    #[test]
    fn unavailable_provider_becomes_recoverable_unknown_without_facts() {
        let output = PythonProviderOutput::unavailable(
            PythonProviderKind::Pyrefly,
            PythonProviderOperation::ResolveFrameworkIdentity,
        );

        assert!(output.facts.is_empty());
        assert!(output.provenance.is_none());
        assert_eq!(output.unknowns.len(), 1);
        assert_eq!(output.unknowns[0].class, UnknownClass::Recoverable);
        assert_eq!(
            output.unknowns[0].reason,
            UnknownReasonCode::MissingDependency
        );
        assert_eq!(
            output.unknowns[0].affected_claim,
            "python_provider:pyrefly:resolve_framework_identity"
        );
        assert_eq!(
            output,
            PythonProviderOutput::abstained(
                PythonProviderState::Absent,
                PythonProviderKind::Pyrefly,
                PythonProviderOperation::ResolveFrameworkIdentity,
            ),
            "the legacy constructor must stay one spelling of the absent state"
        );
    }
}
