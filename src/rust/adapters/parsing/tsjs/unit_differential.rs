//! ADR-0059 slice-3 differential: the line-based unit extractor (kept under
//! `cfg(test)`, D6) against the tree extractor, over committed fixtures, a
//! branch corpus and an external corpus. Units are matched by kind, name and
//! start byte and classified as in
//! `docs/experiments/tsjs-unit-extraction-tree.md`; the anchor decision of
//! every matched unit is compared as in slice 2.

use super::scope_graph::ScopeGraphLite;
use crate::adapters::parsing::legacy_units::LegacyUnitParser;
use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
use crate::core::model::{
    ContentHash, Language, RepositoryRevision, SemanticFact, SemanticFactKind,
};
use crate::ports::parser::{ParseReport, ParserProjectContext, SourceDocument, SourceParser};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// One difference: the class, the source, and a detail line.
pub(super) type Difference = (&'static str, String, String);

#[derive(Debug, Default)]
pub(super) struct Report {
    pub(super) differences: Vec<Difference>,
    pub(super) sources: usize,
    pub(super) legacy_units: usize,
    pub(super) tree_units: usize,
    pub(super) legacy_time: Duration,
    pub(super) tree_time: Duration,
    pub(super) panics: Vec<String>,
    pub(super) degraded_files: usize,
    /// (class, reason) -> count.
    pub(super) by_reason: BTreeMap<(&'static str, String), usize>,
}

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
            .panics
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
        self.legacy_units += other.legacy_units;
        self.tree_units += other.tree_units;
        self.legacy_time += other.legacy_time;
        self.tree_time += other.tree_time;
        self.degraded_files += other.degraded_files;
        self.differences.extend(other.differences);
        for (key, count) in other.by_reason {
            *self.by_reason.entry(key).or_insert(0) += count;
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Decision {
    anchor: Option<(String, Vec<String>)>,
    unknowns: BTreeSet<String>,
}

impl Decision {
    fn describe(&self) -> String {
        let anchor = self
            .anchor
            .as_ref()
            .map_or("-".to_string(), |(target, assumptions)| {
                format!("{target} [{}]", assumptions.join(","))
            });
        format!("anchor={anchor} unknown={:?}", self.unknowns)
    }
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

/// The decision class of one matched unit: `anchor_changed` for any anchor
/// difference (an anchor that becomes an UNKNOWN or the reverse included, since
/// it moves family support), `unknown_kind_changed` for UNKNOWN on both sides.
fn decision_class(old: &Decision, new: &Decision) -> Option<&'static str> {
    if old == new {
        return None;
    }
    if old.anchor.is_none()
        && new.anchor.is_none()
        && !old.unknowns.is_empty()
        && !new.unknowns.is_empty()
    {
        return Some("unknown_kind_changed");
    }
    Some("anchor_changed")
}

#[derive(Debug, Clone)]
struct Unit {
    kind: String,
    name: String,
    start: usize,
    end: usize,
    decision: Decision,
}

fn name_of(id: &str, kind: &str) -> String {
    let marker = format!("#{kind}:");
    let body = id.split_once(&marker).map_or(id, |(_, rest)| rest);
    let mut parts = body.rsplitn(3, ':');
    parts.next();
    parts.next();
    parts.next().unwrap_or("").to_string()
}

fn units_of(report: &ParseReport) -> Vec<Unit> {
    let by_unit = decisions(&report.semantic_facts);
    report
        .units
        .iter()
        .map(|unit| Unit {
            kind: unit.kind.as_str().to_string(),
            name: name_of(unit.id.as_str(), unit.kind.as_str()),
            start: unit.range.start_byte,
            end: unit.range.end_byte,
            decision: by_unit.get(unit.id.as_str()).cloned().unwrap_or_default(),
        })
        .collect()
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

fn snippet(text: &str, start: usize) -> String {
    text.get(start..)
        .unwrap_or("")
        .chars()
        .take(100)
        .map(|character| if character == '\n' { ' ' } else { character })
        .collect()
}

fn line_of(text: &str, offset: usize) -> usize {
    text.as_bytes()[..offset.min(text.len())]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
}

/// Why the line reader made a unit the tree does not: read off the tree.
fn retired_reason(text: &str, graph: &ScopeGraphLite, unit: &Unit) -> String {
    let Some(tree) = graph.tree.as_ref() else {
        return "no_tree".to_string();
    };
    let Some(mut node) = tree
        .root_node()
        .descendant_for_byte_range(unit.start, unit.start + 1)
    else {
        return "other".to_string();
    };
    let at = node;
    loop {
        if matches!(
            node.kind(),
            "comment" | "string" | "template_string" | "regex" | "jsx_text"
        ) {
            return "not_code:text".to_string();
        }
        match node.parent() {
            Some(parent) => node = parent,
            None => break,
        }
    }
    if matches!(unit.kind.as_str(), "test_suite" | "test_case")
        && at.kind() == "property_identifier"
    {
        return "not_code:method_name".to_string();
    }
    if unit.kind == "method" {
        return "statement_in_class".to_string();
    }
    let _ = text;
    "other".to_string()
}

fn added_reason(text: &str, old: &[Unit], unit: &Unit) -> String {
    let before = text.get(..unit.start).unwrap_or("").trim_end();
    if unit.kind == "method" && (before.ends_with("get") || before.ends_with("set")) {
        return "accessor_name".to_string();
    }
    if matches!(
        unit.kind.as_str(),
        "arrow_function" | "react_component" | "react_hook"
    ) {
        return "declarator_shape".to_string();
    }
    let line = line_of(text, unit.start);
    let same_line = old
        .iter()
        .any(|other| other.kind == unit.kind && line_of(text, other.start) == line);
    if same_line {
        "second_unit_on_line".to_string()
    } else {
        "line_reader_missed".to_string()
    }
}

/// Classifies the units of one parse pair.
fn classify(
    path: &str,
    text: &str,
    graph: &ScopeGraphLite,
    old: &[Unit],
    new: &[Unit],
    context: bool,
    report: &mut Report,
) {
    let suffix = if context { " [runner context]" } else { "" };
    let mut push = |class: &'static str, reason: &str, detail: String| {
        *report
            .by_reason
            .entry((class, reason.to_string()))
            .or_insert(0) += 1;
        report.differences.push((
            class,
            path.to_string(),
            format!("{reason}: {detail}{suffix}"),
        ));
    };
    let mut free: Vec<bool> = vec![true; new.len()];
    let mut unmatched_old: Vec<usize> = Vec::new();
    for (index, unit) in old.iter().enumerate() {
        let found = new.iter().enumerate().find(|(slot, candidate)| {
            free[*slot]
                && candidate.kind == unit.kind
                && candidate.name == unit.name
                && candidate.start == unit.start
        });
        match found {
            Some((slot, candidate)) => {
                free[slot] = false;
                if candidate.end != unit.end {
                    push(
                        "unit_boundary_only",
                        &unit.kind,
                        format!(
                            "{} {}: end {} -> {}",
                            unit.kind, unit.start, unit.end, candidate.end
                        ),
                    );
                }
                if let Some(class) = decision_class(&unit.decision, &candidate.decision) {
                    push(
                        class,
                        &unit.kind,
                        format!(
                            "{} {}: {} || old: {} || new: {}",
                            unit.kind,
                            unit.start,
                            snippet(text, unit.start),
                            unit.decision.describe(),
                            candidate.decision.describe()
                        ),
                    );
                }
            }
            None => unmatched_old.push(index),
        }
    }
    let mut leftover_old = Vec::new();
    for index in unmatched_old {
        let unit = &old[index];
        // Same name and start, another kind: `jsx_text_lookalike` (a React
        // component by `</` text, a function by the tree).
        let renamed = (0..new.len()).find(|slot| {
            free[*slot]
                && new[*slot].kind != unit.kind
                && new[*slot].name == unit.name
                && new[*slot].start == unit.start
        });
        if let Some(slot) = renamed {
            free[slot] = false;
            push(
                "unit_kind_changed",
                &format!("{} -> {}", unit.kind, new[slot].kind),
                format!(
                    "{} {}: {}",
                    unit.kind,
                    unit.start,
                    snippet(text, unit.start)
                ),
            );
            continue;
        }
        // The same unit started elsewhere: the ranges overlap.
        let container = (0..new.len()).find(|slot| {
            free[*slot]
                && new[*slot].kind == unit.kind
                && new[*slot].name == unit.name
                && new[*slot].start < unit.end
                && unit.start < new[*slot].end
        });
        match container {
            Some(slot) => {
                free[slot] = false;
                push(
                    "unit_start_corrected",
                    &unit.kind,
                    format!(
                        "{} {} -> {}: {} || old: {} || new: {}",
                        unit.kind,
                        unit.start,
                        new[slot].start,
                        snippet(text, new[slot].start),
                        unit.decision.describe(),
                        new[slot].decision.describe()
                    ),
                );
                if let Some(class) = decision_class(&unit.decision, &new[slot].decision) {
                    push(
                        class,
                        "start_corrected",
                        format!(
                            "{} {} -> {}: {} || old: {} || new: {}",
                            unit.kind,
                            unit.start,
                            new[slot].start,
                            snippet(text, new[slot].start),
                            unit.decision.describe(),
                            new[slot].decision.describe()
                        ),
                    );
                }
            }
            None => leftover_old.push(index),
        }
    }
    let mut retired = Vec::new();
    for index in leftover_old {
        let unit = &old[index];
        let inside: Vec<usize> = (0..new.len())
            .filter(|slot| {
                free[*slot]
                    && new[*slot].kind == unit.kind
                    && unit.start <= new[*slot].start
                    && new[*slot].start < unit.end
            })
            .collect();
        if inside.len() >= 2 {
            for slot in &inside {
                free[*slot] = false;
            }
            push(
                "unit_split",
                &unit.kind,
                format!(
                    "{} {}: {}",
                    unit.kind,
                    unit.start,
                    snippet(text, unit.start)
                ),
            );
        } else {
            retired.push(index);
        }
    }
    for index in retired {
        let unit = &old[index];
        let reason = retired_reason(text, graph, unit);
        push(
            "unit_retired",
            &reason,
            format!(
                "{} {}: {} || old: {}",
                unit.kind,
                unit.start,
                snippet(text, unit.start),
                unit.decision.describe()
            ),
        );
    }
    for slot in 0..new.len() {
        if !free[slot] {
            continue;
        }
        let unit = &new[slot];
        let covered: Vec<usize> = (0..old.len())
            .filter(|index| {
                old[*index].kind == unit.kind
                    && unit.start <= old[*index].start
                    && old[*index].start < unit.end
                    && !new.iter().any(|other| {
                        other.kind == unit.kind
                            && other.start == old[*index].start
                            && other.name == old[*index].name
                    })
            })
            .collect();
        if covered.len() >= 2 {
            push(
                "unit_merged",
                &unit.kind,
                format!(
                    "{} {}: {}",
                    unit.kind,
                    unit.start,
                    snippet(text, unit.start)
                ),
            );
            continue;
        }
        let reason = added_reason(text, old, unit);
        push(
            "unit_added",
            &reason,
            format!(
                "{} {}: {} || new: {}",
                unit.kind,
                unit.start,
                snippet(text, unit.start),
                unit.decision.describe()
            ),
        );
    }
}

/// Compares the two extractors on one source, without and with a test-runner
/// project context.
pub(super) fn compare(path: &str, text: &str, report: &mut Report) {
    report.sources += 1;
    let graph = ScopeGraphLite::analyze(text, path, &language_for(path));
    if graph.degraded {
        report.degraded_files += 1;
        // A degraded file is counted, and its unit set is compared like any
        // other (`degraded_unit_set` is read off these rows by file).
    }
    let runner_context = ParserProjectContext {
        tsjs_has_test_runner_context: true,
        ..ParserProjectContext::default()
    };
    for context in [None, Some(&runner_context)] {
        let started = Instant::now();
        let old = match context {
            Some(context) => LegacyUnitParser.parse_with_context(document(path, text), context),
            None => LegacyUnitParser.parse(document(path, text)),
        }
        .expect("legacy parse");
        report.legacy_time += started.elapsed();
        let started = Instant::now();
        let new = match context {
            Some(context) => SyntaxCodeUnitParser.parse_with_context(document(path, text), context),
            None => SyntaxCodeUnitParser.parse(document(path, text)),
        }
        .expect("tree parse");
        report.tree_time += started.elapsed();
        let (old_units, new_units) = (units_of(&old), units_of(&new));
        report.legacy_units += old_units.len();
        report.tree_units += new_units.len();
        let before = report.differences.len();
        classify(
            path,
            text,
            &graph,
            &old_units,
            &new_units,
            context.is_some(),
            report,
        );
        if graph.degraded {
            for difference in &mut report.differences[before..] {
                difference.2 = format!("[degraded file] {}", difference.2);
            }
        }
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
/// skipped and counted.
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
            let compared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut one = Report::default();
                compare(&display, &text, &mut one);
                one
            }));
            match compared {
                Ok(one) => report.merge(one),
                Err(_) => report.panics.push(display),
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

    fn classes(report: &Report) -> Vec<&'static str> {
        let found: BTreeSet<&'static str> = report
            .differences
            .iter()
            .map(|(class, _, _)| *class)
            .filter(|class| *class != "unit_boundary_only")
            .collect();
        found.into_iter().collect()
    }

    /// Every branch case differs exactly as attributed to its misreading
    /// class (or not at all beyond the unit end).
    #[test]
    fn branch_corpus_differences_match_their_attribution() {
        let mut wrong = Vec::new();
        for (label, path, text, expected) in super::super::unit_corpus::branch_corpus() {
            let mut report = Report::default();
            compare(path, &text, &mut report);
            let found = classes(&report);
            let mut expected: Vec<&str> = expected.to_vec();
            expected.sort_unstable();
            if found != expected {
                wrong.push(format!(
                    "{label} {path}: expected {expected:?}, found {found:?}\n{}",
                    report.lines()
                ));
            }
        }
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    }

    /// The committed fixtures: every difference is accounted for. Two units
    /// (each without and with a test-runner context) in
    /// `drizzle_exact_repositories` start mid-chain at `query.users.findMany(`
    /// (`member_receiver`, `unit_start_corrected`); `lookalikes.ts` has the word
    /// `function` in a comment (`unit_not_code`); and the Drizzle `select` of
    /// `listOrders` read `where:` from the next function's body, because the
    /// line reader's unit ran to the next brace (`extent_by_first_brace`):
    /// `anchor_changed`, which needs maintainer acceptance (ADR-0059 D5).
    #[test]
    fn committed_fixtures_differ_only_where_attributed() {
        let report = fixtures();
        assert!(report.sources >= 82, "{}", report.sources);
        assert_eq!(report.degraded_files, 0);
        assert_eq!(
            report.counts(),
            BTreeMap::from([
                ("anchor_changed", 2),
                ("unit_boundary_only", 258),
                ("unit_retired", 2),
                ("unit_start_corrected", 4),
            ]),
            "{}",
            report.lines()
        );
        for (class, path, detail) in &report.differences {
            match *class {
                "anchor_changed" => assert!(
                    path.ends_with("drizzle_exact_repositories/src/repositories.ts")
                        && detail.contains("drizzle_query 513"),
                    "{path} {detail}"
                ),
                "unit_start_corrected" => assert!(
                    path.ends_with("drizzle_exact_repositories/src/repositories.ts")
                        && detail.contains("prisma_query"),
                    "{path} {detail}"
                ),
                "unit_retired" => assert!(
                    path.ends_with("express_exact_routes/lookalikes.ts")
                        && detail.starts_with("not_code:text"),
                    "{path} {detail}"
                ),
                _ => {}
            }
        }
    }

    #[test]
    #[ignore = "prints the fixture differential"]
    fn print_fixture_differential() {
        let report = fixtures();
        eprintln!(
            "sources={} legacy_units={} tree_units={} counts={:?} by_reason={:?}\n{}",
            report.sources,
            report.legacy_units,
            report.tree_units,
            report.counts(),
            report.by_reason,
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
            "sources={} degraded_files={} skipped={skipped} panics={} legacy_units={} tree_units={} counts={:?} by_reason={:?} legacy_parse={:?} tree_parse={:?}\n",
            report.sources,
            report.degraded_files,
            report.panics.len(),
            report.legacy_units,
            report.tree_units,
            report.counts(),
            report.by_reason,
            report.legacy_time,
            report.tree_time
        );
        std::fs::write(
            variable("REPOGRAMMAR_TSJS_DIFF_OUT"),
            format!("{summary}{}", report.lines()),
        )
        .expect("write differential output");
        eprintln!("{summary}");
    }
}
