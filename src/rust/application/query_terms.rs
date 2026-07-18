//! Deterministic, dependency-free query normalization and family retrieval.
//!
//! This module is the term-based retrieval substrate for RepoGrammar's fuzzy
//! discovery. Today the production lookup path resolves a target only when it
//! equals a family id, a `unit:` member id, an exact member role, or an exact
//! `//`-suffix evidence path, so natural-language targets abstain with
//! `InsufficientSupport`. This module makes path/symbol/role/pattern-question
//! queries rankable without any LLM, embedding, or network dependency:
//!
//! 1. [`normalize_query`] folds a raw target into a typed, bounded, total
//!    [`NormalizedQuery`] using small committed vocabulary tables.
//! 2. [`score_family_candidates`] ranks the source-free family search-summary
//!    projection ([`crate::ports::family_store::IndexedFamilySearchSummaryRecord`])
//!    with explainable additive integer weights.
//!
//! Every alias, concept, and role token below is justified by a `framework_role`
//! the index can actually produce (see `adapters/frameworks` and the semantic
//! workers). The tables are intentionally small and committed; the module is
//! total (it never errors on weird input) and bounded (input token count,
//! residue-term count, and retained candidate count are all capped).
//!
//! ROUTED: this substrate is wired into the production lookup path by
//! `application::query::run_term_retrieval` (invoked from
//! `lookup_family_with_freshness_and_local_context`), which applies the calibrated
//! absolute-score and margin abstention gates and bounded freshness hydration. See
//! `docs/specifications/query-resolution.md` for the routed pipeline and gates.

use crate::ports::family_store::IndexedFamilySearchSummaryRecord;
use std::collections::BTreeSet;

/// Maximum whitespace tokens inspected from one raw target. Bounds work on
/// pathological input.
pub const MAX_QUERY_TOKENS: usize = 64;

/// Maximum bytes inspected per whitespace token. Bounds work on a single huge
/// token.
pub const MAX_QUERY_TOKEN_BYTES: usize = 128;

/// Maximum residue terms retained from one query. Bounds the residue set.
pub const MAX_RESIDUE_TERMS: usize = 32;

/// Minimum term length considered for a substring residue hit; shorter residue
/// terms match summary tokens only by exact equality.
pub const MIN_RESIDUE_SUBSTRING_LEN: usize = 3;

/// Maximum distinct residue-term hits scored per candidate. Caps the residue
/// contribution so a term-heavy query cannot unbound a single candidate's score.
pub const MAX_RESIDUE_HITS_SCORED: usize = 4;

/// Retained ranked-candidate cap (K). Retrieval keeps at most this many
/// candidates; the ranking flags whether more scored above zero.
pub const MAX_RANKED_CANDIDATES: usize = 16;

/// Additive weight: the query named a framework and this family's role belongs to
/// it. Highest single signal because a framework filter is the most specific.
pub const WEIGHT_FRAMEWORK_FILTER: i64 = 6;

/// Additive weight: this family's role maps to a concept the query named.
pub const WEIGHT_CONCEPT: i64 = 4;

/// Additive weight: a bounded multi-term phrase names one concept precisely
/// enough to clear the existing absolute selection floor without a framework
/// filter. Competing families still tie and abstain through the margin gate.
pub const WEIGHT_QUALIFIED_CONCEPT: i64 = 10;

/// Additive weight: the query named a language and this family is in it.
pub const WEIGHT_LANGUAGE_FILTER: i64 = 2;

/// Additive weight per distinct residue-term hit against this family's tokens.
pub const WEIGHT_RESIDUE_HIT: i64 = 3;

/// The bounded concept vocabulary. Each concept is produced by at least one
/// `framework_role` the index can index (see [`ROLE_CONCEPTS`]). There is no
/// `migration` concept because no framework role produces migrations in the
/// current vocabulary; `migration` folds to a residue term instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Concept {
    Route,
    Fixture,
    ValidationModel,
    DataAccess,
    Test,
}

impl Concept {
    /// Stable low-cardinality token for this concept.
    pub fn as_token(self) -> &'static str {
        match self {
            Self::Route => "concept:route",
            Self::Fixture => "concept:fixture",
            Self::ValidationModel => "concept:validation_model",
            Self::DataAccess => "concept:data_access",
            Self::Test => "concept:test",
        }
    }
}

/// The typed, bounded, total output of query normalization. Buckets are disjoint:
/// a term contributes to exactly one of them (filter, concept, or residue), and
/// stopwords contribute to none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedQuery {
    /// Language filter tokens (e.g. `python`). A present filter is a hard
    /// exclusion during scoring.
    pub language_filters: BTreeSet<String>,
    /// Framework filter tokens (e.g. `fastapi`). A present filter is a hard
    /// exclusion during scoring.
    pub framework_filters: BTreeSet<String>,
    /// Concept tokens the query named.
    pub concept_tokens: BTreeSet<Concept>,
    /// Concepts named by a committed two-term phrase. The two consumed terms
    /// contribute only here; a separate later term may still name the same
    /// concept in `concept_tokens`.
    pub qualified_concept_tokens: BTreeSet<Concept>,
    /// Leftover normalized fuzzy terms, plus verbatim passthrough locators and
    /// `unit:`/`family:` handles preserved for the exact-precedence layer.
    pub residue_terms: BTreeSet<String>,
}

impl NormalizedQuery {
    /// True when the query carries no retrieval signal at all. An empty query
    /// must never be scored into an all-families dump.
    pub fn is_empty(&self) -> bool {
        self.language_filters.is_empty()
            && self.framework_filters.is_empty()
            && self.concept_tokens.is_empty()
            && self.qualified_concept_tokens.is_empty()
            && self.residue_terms.is_empty()
    }
}

/// Which signals fired for one scored candidate. Typed and low-cardinality so the
/// next wave can render a route-metadata explanation from it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MatchedSignals {
    pub framework_filter: bool,
    pub concept: bool,
    pub language_filter: bool,
    pub residue_hits: usize,
}

/// One ranked family candidate with its explainable score breakdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoredFamilyCandidate {
    pub family_id: String,
    pub score: i64,
    pub signals: MatchedSignals,
    /// 1-based position in the deterministic total order.
    pub rank: usize,
}

/// The deterministic result of scoring the family search projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FamilyCandidateRanking {
    pub candidates: Vec<ScoredFamilyCandidate>,
    /// True when more candidates scored above zero than the retained cap
    /// [`MAX_RANKED_CANDIDATES`].
    pub truncated: bool,
}

// ---------------------------------------------------------------------------
// Committed vocabulary tables. Small, justified, and documented. Every entry
// maps to a token the current index vocabulary can produce; the alias-table
// tests enforce that invariant against [`KNOWN_FRAMEWORK_TOKENS`],
// [`KNOWN_LANGUAGES`], and [`ROLE_CONCEPTS`].
// ---------------------------------------------------------------------------

/// Bounded stopword set: interrogatives and function words that carry no
/// retrieval signal.
pub const STOPWORDS: &[&str] = &[
    "how",
    "are",
    "is",
    "the",
    "in",
    "this",
    // `repository` is intentionally NOT a stopword: it is a `CONCEPT_ALIASES`
    // data-access term. A stopword entry here would shadow that alias (stopwords
    // are checked first in `classify_term`), stranding repository-worded queries.
    "implemented",
    "defined",
    "structured",
    "handled",
    "done",
    "we",
    "do",
    "where",
    "what",
    "show",
    "me",
    "a",
    "an",
    "of",
    "to",
    "and",
    "or",
    "for",
    "with",
    "does",
    "using",
];

/// Bounded singular/plural folding table (`plural` -> `singular`). Applied before
/// alias lookup so `routes` and `route` classify identically.
pub const PLURALS: &[(&str, &str)] = &[
    ("routes", "route"),
    ("fixtures", "fixture"),
    ("models", "model"),
    ("tests", "test"),
    ("cases", "case"),
    ("handlers", "handler"),
    ("methods", "method"),
    ("transactions", "transaction"),
    ("schemas", "schema"),
    ("sessions", "session"),
    ("endpoints", "endpoint"),
    ("validators", "validator"),
    ("repositories", "repository"),
    ("migrations", "migration"),
    ("controllers", "controller"),
    ("queries", "query"),
];

/// Language aliases (`term` -> canonical language). Canonical languages match the
/// stored `code_units.language` vocabulary ([`KNOWN_LANGUAGES`]). Bare `c` is
/// intentionally omitted as too ambiguous; the C language is reachable through
/// the `c++`/`c#` compound aliases and `cpp`/`csharp`.
pub const LANGUAGE_ALIASES: &[(&str, &str)] = &[
    ("python", "python"),
    ("py", "python"),
    ("typescript", "typescript"),
    ("ts", "typescript"),
    ("javascript", "javascript"),
    ("js", "javascript"),
    ("rust", "rust"),
    ("rs", "rust"),
    ("java", "java"),
    ("csharp", "csharp"),
    ("cpp", "cpp"),
    ("cxx", "cpp"),
];

/// Compound language aliases whose punctuation would be destroyed by generic
/// splitting; detected before punctuation splitting.
pub const COMPOUND_LANGUAGE_ALIASES: &[(&str, &str)] = &[("c#", "csharp"), ("c++", "cpp")];

/// Framework aliases (`term` -> one or more framework filter tokens). Each token
/// is the leading segment of a real `framework_role` the index produces (see
/// [`KNOWN_FRAMEWORK_TOKENS`]). Aliases expand to every producible token that
/// spelling covers (e.g. `jest` and `vitest` share `jest_vitest`; `junit` covers
/// both major versions; `spring` covers the core, boot, and data role families).
pub const FRAMEWORK_ALIASES: &[(&str, &[&str])] = &[
    ("fastapi", &["fastapi"]),
    ("flask", &["flask"]),
    ("django", &["django"]),
    ("express", &["express"]),
    ("fastify", &["fastify"]),
    ("hono", &["hono"]),
    ("nestjs", &["nestjs"]),
    ("next", &["next"]),
    ("prisma", &["prisma"]),
    ("drizzle", &["drizzle"]),
    ("zod", &["zod"]),
    ("jest", &["jest_vitest"]),
    ("vitest", &["jest_vitest"]),
    ("pytest", &["pytest"]),
    ("sqlalchemy", &["sqlalchemy"]),
    ("pydantic", &["pydantic"]),
    ("axum", &["axum"]),
    ("spring", &["spring", "spring_boot", "spring_data"]),
    ("aspnet", &["aspnetcore"]),
    ("xunit", &["xunit"]),
    ("gtest", &["gtest"]),
    ("catch2", &["catch2"]),
    ("junit", &["junit4", "junit5"]),
    ("testng", &["testng"]),
    ("serde", &["serde"]),
    ("thiserror", &["thiserror"]),
    ("tokio", &["tokio"]),
];

/// Concept aliases (`term` -> [`Concept`]). Follows the committed vocabulary:
/// route/endpoint/api/rest/url/handler are route terms; schema/validator/
/// validation are validation-model terms; transaction/session/db/database/orm/
/// repository/query are data-access terms; test/spec/unittest are test terms.
///
/// A term may appear more than once to name multiple concepts:
/// [`classify_term`] inserts **every** matching concept. Bare `model` is
/// genuinely ambiguous — it denotes both validation models (Pydantic/Zod) and
/// data-access ORM models (SQLAlchemy/Django) — so it maps to both concepts,
/// keeping a bare `models` query ambiguous (it surfaces both kinds) while
/// `validation`/`schema` remain validation-model-only disambiguators.
pub const CONCEPT_ALIASES: &[(&str, Concept)] = &[
    ("route", Concept::Route),
    ("endpoint", Concept::Route),
    ("api", Concept::Route),
    ("rest", Concept::Route),
    ("url", Concept::Route),
    ("handler", Concept::Route),
    ("fixture", Concept::Fixture),
    ("model", Concept::ValidationModel),
    ("model", Concept::DataAccess),
    ("schema", Concept::ValidationModel),
    ("validator", Concept::ValidationModel),
    ("validation", Concept::ValidationModel),
    ("transaction", Concept::DataAccess),
    ("session", Concept::DataAccess),
    ("db", Concept::DataAccess),
    ("database", Concept::DataAccess),
    ("orm", Concept::DataAccess),
    ("repository", Concept::DataAccess),
    ("query", Concept::DataAccess),
    ("test", Concept::Test),
    ("spec", Concept::Test),
    ("unittest", Concept::Test),
];

/// Bounded two-term concept phrases (`first`, `second`, concept). Unlike a bare
/// concept alias, these phrases provide two aligned lexical signals for one
/// concept. Both terms are consumed as one qualified concept, so `test fixture`
/// does not also introduce the broader test concept.
pub const QUALIFIED_CONCEPT_ALIASES: &[(&str, &str, Concept)] = &[
    ("test", "fixture", Concept::Fixture),
    ("unit", "test", Concept::Test),
    ("test", "case", Concept::Test),
];

/// The `framework_role` -> [`Concept`] table. Built from the concrete role
/// vocabulary in `adapters/frameworks` and the semantic workers. Roles absent
/// here (CLI commands, tasks, generic components, framework entrypoints) carry no
/// concept and can still match via framework filter or residue.
pub const ROLE_CONCEPTS: &[(&str, Concept)] = &[
    // Route roles: HTTP endpoints, route handlers, controllers, URL patterns.
    ("framework:fastapi.route", Concept::Route),
    ("framework:flask.route", Concept::Route),
    ("framework:django.url_pattern", Concept::Route),
    ("framework:axum.route", Concept::Route),
    ("framework:express.route_handler", Concept::Route),
    ("framework:fastify.route_handler", Concept::Route),
    ("framework:hono.route", Concept::Route),
    ("framework:nestjs.route", Concept::Route),
    ("framework:nestjs.controller", Concept::Route),
    ("framework:next.pages.api_route", Concept::Route),
    ("framework:next.route.handler", Concept::Route),
    ("framework:aspnetcore.controller", Concept::Route),
    ("framework:aspnetcore.controller_action", Concept::Route),
    ("framework:aspnetcore.minimal_route", Concept::Route),
    ("framework:spring.mvc_route", Concept::Route),
    ("framework:jaxrs.resource", Concept::Route),
    ("framework:jaxrs.resource_method", Concept::Route),
    // Fixture roles.
    ("framework:pytest.fixture", Concept::Fixture),
    ("framework:gtest.fixture", Concept::Fixture),
    // Validation-model roles: request/response and schema shapes.
    ("framework:pydantic.model", Concept::ValidationModel),
    ("framework:zod.schema", Concept::ValidationModel),
    ("framework:serde.model", Concept::ValidationModel),
    ("framework:drizzle.schema.table", Concept::ValidationModel),
    // Data-access roles: ORM models, repositories, queries, transactions.
    ("framework:sqlalchemy.model", Concept::DataAccess),
    (
        "framework:sqlalchemy.repository_method",
        Concept::DataAccess,
    ),
    ("framework:django.model", Concept::DataAccess),
    ("framework:drizzle.query", Concept::DataAccess),
    ("framework:drizzle.transaction", Concept::DataAccess),
    ("framework:prisma.query", Concept::DataAccess),
    ("framework:prisma.transaction", Concept::DataAccess),
    ("framework:jpa.entity", Concept::DataAccess),
    ("framework:jpa.embeddable", Concept::DataAccess),
    ("framework:jpa.mapped_superclass", Concept::DataAccess),
    ("framework:spring_data.repository", Concept::DataAccess),
    ("framework:efcore.db_context", Concept::DataAccess),
    ("framework:efcore.entity_set", Concept::DataAccess),
    // Test roles.
    ("framework:pytest.test", Concept::Test),
    ("framework:django.test", Concept::Test),
    ("framework:unittest.test", Concept::Test),
    ("framework:jest_vitest.test", Concept::Test),
    ("framework:jest_vitest.suite", Concept::Test),
    ("framework:gtest.test", Concept::Test),
    ("framework:catch2.test", Concept::Test),
    ("framework:boost_test.test", Concept::Test),
    ("framework:boost_test.suite", Concept::Test),
    ("framework:doctest.test", Concept::Test),
    ("framework:mstest.test", Concept::Test),
    ("framework:nunit.test", Concept::Test),
    ("framework:xunit.test", Concept::Test),
    ("framework:testng.test", Concept::Test),
    ("framework:junit4.test", Concept::Test),
    ("framework:junit5.test", Concept::Test),
    ("framework:tokio.test", Concept::Test),
    ("framework:repogrammar.rust_product_test", Concept::Test),
];

/// Framework filter tokens the index can produce, derived from the leading
/// segment of the concrete `framework_role` vocabulary. Used to prove every
/// framework alias resolves to a producible token.
pub const KNOWN_FRAMEWORK_TOKENS: &[&str] = &[
    "aspnetcore",
    "axum",
    "boost_test",
    "catch2",
    "celery",
    "clap",
    "click",
    "django",
    "doctest",
    "drizzle",
    "efcore",
    "express",
    "fastapi",
    "fastify",
    "flask",
    "gtest",
    "hono",
    "jaxrs",
    "jest_vitest",
    "jpa",
    "junit4",
    "junit5",
    "mstest",
    "nestjs",
    "next",
    "nunit",
    "prisma",
    "pydantic",
    "pytest",
    "react",
    "repogrammar",
    "serde",
    "spring",
    "spring_boot",
    "spring_data",
    "sqlalchemy",
    "testng",
    "thiserror",
    "tokio",
    "typer",
    "unittest",
    "xunit",
    "zod",
];

/// Canonical languages, matching the stored `code_units.language` vocabulary.
pub const KNOWN_LANGUAGES: &[&str] = &[
    "python",
    "typescript",
    "javascript",
    "rust",
    "java",
    "csharp",
    "c",
    "cpp",
    "go",
    "php",
    "ruby",
    "swift",
];

/// Extract a family's framework filter token from its role, e.g.
/// `framework:fastapi.route` -> `fastapi`. Returns `None` for a role that is not
/// a `framework:`-prefixed token.
pub fn framework_token_for_role(role: &str) -> Option<&str> {
    let rest = role.strip_prefix("framework:")?;
    match rest.split('.').next() {
        Some(token) if !token.is_empty() => Some(token),
        _ => None,
    }
}

/// Map a `framework_role` to its concept, if any.
pub fn role_concept(role: &str) -> Option<Concept> {
    ROLE_CONCEPTS
        .iter()
        .find(|(candidate, _)| *candidate == role)
        .map(|(_, concept)| *concept)
}

/// Normalize a raw query target into a typed, bounded [`NormalizedQuery`].
/// Deterministic and total: any input, however malformed, yields a value and
/// never panics or errors.
pub fn normalize_query(raw: &str) -> NormalizedQuery {
    let mut normalized = NormalizedQuery::default();
    let mut pending_term: Option<String> = None;
    for whitespace_token in raw.split_whitespace().take(MAX_QUERY_TOKENS) {
        let whitespace_token = safe_prefix(whitespace_token, MAX_QUERY_TOKEN_BYTES);
        // Locators and `unit:`/`family:` handles pass through verbatim (case
        // preserved) for the exact-precedence layer; only the fuzzy residue is
        // normalized.
        if is_passthrough_token(whitespace_token) {
            flush_pending_term(&mut pending_term, &mut normalized);
            insert_bounded_residue(&mut normalized, whitespace_token.to_string());
            continue;
        }
        let lowered = whitespace_token.to_ascii_lowercase();
        if let Some(language) = compound_language(&lowered) {
            flush_pending_term(&mut pending_term, &mut normalized);
            normalized.language_filters.insert(language.to_string());
            continue;
        }
        for subtoken in lowered.split(|character: char| !character.is_ascii_alphanumeric()) {
            if !subtoken.is_empty() {
                classify_or_qualify_term(subtoken, &mut pending_term, &mut normalized);
            }
        }
    }
    flush_pending_term(&mut pending_term, &mut normalized);
    normalized
}

/// Rank the source-free family search projection against a normalized query with
/// explainable additive weights and a deterministic total order. An empty query
/// yields an empty ranking (never an all-families dump).
pub fn score_family_candidates(
    normalized: &NormalizedQuery,
    summaries: &[IndexedFamilySearchSummaryRecord],
) -> FamilyCandidateRanking {
    if normalized.is_empty() {
        return FamilyCandidateRanking::default();
    }

    // (candidate, prevalence class rank, coverage) tuples held for ordering.
    let mut scored: Vec<(ScoredFamilyCandidate, u8, f64)> = Vec::new();
    for summary in summaries {
        // Language and framework filters are hard exclusions when present.
        if !normalized.language_filters.is_empty()
            && !normalized.language_filters.contains(&summary.language)
        {
            continue;
        }
        let framework_token = framework_token_for_role(&summary.framework_role);
        if !normalized.framework_filters.is_empty()
            && !framework_token
                .map(|token| normalized.framework_filters.contains(token))
                .unwrap_or(false)
        {
            continue;
        }

        let mut signals = MatchedSignals::default();
        let mut score = 0;
        if !normalized.framework_filters.is_empty() {
            signals.framework_filter = true;
            score += WEIGHT_FRAMEWORK_FILTER;
        }
        if let Some(concept) = role_concept(&summary.framework_role) {
            if normalized.qualified_concept_tokens.contains(&concept) {
                signals.concept = true;
                score += WEIGHT_QUALIFIED_CONCEPT;
            } else if normalized.concept_tokens.contains(&concept) {
                signals.concept = true;
                score += WEIGHT_CONCEPT;
            }
        }
        if !normalized.language_filters.is_empty() {
            signals.language_filter = true;
            score += WEIGHT_LANGUAGE_FILTER;
        }
        let residue_hits = residue_hit_count(normalized, summary, framework_token);
        if residue_hits > 0 {
            signals.residue_hits = residue_hits;
            score += WEIGHT_RESIDUE_HIT * residue_hits as i64;
        }

        if score <= 0 {
            // No positive signal: never surface unrelated families.
            continue;
        }
        scored.push((
            ScoredFamilyCandidate {
                family_id: summary.family_id.clone(),
                score,
                signals,
                rank: 0,
            },
            prevalence_class_rank(&summary.classification),
            summary.prevalence.coverage_ratio.unwrap_or(0.0),
        ));
    }

    // Total order: score desc, prevalence class asc (dominant first), coverage
    // desc, family id byte order asc.
    scored.sort_by(|left, right| {
        right
            .0
            .score
            .cmp(&left.0.score)
            .then(left.1.cmp(&right.1))
            .then(right.2.total_cmp(&left.2))
            .then(
                left.0
                    .family_id
                    .as_bytes()
                    .cmp(right.0.family_id.as_bytes()),
            )
    });

    let truncated = scored.len() > MAX_RANKED_CANDIDATES;
    let candidates = scored
        .into_iter()
        .take(MAX_RANKED_CANDIDATES)
        .enumerate()
        .map(|(index, (mut candidate, _, _))| {
            candidate.rank = index + 1;
            candidate
        })
        .collect();
    FamilyCandidateRanking {
        candidates,
        truncated,
    }
}

fn classify_or_qualify_term(
    term: &str,
    pending_term: &mut Option<String>,
    normalized: &mut NormalizedQuery,
) {
    let term = singularize(term).to_string();
    if let Some(previous) = pending_term.take() {
        if let Some(concept) = qualified_concept(&previous, &term) {
            normalized.qualified_concept_tokens.insert(concept);
            return;
        }
        classify_term(&previous, normalized);
    }
    *pending_term = Some(term);
}

fn flush_pending_term(pending_term: &mut Option<String>, normalized: &mut NormalizedQuery) {
    if let Some(term) = pending_term.take() {
        classify_term(&term, normalized);
    }
}

fn qualified_concept(first: &str, second: &str) -> Option<Concept> {
    QUALIFIED_CONCEPT_ALIASES
        .iter()
        .find(|(candidate_first, candidate_second, _)| {
            *candidate_first == first && *candidate_second == second
        })
        .map(|(_, _, concept)| *concept)
}

fn classify_term(term: &str, normalized: &mut NormalizedQuery) {
    let term = singularize(term);
    if STOPWORDS.contains(&term) {
        return;
    }
    if let Some(language) = lookup_alias(LANGUAGE_ALIASES, term) {
        normalized.language_filters.insert(language.to_string());
        return;
    }
    if let Some((_, tokens)) = FRAMEWORK_ALIASES.iter().find(|(alias, _)| *alias == term) {
        for token in *tokens {
            normalized.framework_filters.insert((*token).to_string());
        }
        return;
    }
    // A term may name more than one concept (e.g. bare `model` denotes both a
    // validation model and a data-access ORM model); insert every match.
    let mut matched_concept = false;
    for (_, concept) in CONCEPT_ALIASES.iter().filter(|(alias, _)| *alias == term) {
        normalized.concept_tokens.insert(*concept);
        matched_concept = true;
    }
    if matched_concept {
        return;
    }
    if term.len() >= 2 {
        insert_bounded_residue(normalized, term.to_string());
    }
}

fn insert_bounded_residue(normalized: &mut NormalizedQuery, term: String) {
    if normalized.residue_terms.len() < MAX_RESIDUE_TERMS {
        normalized.residue_terms.insert(term);
    }
}

fn singularize(term: &str) -> &str {
    PLURALS
        .iter()
        .find(|(plural, _)| *plural == term)
        .map(|(_, singular)| *singular)
        .unwrap_or(term)
}

fn lookup_alias(table: &[(&str, &'static str)], term: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(alias, _)| *alias == term)
        .map(|(_, value)| *value)
}

fn is_passthrough_token(token: &str) -> bool {
    token.starts_with("unit:")
        || token.starts_with("family:")
        || token.contains('/')
        || has_line_locator(token)
}

fn has_line_locator(token: &str) -> bool {
    match token.rsplit_once(':') {
        Some((prefix, suffix)) if !prefix.is_empty() && !suffix.is_empty() => {
            suffix
                .chars()
                .all(|character| character.is_ascii_digit() || character == '-')
                && suffix.chars().any(|character| character.is_ascii_digit())
        }
        _ => false,
    }
}

fn compound_language(lowered: &str) -> Option<&'static str> {
    let core = lowered.trim_matches(|character: char| {
        !character.is_ascii_alphanumeric() && character != '#' && character != '+'
    });
    COMPOUND_LANGUAGE_ALIASES
        .iter()
        .find(|(alias, _)| *alias == core)
        .map(|(_, language)| *language)
}

fn safe_prefix(token: &str, max_bytes: usize) -> &str {
    if token.len() <= max_bytes {
        return token;
    }
    let mut end = max_bytes;
    while end > 0 && !token.is_char_boundary(end) {
        end -= 1;
    }
    &token[..end]
}

fn prevalence_class_rank(classification: &str) -> u8 {
    match classification {
        "DOMINANT_PATTERN" => 0,
        "SUPPORTED_PATTERN" => 1,
        "MINORITY_PATTERN" => 2,
        // UNKNOWN_PREVALENCE and any unexpected value rank last.
        _ => 3,
    }
}

fn residue_hit_count(
    normalized: &NormalizedQuery,
    summary: &IndexedFamilySearchSummaryRecord,
    framework_token: Option<&str>,
) -> usize {
    if normalized.residue_terms.is_empty() {
        return 0;
    }
    let mut tokens = BTreeSet::new();
    insert_subtokens(&mut tokens, &summary.framework_role);
    insert_subtokens(&mut tokens, &summary.language);
    insert_subtokens(&mut tokens, &summary.code_unit_kind);
    if let Some(token) = framework_token {
        tokens.insert(token.to_ascii_lowercase());
    }
    for component in &summary.evidence_path_components {
        insert_subtokens(&mut tokens, component);
    }

    let mut hits = 0;
    for term in &normalized.residue_terms {
        if term_hits(&tokens, term) {
            hits += 1;
            if hits >= MAX_RESIDUE_HITS_SCORED {
                break;
            }
        }
    }
    hits
}

fn insert_subtokens(tokens: &mut BTreeSet<String>, value: &str) {
    let lowered = value.to_ascii_lowercase();
    tokens.insert(lowered.clone());
    for subtoken in lowered.split(|character: char| !character.is_ascii_alphanumeric()) {
        if !subtoken.is_empty() {
            tokens.insert(subtoken.to_string());
        }
    }
}

fn term_hits(tokens: &BTreeSet<String>, term: &str) -> bool {
    let term = term.to_ascii_lowercase();
    tokens.iter().any(|token| {
        *token == term || (term.len() >= MIN_RESIDUE_SUBSTRING_LEN && token.contains(term.as_str()))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::FamilyPrevalence;

    fn summary(
        family_id: &str,
        language: &str,
        framework_role: &str,
        classification: &str,
        components: &[&str],
    ) -> IndexedFamilySearchSummaryRecord {
        IndexedFamilySearchSummaryRecord {
            family_id: family_id.to_string(),
            language: language.to_string(),
            code_unit_kind: "unit".to_string(),
            framework_role: framework_role.to_string(),
            classification: classification.to_string(),
            support: 2,
            prevalence: prevalence(classification, Some(0.5)),
            evidence_path_components: components.iter().map(|value| value.to_string()).collect(),
        }
    }

    fn prevalence(classification: &str, coverage: Option<f64>) -> FamilyPrevalence {
        FamilyPrevalence {
            eligible_peer_count: 2,
            supported_member_count: 2,
            coverage_ratio: coverage,
            competing_ready_family_count: 0,
            largest_competing_support: 0,
            blocked_peer_count: 0,
            unsupported_peer_count: 0,
            classification_reason: format!("{classification} sample"),
        }
    }

    #[test]
    fn normalization_folds_case_punctuation_plurals_aliases_and_stopwords() {
        let normalized = normalize_query("How are FastAPI ROUTES implemented?");
        assert_eq!(
            normalized.framework_filters,
            BTreeSet::from(["fastapi".to_string()])
        );
        assert_eq!(normalized.concept_tokens, BTreeSet::from([Concept::Route]));
        assert!(normalized.language_filters.is_empty());
        // "how", "are", "implemented" are stopwords; nothing lands in residue.
        assert!(normalized.residue_terms.is_empty());
    }

    #[test]
    fn normalization_maps_language_and_framework_aliases() {
        let normalized = normalize_query("python sqlalchemy sessions");
        assert_eq!(
            normalized.language_filters,
            BTreeSet::from(["python".to_string()])
        );
        assert_eq!(
            normalized.framework_filters,
            BTreeSet::from(["sqlalchemy".to_string()])
        );
        // "sessions" -> "session" -> data-access concept.
        assert_eq!(
            normalized.concept_tokens,
            BTreeSet::from([Concept::DataAccess])
        );
    }

    #[test]
    fn normalization_maps_ambiguous_model_and_repository_concepts() {
        // Bare `models` is genuinely ambiguous: it denotes both validation models
        // and data-access ORM models, so it surfaces both concepts.
        let models = normalize_query("How are models defined?");
        assert_eq!(
            models.concept_tokens,
            BTreeSet::from([Concept::ValidationModel, Concept::DataAccess])
        );
        assert!(models.framework_filters.is_empty());
        assert!(models.residue_terms.is_empty());
        // `validation` and `schema` remain validation-model-only disambiguators.
        let schema = normalize_query("schema validation");
        assert_eq!(
            schema.concept_tokens,
            BTreeSet::from([Concept::ValidationModel])
        );
        // `repository`/`repositories` is a data-access concept, not a stopword.
        let repos = normalize_query("How do Prisma repositories work?");
        assert!(repos.concept_tokens.contains(&Concept::DataAccess));
        assert_eq!(
            repos.framework_filters,
            BTreeSet::from(["prisma".to_string()])
        );
    }

    #[test]
    fn normalization_qualifies_bounded_concept_phrases_without_broadening_decoys() {
        for (target, concept) in [
            ("test fixtures", Concept::Fixture),
            ("unit tests", Concept::Test),
            ("test cases", Concept::Test),
        ] {
            let normalized = normalize_query(target);
            assert_eq!(
                normalized.qualified_concept_tokens,
                BTreeSet::from([concept]),
                "{target}"
            );
            assert!(normalized.concept_tokens.is_empty(), "{target}");
            assert!(normalized.residue_terms.is_empty(), "{target}");
        }

        for target in ["pytset fixture", "tests written", "endpoint"] {
            assert!(
                normalize_query(target).qualified_concept_tokens.is_empty(),
                "{target} must not inherit a qualified phrase"
            );
        }
    }

    #[test]
    fn normalization_handles_compound_language_tokens() {
        let normalized = normalize_query("where is c# and c++ code");
        assert_eq!(
            normalized.language_filters,
            BTreeSet::from(["csharp".to_string(), "cpp".to_string()])
        );
    }

    #[test]
    fn normalization_passes_locators_and_handles_through_verbatim() {
        let normalized =
            normalize_query("unit:src/App.ts#method:foo family:python:x src/app.py:12 routes");
        assert!(normalized
            .residue_terms
            .contains("unit:src/App.ts#method:foo"));
        assert!(normalized.residue_terms.contains("family:python:x"));
        assert!(normalized.residue_terms.contains("src/app.py:12"));
        // Case is preserved for the exact-precedence layer.
        assert!(normalized
            .residue_terms
            .iter()
            .any(|term| term.contains("App.ts")));
        // The fuzzy residue term still classifies.
        assert_eq!(normalized.concept_tokens, BTreeSet::from([Concept::Route]));
    }

    #[test]
    fn normalization_is_total_and_bounded_on_junk_input() {
        let junk = "!@#$ ".repeat(500) + &"blah".repeat(1000);
        let first = normalize_query(&junk);
        let second = normalize_query(&junk);
        assert_eq!(first, second, "normalization must be deterministic");
        assert!(first.residue_terms.len() <= MAX_RESIDUE_TERMS);
        // Pure punctuation yields no signal.
        assert!(normalize_query("!!! ??? ...").is_empty());
    }

    #[test]
    fn every_framework_alias_maps_to_a_producible_token() {
        for (alias, tokens) in FRAMEWORK_ALIASES {
            for token in *tokens {
                assert!(
                    KNOWN_FRAMEWORK_TOKENS.contains(token),
                    "framework alias {alias} maps to non-producible token {token}"
                );
            }
        }
    }

    #[test]
    fn every_concept_alias_maps_to_a_producible_concept() {
        let producible: BTreeSet<Concept> =
            ROLE_CONCEPTS.iter().map(|(_, concept)| *concept).collect();
        for (alias, concept) in CONCEPT_ALIASES {
            assert!(
                producible.contains(concept),
                "concept alias {alias} maps to concept no role produces"
            );
        }
        for (first, second, concept) in QUALIFIED_CONCEPT_ALIASES {
            assert!(
                producible.contains(concept),
                "qualified concept alias {first} {second} maps to concept no role produces"
            );
        }
    }

    #[test]
    fn every_language_alias_maps_to_a_known_language() {
        for (alias, language) in LANGUAGE_ALIASES.iter().chain(COMPOUND_LANGUAGE_ALIASES) {
            assert!(
                KNOWN_LANGUAGES.contains(language),
                "language alias {alias} maps to unknown language {language}"
            );
        }
    }

    #[test]
    fn every_role_concept_token_is_producible() {
        for (role, _) in ROLE_CONCEPTS {
            let token = framework_token_for_role(role)
                .unwrap_or_else(|| panic!("role {role} has no framework token"));
            assert!(
                KNOWN_FRAMEWORK_TOKENS.contains(&token),
                "role {role} derives non-producible token {token}"
            );
        }
    }

    #[test]
    fn scoring_hard_excludes_language_and_framework_mismatches() {
        let normalized = normalize_query("python fastapi routes");
        let summaries = vec![
            summary(
                "family:py:fastapi",
                "python",
                "framework:fastapi.route",
                "DOMINANT_PATTERN",
                &["routes"],
            ),
            // Wrong language: excluded despite the route concept.
            summary(
                "family:ts:hono",
                "typescript",
                "framework:hono.route",
                "DOMINANT_PATTERN",
                &["routes"],
            ),
            // Right language, wrong framework: excluded.
            summary(
                "family:py:flask",
                "python",
                "framework:flask.route",
                "DOMINANT_PATTERN",
                &["routes"],
            ),
        ];
        let ranking = score_family_candidates(&normalized, &summaries);
        assert_eq!(ranking.candidates.len(), 1);
        assert_eq!(ranking.candidates[0].family_id, "family:py:fastapi");
    }

    #[test]
    fn concept_match_ranks_route_family_above_unrelated_residue_only() {
        let normalized = normalize_query("route users");
        let summaries = vec![
            summary(
                "family:route",
                "python",
                "framework:fastapi.route",
                "DOMINANT_PATTERN",
                &["users"],
            ),
            summary(
                "family:model",
                "python",
                "framework:sqlalchemy.model",
                "DOMINANT_PATTERN",
                &["users"],
            ),
        ];
        let ranking = score_family_candidates(&normalized, &summaries);
        assert_eq!(ranking.candidates.len(), 2);
        assert_eq!(ranking.candidates[0].family_id, "family:route");
        assert!(ranking.candidates[0].signals.concept);
        assert!(ranking.candidates[0].score > ranking.candidates[1].score);
        assert!(!ranking.candidates[1].signals.concept);
    }

    #[test]
    fn qualified_fixture_phrase_clears_floor_without_scoring_test_family() {
        let normalized = normalize_query("How are test fixtures defined?");
        let summaries = vec![
            summary(
                "family:fixture",
                "python",
                "framework:pytest.fixture",
                "DOMINANT_PATTERN",
                &[],
            ),
            summary(
                "family:test",
                "python",
                "framework:pytest.test",
                "DOMINANT_PATTERN",
                &[],
            ),
        ];
        let ranking = score_family_candidates(&normalized, &summaries);
        assert_eq!(ranking.candidates.len(), 1);
        assert_eq!(ranking.candidates[0].family_id, "family:fixture");
        assert_eq!(ranking.candidates[0].score, WEIGHT_QUALIFIED_CONCEPT);
        assert!(ranking.candidates[0].signals.concept);
    }

    #[test]
    fn residue_terms_hit_path_components() {
        let normalized = normalize_query("checkout");
        let summaries = vec![summary(
            "family:orders",
            "python",
            "framework:fastapi.route",
            "DOMINANT_PATTERN",
            &["checkout", "orders"],
        )];
        let ranking = score_family_candidates(&normalized, &summaries);
        assert_eq!(ranking.candidates.len(), 1);
        assert_eq!(ranking.candidates[0].signals.residue_hits, 1);
        assert_eq!(ranking.candidates[0].score, WEIGHT_RESIDUE_HIT);
    }

    #[test]
    fn deterministic_tiebreak_orders_by_class_then_family_id() {
        let normalized = normalize_query("route");
        let summaries = vec![
            summary(
                "family:zzz",
                "python",
                "framework:fastapi.route",
                "DOMINANT_PATTERN",
                &[],
            ),
            summary(
                "family:aaa",
                "python",
                "framework:flask.route",
                "DOMINANT_PATTERN",
                &[],
            ),
            summary(
                "family:mmm",
                "python",
                "framework:django.url_pattern",
                "MINORITY_PATTERN",
                &[],
            ),
        ];
        let ranking = score_family_candidates(&normalized, &summaries);
        // Equal score and class break by family id byte order; the minority
        // family sorts last by class rank.
        let ids: Vec<&str> = ranking
            .candidates
            .iter()
            .map(|candidate| candidate.family_id.as_str())
            .collect();
        assert_eq!(ids, vec!["family:aaa", "family:zzz", "family:mmm"]);
    }

    #[test]
    fn k_cap_truncates_and_flags() {
        let mut summaries = Vec::new();
        for index in 0..(MAX_RANKED_CANDIDATES + 4) {
            summaries.push(summary(
                &format!("family:{index:03}"),
                "python",
                "framework:fastapi.route",
                "DOMINANT_PATTERN",
                &[],
            ));
        }
        let ranking = score_family_candidates(&normalize_query("route"), &summaries);
        assert_eq!(ranking.candidates.len(), MAX_RANKED_CANDIDATES);
        assert!(ranking.truncated);
        assert_eq!(ranking.candidates[0].rank, 1);
        assert_eq!(
            ranking.candidates[MAX_RANKED_CANDIDATES - 1].rank,
            MAX_RANKED_CANDIDATES
        );
    }

    #[test]
    fn empty_normalization_yields_empty_candidates() {
        let summaries = vec![summary(
            "family:route",
            "python",
            "framework:fastapi.route",
            "DOMINANT_PATTERN",
            &["routes"],
        )];
        // A stopword-only query normalizes to empty and must not dump families.
        // (`repository` is a data-access concept alias, not a stopword, so it is
        // deliberately excluded from this all-stopword phrase.)
        let normalized = normalize_query("how is this structured");
        assert!(normalized.is_empty());
        let ranking = score_family_candidates(&normalized, &summaries);
        assert!(ranking.candidates.is_empty());
        assert!(!ranking.truncated);
    }

    #[test]
    fn golden_fastapi_route_question_ranks_route_family_first() {
        let summaries = vec![
            summary(
                "family:python:fastapi:route",
                "python",
                "framework:fastapi.route",
                "DOMINANT_PATTERN",
                &["app", "api", "routes", "users.py"],
            ),
            summary(
                "family:python:pytest:test",
                "python",
                "framework:pytest.test",
                "DOMINANT_PATTERN",
                &["tests", "test_users.py"],
            ),
            summary(
                "family:python:sqlalchemy:model",
                "python",
                "framework:sqlalchemy.model",
                "SUPPORTED_PATTERN",
                &["app", "models", "user.py"],
            ),
        ];
        let normalized = normalize_query("How are FastAPI routes implemented?");
        let ranking = score_family_candidates(&normalized, &summaries);

        assert_eq!(
            ranking.candidates.len(),
            1,
            "framework filter excludes non-fastapi families"
        );
        let top = &ranking.candidates[0];
        assert_eq!(top.family_id, "family:python:fastapi:route");
        assert_eq!(top.rank, 1);
        assert_eq!(
            top.score,
            WEIGHT_FRAMEWORK_FILTER + WEIGHT_CONCEPT,
            "framework filter plus route concept"
        );
        assert_eq!(
            top.signals,
            MatchedSignals {
                framework_filter: true,
                concept: true,
                language_filter: false,
                residue_hits: 0,
            }
        );
        assert!(!ranking.truncated);
    }
}
