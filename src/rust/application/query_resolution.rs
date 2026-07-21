//! Shared, deterministic, source-free target-resolution vocabulary.
//!
//! RepoGrammar resolves a raw query/check target through several call sites that
//! historically each re-derived the same raw-target facts (path locators,
//! `unit:`/`family:` handles, path-locator shape, identifier tokens) from the
//! same low-level primitives. This module introduces **one authority** for
//! parsing a raw target into a typed intermediate representation
//! ([`TargetConstraints`]) so those facts have a single, testable source of
//! truth.
//!
//! Phase 1 is a **behavior-preserving extraction**: [`parse_target`] REUSES the
//! existing locator primitives in [`crate::application::query`] and the
//! committed retrieval vocabulary in [`crate::application::query_terms`]. It
//! never reimplements path/locator parsing, performs no I/O, reads no source,
//! and is deterministic and total (any input yields a value and never panics).
//!
//! The IR separates three orthogonal constraint tiers:
//!
//! * **HARD** ([`HardConstraints`]) — exact identity locks (`family:`/`unit:`
//!   ids and file path / `path:line` / `path:start-end` locators).
//! * **SCOPE** ([`ScopeConstraints`]) — filters that narrow where a match may
//!   live (directory prefixes, language). Parsed and represented here; acting on
//!   directory scope during retrieval is a later phase.
//! * **RANKING** ([`RankingSignals`]) — soft signals that order candidates but
//!   never pin identity (identifier/symbol hints, framework role, pattern
//!   concept, natural-language residue).
//!
//! Consuming operations are intentionally out of scope for Phase 1: the parser
//! captures the reserved [`TargetConstraints::within`] / [`TargetConstraints::against`]
//! scopes and any [`TargetConflict`] without resolving them.

use std::collections::BTreeSet;

use crate::application::query::{
    is_safe_query_path_text, split_query_path_locator, target_has_path_locator_shape,
    target_identifier_tokens, PATH_LOCATOR_EXTENSIONS,
};
use crate::application::query_terms::{normalize_query, Concept, MAX_QUERY_TOKENS};

/// A single file path locator parsed from one whitespace token: the path text
/// with the locator suffix stripped, plus an optional 1-based line locator
/// (`path:line`) or byte range (`path:start-end`), exactly as
/// [`split_query_path_locator`] produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathLocator {
    /// Path text with any trailing `:line` / `:start-end` locator removed.
    pub path: String,
    /// 1-based line locator, when the token carried `path:line`.
    pub line: Option<usize>,
    /// Byte range `(start, end)`, when the token carried `path:start-end`.
    pub byte_range: Option<(usize, usize)>,
}

/// HARD constraints: exact identity locks that pin a specific locus. Empty when
/// the target carries no exact identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HardConstraints {
    /// Exact pattern-family id (`family:` prefixed handle). `None` when absent or
    /// when the target names more than one distinct family id (a conflict is
    /// recorded instead of resolving a winner).
    pub family_id: Option<String>,
    /// Exact code-unit member id (`unit:` prefixed handle). `None` when absent or
    /// when the target names more than one distinct unit id (a conflict is
    /// recorded instead of resolving a winner).
    pub unit_id: Option<String>,
    /// File path / `path:line` / `path:start-end` locators, in first-seen order.
    pub path_locators: Vec<PathLocator>,
}

/// SCOPE constraints: filters that narrow where a match may live. Parsed and
/// represented in Phase 1; directory-scope retrieval is a later phase, so no
/// operation acts on these fields yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScopeConstraints {
    /// Directory prefixes named as `/`-containing tokens that are not file
    /// locators (e.g. `app/api`). First-seen order.
    pub directory_prefixes: Vec<String>,
    /// Canonical language filter tokens named by the query (e.g. `python`),
    /// reusing the committed language vocabulary.
    pub languages: BTreeSet<String>,
}

/// RANKING signals: soft hints that order candidates but never pin identity.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RankingSignals {
    /// Identifier-like tokens (from the shared identifier tokenizer), in
    /// first-seen order. The local-context resolver consumes these as its
    /// candidate symbol/residue terms.
    pub identifier_tokens: Vec<String>,
    /// Framework filter tokens named by the query (e.g. `fastapi`).
    pub framework_roles: BTreeSet<String>,
    /// Pattern-concept tokens named by a single alias.
    pub concepts: BTreeSet<Concept>,
    /// Pattern-concept tokens named by a committed two-term phrase.
    pub qualified_concepts: BTreeSet<Concept>,
    /// Natural-language residue tokens after committed vocabulary was consumed.
    pub residue_terms: BTreeSet<String>,
}

/// Coarse classification of the dominant form of a raw target. Deterministic and
/// derived from the parsed tiers; never consumed by a Phase 1 decision path.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TargetForm {
    /// A single-token exact `family:` id.
    FamilyId,
    /// A single-token exact `unit:` member id.
    UnitId,
    /// One or more file path / `path:line` / `path:start-end` locators.
    PathLocator,
    /// A directory-prefix scope with no file locator.
    DirectoryScope,
    /// No hard locator: natural-language / role / concept prose.
    #[default]
    NaturalLanguage,
}

/// How many loci a target is expected to resolve to. Defined now for later
/// resolution/projection phases; Phase 1 only derives it from [`TargetForm`] and
/// exercises it in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionCardinality {
    /// No resolvable locus (e.g. an unresolved conflict).
    None,
    /// Exactly one locus.
    One,
    /// More than one candidate locus.
    Many,
    /// Candidates exist but the set was truncated at a cap. Produced by later
    /// retrieval phases, not by [`parse_target`].
    Truncated,
}

/// A typed conflict detected while parsing hard constraints. Represented, never
/// resolved, in Phase 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TargetConflict {
    /// More than one distinct `unit:` member id appeared in the target.
    MultipleUnitIds(Vec<String>),
    /// More than one distinct `family:` id appeared in the target.
    MultipleFamilyIds(Vec<String>),
    /// Both an exact `family:` id and an exact `unit:` id appeared; they pin
    /// different identity kinds.
    MixedFamilyAndUnitId,
}

/// The typed, orthogonal, source-free result of parsing one raw target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TargetConstraints {
    /// The verbatim raw target, exactly as passed to [`parse_target`].
    pub original: String,
    /// The dominant classified form.
    pub form: TargetForm,
    /// Exact identity locks.
    pub hard: HardConstraints,
    /// Narrowing filters (represented, not yet acted upon).
    pub scope: ScopeConstraints,
    /// Soft ranking signals.
    pub ranking: RankingSignals,
    /// Whether the raw target is path-locator-shaped, computed by the shared
    /// [`target_has_path_locator_shape`] authority. Consumed by the term-retrieval
    /// guard to keep path-shaped targets on the exact/local-context path.
    pub path_locator_shaped: bool,
    /// Reserved `within` scope: parsed and captured, not yet consumed.
    pub within: Option<String>,
    /// Reserved `against` comparison scope: parsed and captured, not yet consumed.
    pub against: Option<String>,
    /// Typed hard-constraint conflicts, represented without resolution.
    pub conflicts: Vec<TargetConflict>,
}

impl TargetConstraints {
    /// True when the raw target is `unit:`-prefixed. Mirrors the exact-authority
    /// `starts_with("unit:")` check the fuzzy family lookup performs on the whole
    /// target, independent of whether a single exact [`HardConstraints::unit_id`]
    /// was extracted (a multi-id target is still `unit:`-prefixed).
    pub fn is_unit_prefixed(&self) -> bool {
        self.original.starts_with("unit:")
    }

    /// The cardinality a resolver should expect for this target, derived from the
    /// classified form. An unresolved conflict yields [`ResolutionCardinality::None`].
    pub fn expected_cardinality(&self) -> ResolutionCardinality {
        if !self.conflicts.is_empty() {
            return ResolutionCardinality::None;
        }
        match self.form {
            TargetForm::FamilyId | TargetForm::UnitId => ResolutionCardinality::One,
            TargetForm::PathLocator => {
                if self.hard.path_locators.len() == 1 {
                    ResolutionCardinality::One
                } else {
                    ResolutionCardinality::Many
                }
            }
            TargetForm::DirectoryScope | TargetForm::NaturalLanguage => ResolutionCardinality::Many,
        }
    }
}

/// The single authoritative parser for a raw target string.
///
/// Deterministic, total, source-free, and I/O-free. It composes the existing
/// locator primitives and committed retrieval vocabulary into a
/// [`TargetConstraints`] IR; it does not reimplement any path/locator parsing.
/// The optional `within`/`against` scopes are captured verbatim for later
/// phases.
pub fn parse_target(raw: &str, within: Option<&str>, against: Option<&str>) -> TargetConstraints {
    let mut hard = HardConstraints::default();
    let mut scope = ScopeConstraints::default();
    let mut conflicts = Vec::new();

    let mut family_ids: Vec<String> = Vec::new();
    let mut unit_ids: Vec<String> = Vec::new();

    // Bounded token scan mirrors the retrieval normalizer's bound so pathological
    // input cannot unbound this parser. Path/locator facts reuse the shared
    // primitives; only whole-target facts feed the wired call sites.
    for token in raw.split_whitespace().take(MAX_QUERY_TOKENS) {
        if token.starts_with("family:") {
            push_distinct(&mut family_ids, token);
        } else if token.starts_with("unit:") {
            push_distinct(&mut unit_ids, token);
        } else {
            let (path_text, locator) = split_query_path_locator(token);
            if locator.is_some() || extension_is_known(path_text) {
                hard.path_locators.push(PathLocator {
                    path: path_text.to_string(),
                    line: locator.and_then(|locator| locator.line),
                    byte_range: locator.and_then(|locator| locator.byte_range),
                });
            } else if path_text.contains('/') {
                push_distinct(&mut scope.directory_prefixes, path_text);
            }
        }
    }

    // Never resolve a conflict here: leave the exact id `None` and record it.
    if family_ids.len() > 1 {
        conflicts.push(TargetConflict::MultipleFamilyIds(family_ids.clone()));
    } else {
        hard.family_id = family_ids.first().cloned();
    }
    if unit_ids.len() > 1 {
        conflicts.push(TargetConflict::MultipleUnitIds(unit_ids.clone()));
    } else {
        hard.unit_id = unit_ids.first().cloned();
    }
    if !family_ids.is_empty() && !unit_ids.is_empty() {
        conflicts.push(TargetConflict::MixedFamilyAndUnitId);
    }

    // RANKING + language scope reuse the committed retrieval vocabulary verbatim.
    let normalized = normalize_query(raw);
    scope.languages = normalized.language_filters;
    let ranking = RankingSignals {
        identifier_tokens: target_identifier_tokens(raw)
            .into_iter()
            .map(str::to_string)
            .collect(),
        framework_roles: normalized.framework_filters,
        concepts: normalized.concept_tokens,
        qualified_concepts: normalized.qualified_concept_tokens,
        residue_terms: normalized.residue_terms,
    };

    let trimmed = raw.trim();
    let form = if family_ids.len() == 1 && trimmed == family_ids[0] {
        TargetForm::FamilyId
    } else if unit_ids.len() == 1 && trimmed == unit_ids[0] {
        TargetForm::UnitId
    } else if !hard.path_locators.is_empty() {
        TargetForm::PathLocator
    } else if !scope.directory_prefixes.is_empty() {
        TargetForm::DirectoryScope
    } else {
        TargetForm::NaturalLanguage
    };

    TargetConstraints {
        original: raw.to_string(),
        form,
        hard,
        scope,
        ranking,
        path_locator_shaped: target_has_path_locator_shape(raw),
        within: within.map(str::to_string),
        against: against.map(str::to_string),
        conflicts,
    }
}

/// Normalize a directory-scope prefix into a safe, canonical repo-relative form,
/// or `None` when it is unsafe to read.
///
/// Deterministic, source-free, and I/O-free. It strips leading `./` segments and
/// any trailing slash, then applies the shared [`is_safe_query_path_text`]
/// authority as the safety gate. Reusing that authority (rather than collapsing
/// first, which would hide the signal) is what rejects an absolute path, a `..`
/// traversal, a backslash, a scheme (`://`), a control character, or an empty
/// segment (including a redundant `//`). A prefix that returns `None` must never
/// be used to read a directory scope.
pub fn normalize_directory_prefix(raw: &str) -> Option<String> {
    let mut candidate = raw.trim();
    // Strip a leading `./` (possibly repeated) without touching a leading `/`,
    // `..`, backslash, or scheme, so the safety gate below still sees those.
    while let Some(rest) = candidate.strip_prefix("./") {
        candidate = rest;
    }
    let candidate = candidate.trim_end_matches('/');
    if candidate.is_empty() || !is_safe_query_path_text(candidate) {
        return None;
    }
    Some(candidate.to_string())
}

/// Append `token` to `sink` if not already present, preserving first-seen order.
fn push_distinct(sink: &mut Vec<String>, token: &str) {
    if !sink.iter().any(|existing| existing == token) {
        sink.push(token.to_string());
    }
}

/// True when `path`'s final dotted segment is a known repository source-file
/// extension. Reuses the shared [`PATH_LOCATOR_EXTENSIONS`] table (the same test
/// [`target_has_path_locator_shape`] applies to a stripped path token); it does
/// not reimplement locator parsing.
fn extension_is_known(path: &str) -> bool {
    path.rsplit_once('.')
        .map(|(_, extension)| {
            PATH_LOCATOR_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(raw: &str) -> TargetConstraints {
        parse_target(raw, None, None)
    }

    #[test]
    fn exact_family_id_is_a_hard_identity() {
        let parsed = parse("family:python:fastapi:route");
        assert_eq!(parsed.form, TargetForm::FamilyId);
        assert_eq!(
            parsed.hard.family_id.as_deref(),
            Some("family:python:fastapi:route")
        );
        assert!(parsed.hard.unit_id.is_none());
        assert!(parsed.hard.path_locators.is_empty());
        assert!(parsed.conflicts.is_empty());
        assert_eq!(parsed.expected_cardinality(), ResolutionCardinality::One);
    }

    #[test]
    fn exact_unit_id_is_a_hard_identity_and_reports_prefix() {
        let raw = "unit:app/api/routes.py#fastapi_route:10-50";
        let parsed = parse(raw);
        assert_eq!(parsed.form, TargetForm::UnitId);
        assert_eq!(parsed.hard.unit_id.as_deref(), Some(raw));
        assert!(parsed.hard.family_id.is_none());
        // The whole-target `unit:` fact the fuzzy lookup consumes.
        assert!(parsed.is_unit_prefixed());
        assert!(!parse("app/api/routes.py").is_unit_prefixed());
        assert_eq!(parsed.expected_cardinality(), ResolutionCardinality::One);
    }

    #[test]
    fn exact_path_is_a_hard_locator_without_line_or_range() {
        let parsed = parse("app/api/routes.py");
        assert_eq!(parsed.form, TargetForm::PathLocator);
        assert_eq!(parsed.hard.path_locators.len(), 1);
        let locator = &parsed.hard.path_locators[0];
        assert_eq!(locator.path, "app/api/routes.py");
        assert_eq!(locator.line, None);
        assert_eq!(locator.byte_range, None);
        assert!(parsed.path_locator_shaped);
        assert_eq!(parsed.expected_cardinality(), ResolutionCardinality::One);
    }

    #[test]
    fn bare_filename_is_a_hard_path_locator() {
        let parsed = parse("routes.py");
        assert_eq!(parsed.form, TargetForm::PathLocator);
        assert_eq!(parsed.hard.path_locators.len(), 1);
        assert_eq!(parsed.hard.path_locators[0].path, "routes.py");
        assert!(parsed.path_locator_shaped);
    }

    #[test]
    fn path_line_locator_is_parsed() {
        let parsed = parse("src/models.ts:12");
        assert_eq!(parsed.hard.path_locators.len(), 1);
        let locator = &parsed.hard.path_locators[0];
        assert_eq!(locator.path, "src/models.ts");
        assert_eq!(locator.line, Some(12));
        assert_eq!(locator.byte_range, None);
        assert_eq!(parsed.form, TargetForm::PathLocator);
    }

    #[test]
    fn path_byte_range_locator_is_parsed() {
        let parsed = parse("src/models.ts:40-120");
        assert_eq!(parsed.hard.path_locators.len(), 1);
        let locator = &parsed.hard.path_locators[0];
        assert_eq!(locator.path, "src/models.ts");
        assert_eq!(locator.line, None);
        assert_eq!(locator.byte_range, Some((40, 120)));
    }

    #[test]
    fn directory_prefix_is_captured_as_scope_not_a_hard_locator() {
        let parsed = parse("app/api");
        assert_eq!(parsed.scope.directory_prefixes, vec!["app/api".to_string()]);
        assert!(parsed.hard.path_locators.is_empty());
        assert_eq!(parsed.form, TargetForm::DirectoryScope);
        assert_eq!(parsed.expected_cardinality(), ResolutionCardinality::Many);
    }

    #[test]
    fn path_plus_symbol_separates_hard_path_from_ranking_symbol() {
        let parsed = parse("app/api/routes.py list_orders");
        // HARD: the path locator.
        assert_eq!(parsed.hard.path_locators.len(), 1);
        assert_eq!(parsed.hard.path_locators[0].path, "app/api/routes.py");
        // RANKING: the symbol/identifier token, orthogonal to the hard locator.
        assert!(parsed
            .ranking
            .identifier_tokens
            .iter()
            .any(|token| token == "list_orders"));
        assert!(parsed.hard.family_id.is_none());
        assert!(parsed.hard.unit_id.is_none());
    }

    #[test]
    fn path_plus_concept_separates_hard_path_from_ranking_concept() {
        let parsed = parse("app/api/routes.py route");
        assert_eq!(parsed.hard.path_locators.len(), 1);
        // RANKING: the pattern concept, never promoted to a hard constraint.
        assert!(parsed.ranking.concepts.contains(&Concept::Route));
        assert!(parsed.hard.path_locators[0].path == "app/api/routes.py");
    }

    #[test]
    fn framework_role_is_a_ranking_signal() {
        let parsed = parse("How are FastAPI routes implemented?");
        assert_eq!(parsed.form, TargetForm::NaturalLanguage);
        assert!(parsed.ranking.framework_roles.contains("fastapi"));
        assert!(parsed.ranking.concepts.contains(&Concept::Route));
        assert!(parsed.hard.family_id.is_none());
        assert!(parsed.hard.unit_id.is_none());
        assert!(parsed.hard.path_locators.is_empty());
        assert!(!parsed.path_locator_shaped);
    }

    #[test]
    fn language_is_captured_as_scope() {
        let parsed = parse("python sqlalchemy sessions");
        assert!(parsed.scope.languages.contains("python"));
        assert!(parsed.ranking.framework_roles.contains("sqlalchemy"));
        assert!(parsed.ranking.concepts.contains(&Concept::DataAccess));
    }

    #[test]
    fn pure_natural_language_pins_no_hard_identity() {
        let parsed = parse("where do we validate incoming payloads");
        assert_eq!(parsed.form, TargetForm::NaturalLanguage);
        assert!(parsed.hard.family_id.is_none());
        assert!(parsed.hard.unit_id.is_none());
        assert!(parsed.hard.path_locators.is_empty());
        assert!(parsed.scope.directory_prefixes.is_empty());
        assert!(!parsed.path_locator_shaped);
        assert_eq!(parsed.expected_cardinality(), ResolutionCardinality::Many);
    }

    #[test]
    fn within_and_against_scopes_are_captured_verbatim() {
        let parsed = parse_target(
            "route",
            Some("app/api"),
            Some("family:python:fastapi:route"),
        );
        assert_eq!(parsed.within.as_deref(), Some("app/api"));
        assert_eq!(
            parsed.against.as_deref(),
            Some("family:python:fastapi:route")
        );
        // Reserved scopes do not leak into the hard/scope tiers in Phase 1.
        assert!(parsed.hard.family_id.is_none());
        assert!(parsed.scope.directory_prefixes.is_empty());
    }

    #[test]
    fn conflicting_unit_ids_are_represented_not_resolved() {
        let parsed = parse("unit:a.py#f:1-2 unit:b.py#g:3-4");
        // No winner is chosen; the exact id stays empty.
        assert!(parsed.hard.unit_id.is_none());
        assert_eq!(
            parsed.conflicts,
            vec![TargetConflict::MultipleUnitIds(vec![
                "unit:a.py#f:1-2".to_string(),
                "unit:b.py#g:3-4".to_string(),
            ])]
        );
        // The whole-target prefix fact is still reported for the exact authority.
        assert!(parsed.is_unit_prefixed());
        assert_eq!(parsed.expected_cardinality(), ResolutionCardinality::None);
    }

    #[test]
    fn conflicting_family_ids_are_represented_not_resolved() {
        let parsed = parse("family:one family:two");
        assert!(parsed.hard.family_id.is_none());
        assert_eq!(
            parsed.conflicts,
            vec![TargetConflict::MultipleFamilyIds(vec![
                "family:one".to_string(),
                "family:two".to_string(),
            ])]
        );
    }

    #[test]
    fn mixed_family_and_unit_identity_is_a_conflict() {
        let parsed = parse("family:one unit:a.py#f:1-2");
        assert!(parsed
            .conflicts
            .contains(&TargetConflict::MixedFamilyAndUnitId));
        // Each single distinct id is still captured for later phases.
        assert_eq!(parsed.hard.family_id.as_deref(), Some("family:one"));
        assert_eq!(parsed.hard.unit_id.as_deref(), Some("unit:a.py#f:1-2"));
    }

    #[test]
    fn tiers_are_orthogonal_for_a_mixed_target() {
        // A path locator (HARD), a language filter (SCOPE), and a concept plus a
        // symbol residue (RANKING) coexist without cross-contamination.
        let parsed = parse("python app/api/routes.py list_orders route");
        assert_eq!(
            parsed.hard.path_locators.len(),
            1,
            "HARD holds only the path"
        );
        assert!(
            parsed.scope.languages.contains("python"),
            "SCOPE holds language"
        );
        assert!(
            parsed.ranking.concepts.contains(&Concept::Route),
            "RANKING holds concept"
        );
        assert!(
            parsed
                .ranking
                .identifier_tokens
                .iter()
                .any(|token| token == "list_orders"),
            "RANKING holds symbol residue"
        );
        // A path did not become a family/unit id, and a concept did not become a
        // path locator.
        assert!(parsed.hard.family_id.is_none());
        assert!(parsed.hard.unit_id.is_none());
    }

    #[test]
    fn identifier_tokens_preserve_the_shared_tokenizer_output() {
        // The local-context resolver consumes these; they must equal the shared
        // identifier tokenizer applied to the same raw target, in order.
        let raw = "app/api/routes.py list_orders create_order";
        let parsed = parse(raw);
        let expected: Vec<String> = target_identifier_tokens(raw)
            .into_iter()
            .map(str::to_string)
            .collect();
        assert_eq!(parsed.ranking.identifier_tokens, expected);
    }

    #[test]
    fn path_locator_shaped_matches_the_shared_authority() {
        for raw in [
            "app/api/routes.py",
            "routes.py",
            "src/models.ts:12",
            "How are FastAPI routes implemented?",
            "fastapi.Depends",
        ] {
            assert_eq!(
                parse(raw).path_locator_shaped,
                target_has_path_locator_shape(raw),
                "{raw}"
            );
        }
    }

    #[test]
    fn parser_is_deterministic_and_total_on_junk_input() {
        let junk = "!@#$ ".repeat(400) + &"unit:x ".repeat(200);
        let first = parse(&junk);
        let second = parse(&junk);
        assert_eq!(first, second);
        // A single distinct `unit:` handle repeated is not a conflict.
        assert!(first
            .conflicts
            .iter()
            .all(|conflict| !matches!(conflict, TargetConflict::MultipleUnitIds(_))));
    }

    #[test]
    fn directory_prefix_normalization_canonicalizes_safe_scopes() {
        assert_eq!(
            normalize_directory_prefix("app/api"),
            Some("app/api".to_string())
        );
        // Trailing slash, `./` prefix, and collapsed double separators.
        assert_eq!(
            normalize_directory_prefix("./app/api/"),
            Some("app/api".to_string())
        );
        // A single interior segment is preserved verbatim.
        assert_eq!(
            normalize_directory_prefix("src/rust/interfaces/cli"),
            Some("src/rust/interfaces/cli".to_string())
        );
    }

    #[test]
    fn directory_prefix_normalization_rejects_malformed_separators() {
        // A redundant separator leaves an empty segment, which the shared safety
        // authority rejects rather than silently repairing.
        assert_eq!(normalize_directory_prefix("app//api"), None);
    }

    #[test]
    fn directory_prefix_normalization_rejects_unsafe_scopes() {
        // Absolute path, parent traversal, backslash, and scheme are never used to
        // read a scope.
        assert_eq!(normalize_directory_prefix("/etc/passwd"), None);
        assert_eq!(normalize_directory_prefix("app/../secrets"), None);
        assert_eq!(normalize_directory_prefix("app\\api"), None);
        assert_eq!(normalize_directory_prefix("file://app/api"), None);
        assert_eq!(normalize_directory_prefix("   "), None);
    }

    #[test]
    fn resolution_cardinality_variants_are_distinct() {
        // Exercise every variant, including the retrieval-only `Truncated`.
        let variants = [
            ResolutionCardinality::None,
            ResolutionCardinality::One,
            ResolutionCardinality::Many,
            ResolutionCardinality::Truncated,
        ];
        for (index, left) in variants.iter().enumerate() {
            for (other, right) in variants.iter().enumerate() {
                assert_eq!(index == other, left == right);
            }
        }
    }
}
