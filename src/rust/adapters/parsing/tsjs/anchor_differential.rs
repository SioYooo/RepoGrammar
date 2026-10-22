//! ADR-0059 slice-2 differential: the text reading of receivers, call parts
//! and decorators against the tree reading, over committed fixtures, a branch
//! corpus and an external corpus. Test-only scaffolding kept with the legacy
//! text reading (D6) until the maintainer accepts the classification.

use super::scope_graph::ScopeGraphLite;
use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
use crate::core::model::{
    CodeUnit, ContentHash, Language, RepositoryRevision, SemanticFact, SemanticFactKind,
};
use crate::ports::parser::{ParserProjectContext, SourceDocument, SourceParser};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// One difference: the class, the source, and a short detail.
pub(super) type Difference = (&'static str, String, String);

#[derive(Debug, Default)]
pub(super) struct Report {
    pub(super) differences: Vec<Difference>,
    pub(super) sources: usize,
    pub(super) considered_units: usize,
    pub(super) legacy_time: Duration,
    pub(super) tree_time: Duration,
    /// Whole anchor pass (graph analysis included) per reading.
    pub(super) legacy_pass: Duration,
    pub(super) tree_pass: Duration,
    pub(super) extractor_panics: Vec<String>,
    pub(super) degraded_files: usize,
    /// Decision differences by unit kind and class.
    pub(super) by_kind: BTreeMap<(String, &'static str), usize>,
}

const DECISION_CLASSES: [&str; 4] = [
    "unknown_added",
    "unknown_removed",
    "anchor_changed",
    "unknown_kind_changed",
];

impl Report {
    pub(super) fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for (class, _, _) in &self.differences {
            *counts.entry(*class).or_insert(0) += 1;
        }
        counts
    }

    pub(super) fn lines(&self) -> String {
        let panics: String = self
            .extractor_panics
            .iter()
            .map(|path| format!("extractor_panic\t{path}\n"))
            .collect();
        let differences: String = self
            .differences
            .iter()
            .map(|(class, path, detail)| format!("{class}\t{path}\t{detail}\n"))
            .collect();
        panics + &differences
    }

    fn merge(&mut self, other: Report) {
        self.sources += other.sources;
        self.degraded_files += other.degraded_files;
        self.considered_units += other.considered_units;
        self.legacy_time += other.legacy_time;
        self.tree_time += other.tree_time;
        self.legacy_pass += other.legacy_pass;
        self.tree_pass += other.tree_pass;
        self.differences.extend(other.differences);
        for (key, count) in other.by_kind {
            *self.by_kind.entry(key).or_insert(0) += count;
        }
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Decision {
    anchor: Option<(String, Vec<String>)>,
    unknowns: BTreeSet<String>,
}

fn decisions(facts: &[SemanticFact]) -> BTreeMap<String, Decision> {
    let mut by_unit: BTreeMap<String, Decision> = BTreeMap::new();
    for fact in facts {
        let decision = by_unit.entry(fact.subject.clone()).or_default();
        if fact.kind == SemanticFactKind::Unknown {
            let kind = fact
                .assumptions
                .iter()
                .find_map(|assumption| assumption.strip_prefix("tsjs_unknown_kind="))
                .unwrap_or("?");
            decision.unknowns.insert(kind.to_string());
        } else {
            decision.anchor = Some((
                fact.target
                    .as_ref()
                    .map_or(String::new(), |target| target.as_str().to_string()),
                fact.assumptions.clone(),
            ));
        }
    }
    by_unit
}

fn decision_class(old: &Decision, new: &Decision) -> Option<&'static str> {
    match (&old.anchor, &new.anchor) {
        (Some(left), Some(right)) if left != right => Some("anchor_changed"),
        (Some(_), None) if new.unknowns.is_empty() => Some("anchor_changed"),
        (Some(_), None) => Some("unknown_added"),
        (None, Some(_)) if old.unknowns.is_empty() => Some("anchor_changed"),
        (None, Some(_)) => Some("unknown_removed"),
        _ if old.unknowns == new.unknowns => None,
        _ if old.unknowns.is_empty() => Some("unknown_added"),
        _ if new.unknowns.is_empty() => Some("unknown_removed"),
        _ => Some("unknown_kind_changed"),
    }
}

fn describe(decision: &Decision) -> String {
    let anchor = decision
        .anchor
        .as_ref()
        .map_or("-".to_string(), |(target, assumptions)| {
            format!("{target} [{}]", assumptions.join(","))
        });
    format!("anchor={anchor} unknown={:?}", decision.unknowns)
}

pub(super) fn language_for(path: &str) -> Language {
    if path.ends_with(".js") || path.ends_with(".jsx") {
        Language::JavaScript
    } else {
        Language::TypeScript
    }
}

fn document<'a>(path: &'a str, text: &'a str) -> SourceDocument<'a> {
    SourceDocument {
        path,
        language: language_for(path),
        content_hash: ContentHash::new(
            "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        )
        .expect("valid hash"),
        repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
        text,
    }
}

fn units_of(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
) -> Vec<CodeUnit> {
    let copy = || SourceDocument {
        path: document.path,
        language: document.language.clone(),
        content_hash: document.content_hash.clone(),
        repository_revision: document.repository_revision.clone(),
        text: document.text,
    };
    match context {
        Some(context) => {
            SyntaxCodeUnitParser
                .parse_with_context(copy(), context)
                .expect("parse with context")
                .units
        }
        None => SyntaxCodeUnitParser.parse(copy()).expect("parse").units,
    }
}

fn receiver_strings(graph: &ScopeGraphLite) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (name, receiver) in &graph.express_receivers {
        found.insert(format!("express:{name}:{receiver:?}"));
    }
    for (router, prefix) in &graph.express_router_prefixes {
        found.insert(format!("express_prefix:{router}:{prefix}"));
    }
    for (label, names) in [
        ("fastify", &graph.fastify_receivers),
        ("hono", &graph.hono_receivers),
        ("prisma", &graph.prisma_clients),
        ("drizzle_factory", &graph.drizzle_table_factories),
        ("drizzle_table", &graph.drizzle_tables),
        ("drizzle_db", &graph.drizzle_dbs),
    ] {
        for name in names {
            found.insert(format!("{label}:{name}"));
        }
    }
    found
}

fn snippet(text: &str, start: usize) -> String {
    text.get(start..)
        .unwrap_or("")
        .chars()
        .take(120)
        .map(|character| if character == '\n' { ' ' } else { character })
        .collect()
}

/// Compares the text and the tree reading of one source, with and without a
/// test-runner project context.
pub(super) fn compare(path: &str, text: &str, report: &mut Report) {
    report.sources += 1;
    let document = document(path, text);
    let started = Instant::now();
    let old = ScopeGraphLite::analyze_text(text, path, &document.language);
    report.legacy_time += started.elapsed();
    let started = Instant::now();
    let new = ScopeGraphLite::analyze(text, path, &document.language);
    report.tree_time += started.elapsed();

    let runner_context = ParserProjectContext {
        tsjs_has_test_runner_context: true,
        ..ParserProjectContext::default()
    };
    for context in [None, Some(&runner_context)] {
        let units = units_of(&document, context);
        let anchors = |graph: ScopeGraphLite| {
            super::super::tsjs::anchors_with_bindings(&document, &units, context, graph)
                .expect("anchors")
        };
        // The units the anchor pass considers are exactly those a degraded
        // graph refuses.
        let considered: BTreeSet<String> = anchors(ScopeGraphLite::degraded())
            .iter()
            .map(|fact| fact.subject.clone())
            .collect();
        let started = Instant::now();
        let old_facts = anchors(ScopeGraphLite::analyze_text(text, path, &document.language));
        report.legacy_pass += started.elapsed();
        let started = Instant::now();
        let new_facts = anchors(ScopeGraphLite::analyze(text, path, &document.language));
        report.tree_pass += started.elapsed();
        let (old_decisions, new_decisions) = (decisions(&old_facts), decisions(&new_facts));
        for unit in units
            .iter()
            .filter(|unit| considered.contains(unit.id.as_str()))
        {
            report.considered_units += 1;
            let none = Decision::default();
            let before = old_decisions.get(unit.id.as_str()).unwrap_or(&none);
            let after = new_decisions.get(unit.id.as_str()).unwrap_or(&none);
            if let Some(class) = decision_class(before, after) {
                *report
                    .by_kind
                    .entry((unit.kind.as_str().to_string(), class))
                    .or_insert(0) += 1;
                report.differences.push((
                    class,
                    path.to_string(),
                    format!(
                        "{} {}: {} || text: {} || tree: {}{}",
                        unit.kind.as_str(),
                        unit.range.start_byte,
                        snippet(text, unit.range.start_byte),
                        describe(before),
                        describe(after),
                        if context.is_some() {
                            " [runner context]"
                        } else {
                            ""
                        }
                    ),
                ));
            }
        }
    }
    if new.degraded {
        report.degraded_files += 1;
        return;
    }
    let (old_receivers, new_receivers) = (receiver_strings(&old), receiver_strings(&new));
    for receiver in old_receivers.symmetric_difference(&new_receivers) {
        let side = if old_receivers.contains(receiver) {
            "text only"
        } else {
            "tree only"
        };
        report.differences.push((
            "receiver_changed",
            path.to_string(),
            format!("{receiver} ({side})"),
        ));
    }
}

fn walk(directory: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for entry in entries {
        let name = entry
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if entry.is_dir() {
            if !["node_modules", ".git", "target", "chrome-profile", ".next"].contains(&name) {
                walk(&entry, found);
            }
        } else if [".ts", ".tsx", ".js", ".jsx"]
            .iter()
            .any(|extension| name.ends_with(extension))
            && !name.ends_with(".d.ts")
        {
            found.push(entry);
        }
    }
}

const MAX_SOURCE_BYTES: usize = 400_000;

/// Compares every TS/JS source under `roots`; non-UTF-8 and oversized files are
/// skipped and counted. Directories named like an installed package are walked
/// when passed as a root.
pub(super) fn compare_roots(roots: &[std::path::PathBuf], report: &mut Report) -> usize {
    let mut skipped = 0;
    for root in roots {
        let mut paths = Vec::new();
        walk(root, &mut paths);
        for path in paths {
            let Ok(text) = std::fs::read_to_string(&path)
                .ok()
                .filter(|text| text.len() <= MAX_SOURCE_BYTES)
                .ok_or(())
            else {
                skipped += 1;
                continue;
            };
            let display = path.to_string_lossy().replace('\\', "/");
            // The line-based unit extractor (slice 3) can panic on non-ASCII
            // input; such a source is counted, not compared.
            let compared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut one = Report::default();
                compare(&display, &text, &mut one);
                one
            }));
            match compared {
                Ok(one) => report.merge(one),
                Err(_) => report.extractor_panics.push(display),
            }
        }
    }
    skipped
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures() -> Report {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures");
        let mut report = Report::default();
        compare_roots(&[root], &mut report);
        report
    }

    fn decision_classes(report: &Report) -> BTreeSet<&'static str> {
        report
            .differences
            .iter()
            .map(|(class, _, _)| *class)
            .filter(|class| DECISION_CLASSES.contains(class))
            .collect()
    }

    /// Every branch case differs exactly as attributed: the text reading
    /// misread the source (the listed class) and the tree reads it correctly,
    /// or the source is a syntax error (ADR-0059 D4).
    #[test]
    fn branch_corpus_decisions_match_their_attribution() {
        let mut wrong = Vec::new();
        for (label, path, text, expected) in super::super::anchor_corpus::branch_corpus() {
            let mut report = Report::default();
            compare(path, &text, &mut report);
            let found: Vec<&str> = decision_classes(&report).into_iter().collect();
            let mut expected: Vec<&str> = expected.to_vec();
            expected.sort_unstable();
            if found != expected {
                wrong.push(format!(
                    "{label} {path}: expected {expected:?}, found {found:?}\n{}",
                    report.lines()
                ));
            }
            assert_eq!(
                report.degraded_files,
                usize::from(label == "syntax_error"),
                "{path}"
            );
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// The committed fixtures: every difference is accounted for. The extractor
    /// starts a Prisma-shaped unit at `query.users.findMany(` inside
    /// `db.query.users.findMany(` (`member_receiver`); both readings give an
    /// UNKNOWN, with a different kind. Two units, each without and with a
    /// test-runner context.
    #[test]
    fn committed_fixtures_differ_only_where_attributed() {
        let report = fixtures();
        assert!(report.sources >= 82, "{}", report.sources);
        assert_eq!(report.degraded_files, 0);
        assert_eq!(
            report.counts(),
            BTreeMap::from([("unknown_kind_changed", 4)]),
            "{}",
            report.lines()
        );
        assert!(
            report.differences.iter().all(|(_, path, detail)| path
                .ends_with("drizzle_exact_repositories/src/repositories.ts")
                && detail.starts_with("prisma_query")),
            "{}",
            report.lines()
        );
    }

    /// Set `REPOGRAMMAR_TSJS_DIFF_ROOTS` (colon-separated directories) and
    /// `REPOGRAMMAR_TSJS_DIFF_OUT`.
    #[test]
    #[ignore = "set REPOGRAMMAR_TSJS_DIFF_{ROOTS,OUT}"]
    fn external_corpus_differential() {
        let variable = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name}"));
        let roots: Vec<std::path::PathBuf> = variable("REPOGRAMMAR_TSJS_DIFF_ROOTS")
            .split(':')
            .map(std::path::PathBuf::from)
            .collect();
        let mut report = Report::default();
        let skipped = compare_roots(&roots, &mut report);
        let summary = format!(
            "sources={} degraded_files={} skipped={skipped} extractor_panics={} considered_units={} counts={:?} by_kind={:?} text_analyze={:?} tree_analyze={:?} text_pass={:?} tree_pass={:?}\n",
            report.sources,
            report.degraded_files,
            report.extractor_panics.len(),
            report.considered_units,
            report.counts(),
            report.by_kind,
            report.legacy_time,
            report.tree_time,
            report.legacy_pass,
            report.tree_pass
        );
        std::fs::write(
            variable("REPOGRAMMAR_TSJS_DIFF_OUT"),
            format!("{summary}{}", report.lines()),
        )
        .expect("write differential output");
        eprintln!("{summary}");
    }
}
