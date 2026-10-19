//! ADR-0059 slice-1 differential: the legacy line reader against the tree
//! reader, over committed fixtures, a branch corpus and an external corpus.
//! Temporary scaffolding, deleted with the legacy reader.

use super::{ImportBinding, ScopeGraphLite};
use crate::adapters::parsing::syntax::SyntaxCodeUnitParser;
use crate::core::model::{
    CodeUnit, ContentHash, Language, RepositoryRevision, SemanticFact, SemanticFactKind,
};
use crate::ports::parser::{ParserProjectContext, SourceDocument, SourceParser};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

/// One difference: the preregistered class, the source, and a short detail.
type Difference = (&'static str, String, String);

#[derive(Debug, Default)]
pub(super) struct Report {
    pub(super) differences: Vec<Difference>,
    pub(super) sources: usize,
    pub(super) considered_units: usize,
    pub(super) legacy_time: Duration,
    pub(super) tree_time: Duration,
    pub(super) extractor_panics: Vec<String>,
    pub(super) degraded_files: usize,
}

impl Report {
    pub(super) fn count(&self, class: &str) -> usize {
        self.differences
            .iter()
            .filter(|(found, _, _)| *found == class)
            .count()
    }

    pub(super) fn counts(&self) -> BTreeMap<&'static str, usize> {
        let mut counts = BTreeMap::new();
        for (class, _, _) in &self.differences {
            *counts.entry(*class).or_insert(0) += 1;
        }
        counts
    }

    pub(super) fn decision_differences(&self) -> usize {
        [
            "unknown_added",
            "unknown_removed",
            "anchor_changed",
            "unknown_kind_changed",
        ]
        .iter()
        .map(|class| self.count(class))
        .sum()
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

fn language_for(path: &str) -> Language {
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

fn line_start(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())]
        .rfind('\n')
        .map_or(0, |index| index + 1)
}

fn import_signature(binding: &ImportBinding) -> String {
    format!("{}:{:?}", binding.module, binding.kind)
}

fn set_differences(
    report: &mut Vec<Difference>,
    path: &str,
    added: &'static str,
    removed: &'static str,
    old: &BTreeSet<String>,
    new: &BTreeSet<String>,
) {
    for name in new.difference(old) {
        report.push((added, path.to_string(), name.clone()));
    }
    for name in old.difference(new) {
        report.push((removed, path.to_string(), name.clone()));
    }
}

fn events(graph: &ScopeGraphLite, text: &str) -> BTreeSet<String> {
    graph
        .unsafe_events
        .iter()
        .flat_map(|(name, events)| {
            events
                .iter()
                .map(move |event| format!("{name}@{}", line_start(text, event.byte)))
        })
        .collect()
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

/// The first source line that mentions `name` next to a declaring keyword,
/// to help attribute a field difference by hand.
fn context(text: &str, name: &str) -> String {
    let name = name.split(['=', ':', '@', ' ']).next().unwrap_or(name);
    text.lines()
        .find(|line| {
            line.contains(name)
                && [
                    "const ", "let ", "var ", "function", "class ", "import", "require", "= ",
                ]
                .iter()
                .any(|keyword| line.contains(keyword))
        })
        .map(|line| line.trim().chars().take(110).collect())
        .unwrap_or_default()
}

/// Where the first `ERROR` or `MISSING` node is, for the attribution of a
/// degraded file.
fn first_error(text: &str, path: &str) -> String {
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&super::grammar(path, &language_for(path)))
        .expect("grammar");
    let Some(tree) = parser.parse(text, None) else {
        return "no tree (work bound or parse failure)".to_string();
    };
    let mut cursor = tree.walk();
    let mut descend = true;
    loop {
        let node = cursor.node();
        if descend && (node.is_error() || node.is_missing()) {
            let line: String = text
                .lines()
                .nth(node.start_position().row)
                .unwrap_or("")
                .trim()
                .chars()
                .take(90)
                .collect();
            return format!(
                "{} at {}:{} || {line}",
                if node.is_missing() {
                    "MISSING"
                } else {
                    "ERROR"
                },
                node.start_position().row + 1,
                node.start_position().column + 1
            );
        }
        if descend && cursor.goto_first_child() {
            continue;
        }
        if cursor.goto_next_sibling() {
            descend = true;
        } else if cursor.goto_parent() {
            descend = false;
        } else {
            return "work bound exceeded".to_string();
        }
    }
}

/// Which scope held a name on the side that said unsafe: a brace pair the other
/// side has no scope for, or the same scope with different content.
fn shadow_cause(
    old: &ScopeGraphLite,
    new: &ScopeGraphLite,
    name: &str,
    byte: usize,
    root: tree_sitter::Node<'_>,
) -> String {
    let (holder_graph, other) = if old.name_is_unsafe_at(name, byte) {
        (old, new)
    } else {
        (new, old)
    };
    let holders: Vec<_> = holder_graph
        .scope_intervals
        .iter()
        .filter(|scope| {
            scope.start_byte <= byte
                && byte < scope.end_byte
                && (scope.local_decls.contains(name) || scope.params.contains(name))
        })
        .collect();
    if holders.is_empty() {
        return "name_or_event".to_string();
    }
    let counterpart = |holder: &super::ScopeInterval| {
        other
            .scope_intervals
            .iter()
            .any(|scope| scope.start_byte == holder.start_byte && scope.end_byte == holder.end_byte)
    };
    if holders.iter().any(|holder| counterpart(holder)) {
        return "same_scope_other_content".to_string();
    }
    // The tree node that spans the one-sided brace pair, if any.
    let holder = holders[0];
    let mut node = root.descendant_for_byte_range(holder.start_byte, holder.start_byte + 1);
    while let Some(candidate) = node {
        if candidate.start_byte() == holder.start_byte && candidate.end_byte() == holder.end_byte {
            return format!("one_side_scope:{}", candidate.kind());
        }
        node = candidate.parent();
    }
    "one_side_scope:misaligned".to_string()
}

fn field_differences(
    path: &str,
    text: &str,
    old: &ScopeGraphLite,
    new: &ScopeGraphLite,
    starts: &[usize],
) -> Vec<Difference> {
    let mut found = Vec::new();
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&super::grammar(path, &language_for(path)))
        .expect("grammar");
    let tree = parser.parse(text, None).expect("tree");
    for (name, binding) in &new.imports {
        match old.imports.get(name) {
            None => found.push((
                "import_added",
                path.to_string(),
                format!("{name}={}", import_signature(binding)),
            )),
            Some(before) if before != binding => found.push((
                "import_changed",
                path.to_string(),
                format!(
                    "{name}: {} -> {}",
                    import_signature(before),
                    import_signature(binding)
                ),
            )),
            Some(_) => {}
        }
    }
    for (name, binding) in &old.imports {
        if !new.imports.contains_key(name) {
            found.push((
                "import_removed",
                path.to_string(),
                format!("{name}={}", import_signature(binding)),
            ));
        }
    }
    set_differences(
        &mut found,
        path,
        "decl_added",
        "decl_removed",
        &old.local_decls,
        &new.local_decls,
    );
    set_differences(
        &mut found,
        path,
        "unsafe_added",
        "unsafe_removed",
        &old.unsafe_names,
        &new.unsafe_names,
    );
    set_differences(
        &mut found,
        path,
        "event_added",
        "event_removed",
        &events(old, text),
        &events(new, text),
    );
    let (old_receivers, new_receivers) = (receiver_strings(old), receiver_strings(new));
    for receiver in old_receivers.symmetric_difference(&new_receivers) {
        let side = if old_receivers.contains(receiver) {
            "legacy only"
        } else {
            "tree only"
        };
        found.push((
            "receiver_changed",
            path.to_string(),
            format!("{receiver} ({side})"),
        ));
    }
    // Shadowing is only consulted for names the lane binds, so compare those
    // names at the start of every considered unit.
    let names: BTreeSet<&String> = old
        .imports
        .keys()
        .chain(new.imports.keys())
        .chain(&old.local_decls)
        .chain(&new.local_decls)
        .chain(old.express_receivers.keys())
        .chain(new.express_receivers.keys())
        .chain(&old.fastify_receivers)
        .chain(&new.fastify_receivers)
        .chain(&old.hono_receivers)
        .chain(&new.hono_receivers)
        .chain(&old.prisma_clients)
        .chain(&new.prisma_clients)
        .chain(&old.drizzle_dbs)
        .chain(&new.drizzle_dbs)
        .chain(&old.drizzle_tables)
        .chain(&new.drizzle_tables)
        .collect();
    for start in starts {
        for name in &names {
            let (before, after) = (
                old.name_is_unsafe_at(name, *start),
                new.name_is_unsafe_at(name, *start),
            );
            if before != after {
                found.push((
                    "shadow_changed",
                    path.to_string(),
                    format!(
                        "{name}@{start}: legacy {before} -> tree {after} [{}]",
                        shadow_cause(old, new, name, *start, tree.root_node())
                    ),
                ));
            }
        }
    }
    for (class, _, detail) in &mut found {
        if *class != "receiver_changed" {
            *detail = format!("{detail} || {}", context(text, detail));
        }
    }
    found
}

fn units_of(
    document: &SourceDocument<'_>,
    context: Option<&ParserProjectContext>,
) -> Vec<CodeUnit> {
    let report = match context {
        Some(context) => SyntaxCodeUnitParser
            .parse_with_context(
                SourceDocument {
                    path: document.path,
                    language: document.language.clone(),
                    content_hash: document.content_hash.clone(),
                    repository_revision: document.repository_revision.clone(),
                    text: document.text,
                },
                context,
            )
            .expect("parse with context"),
        None => SyntaxCodeUnitParser
            .parse(SourceDocument {
                path: document.path,
                language: document.language.clone(),
                content_hash: document.content_hash.clone(),
                repository_revision: document.repository_revision.clone(),
                text: document.text,
            })
            .expect("parse"),
    };
    report.units
}

/// Compares the legacy and the tree graph of one source, with and without a
/// test-runner project context.
pub(super) fn compare(path: &str, text: &str, report: &mut Report) {
    report.sources += 1;
    let document = document(path, text);
    let started = Instant::now();
    let old = ScopeGraphLite::analyze_legacy(text);
    report.legacy_time += started.elapsed();
    let started = Instant::now();
    let new = ScopeGraphLite::analyze(text, path, &document.language);
    report.tree_time += started.elapsed();

    let runner_context = ParserProjectContext {
        tsjs_has_test_runner_context: true,
        ..ParserProjectContext::default()
    };
    let mut starts: BTreeSet<usize> = BTreeSet::new();
    for context in [None, Some(&runner_context)] {
        let units = units_of(&document, context);
        // The units the anchor pass considers are exactly those a degraded
        // graph refuses.
        let considered: BTreeSet<String> = super::super::anchors_with_bindings(
            &document,
            &units,
            context,
            ScopeGraphLite::degraded(),
        )
        .expect("degraded anchors")
        .iter()
        .map(|fact| fact.subject.clone())
        .collect();
        let old_facts =
            super::super::anchors_with_bindings(&document, &units, context, old.clone())
                .expect("legacy anchors");
        let new_facts =
            super::super::anchors_with_bindings(&document, &units, context, new.clone())
                .expect("tree anchors");
        let (old_decisions, new_decisions) = (decisions(&old_facts), decisions(&new_facts));
        for unit in units
            .iter()
            .filter(|unit| considered.contains(unit.id.as_str()))
        {
            starts.insert(unit.range.start_byte);
            report.considered_units += 1;
            let none = Decision::default();
            let before = old_decisions.get(unit.id.as_str()).unwrap_or(&none);
            let after = new_decisions.get(unit.id.as_str()).unwrap_or(&none);
            if let Some(class) = decision_class(before, after) {
                report.differences.push((
                    class,
                    path.to_string(),
                    format!(
                        "{} {}@{}: {:?} -> {:?}{}",
                        unit.kind.as_str(),
                        unit.id.as_str(),
                        unit.range.start_byte,
                        before,
                        after,
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
        report
            .differences
            .push(("syntax_error", path.to_string(), first_error(text, path)));
        return;
    }
    let starts: Vec<usize> = starts.into_iter().collect();
    report
        .differences
        .extend(field_differences(path, text, &old, &new, &starts));
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
            // Minified bundles are outside the corpus: the legacy reader is
            // quadratic on them, so a bound keeps the run finite.
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
                Ok(one) => {
                    report.sources += one.sources;
                    report.degraded_files += one.degraded_files;
                    report.considered_units += one.considered_units;
                    report.legacy_time += one.legacy_time;
                    report.tree_time += one.tree_time;
                    report.differences.extend(one.differences);
                }
                Err(_) => report.extractor_panics.push(display),
            }
        }
    }
    skipped
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (misreading class, path, source). The expected differences are listed
    /// beside the harness below.
    const EXPRESS_HEAD: &str = "import express, { Router } from \"express\";\n";

    fn branch_corpus() -> Vec<(&'static str, &'static str, String)> {
        let express_app = format!("{EXPRESS_HEAD}const app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n");
        vec![
            // ---- equivalence: every receiver form and near-miss that both readers get right
            ("none", "app.ts", express_app.clone()),
            ("none", "router.ts", format!("{EXPRESS_HEAD}const router = Router();\nconst app = express();\nrouter.get(\"/r\", (req, res) => res.send(\"r\"));\napp.use(\"/api\", router);\napp.get(\"/b\", (req, res) => res.send(\"b\"));\n")),
            ("none", "ns_router.ts", "import * as ex from \"express\";\nconst router = ex.Router();\nrouter.post(\"/p\", (req, res) => res.send(\"p\"));\n".to_string()),
            ("none", "require_express.js", "const express = require(\"express\");\nconst { Router } = require(\"express\");\nconst app = express();\nconst r = Router();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\nr.get(\"/b\", (req, res) => res.send(\"b\"));\n".to_string()),
            ("none", "fastify.ts", "import Fastify from \"fastify\";\nconst app = Fastify();\napp.get(\"/f\", async () => ({ ok: true }));\napp.register(plugin, { prefix: \"/p\" });\n".to_string()),
            ("none", "hono.ts", "import { Hono } from \"hono\";\ntype Env = { Bindings: {} };\nconst app = new Hono<Env>();\nconst plain = new Hono();\napp.get(\"/h\", (c) => c.text(\"h\"));\nplain.post(\"/q\", (c) => c.text(\"q\"));\n".to_string()),
            ("none", "prisma.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\nexport async function list() {\n  return prisma.user.findMany({ where: { active: true } });\n}\nexport async function tx() {\n  return prisma.$transaction([prisma.user.findMany()]);\n}\n".to_string()),
            ("none", "drizzle.ts", "import { drizzle } from \"drizzle-orm/node-postgres\";\nimport { pgTable, serial, text } from \"drizzle-orm/pg-core\";\nexport const users = pgTable(\"users\", { id: serial(\"id\").primaryKey(), name: text(\"name\") });\nconst db = drizzle(pool);\nexport async function list() {\n  return db.select().from(users);\n}\nexport async function tx() {\n  return db.transaction(async (t) => t.select().from(users));\n}\n".to_string()),
            ("none", "jest.test.ts", "import { describe, it, expect } from \"vitest\";\ndescribe(\"suite\", () => {\n  it(\"works\", () => {\n    expect(1).toBe(1);\n  });\n});\n".to_string()),
            ("none", "nest.ts", "import { Controller, Get, Injectable, Module } from \"@nestjs/common\";\n@Injectable()\nexport class Svc {}\n@Controller(\"users\")\nexport class UsersController {\n  @Get(\"all\")\n  findAll() {\n    return [];\n  }\n}\n@Module({ controllers: [UsersController] })\nexport class AppModule {}\n".to_string()),
            ("none", "zod.ts", "import { z } from \"zod\";\nexport const UserSchema = z.object({ name: z.string(), age: z.number().optional() });\nexport const Other = z.string().min(2);\n".to_string()),
            ("none", "balanced_literals.ts", format!("{EXPRESS_HEAD}const app = express();\n// a comment with balanced {{ }} braces\nconst t = `value ${{1 + 1}} done`;\nconst o = {{ a: 1 }};\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("none", "shadowed_param.ts", format!("{EXPRESS_HEAD}const app = express();\nfunction inner(app) {{\n  app.get(\"/x\", (req, res) => res.send(\"x\"));\n}}\napp.get(\"/y\", (req, res) => res.send(\"y\"));\n")),
            ("none", "redeclared.ts", format!("{EXPRESS_HEAD}const app = express();\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("none", "reassigned.ts", format!("{EXPRESS_HEAD}let app = express();\napp = other();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("none", "conditional_import.ts", "if (flag) {\n  const express = require(\"express\");\n}\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            // ---- multi_line_clause
            ("multi_line_clause", "multi_import.ts", "import express, {\n  Router,\n  json,\n} from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("multi_line_clause", "multi_require.js", "const {\n  Router,\n} = require(\"express\");\nconst r = Router();\nr.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("multi_line_clause", "multi_decl.ts", format!("{EXPRESS_HEAD}const app =\n  express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            // ---- brace_in_literal_or_comment
            ("brace_in_literal_or_comment", "brace_string_open.ts", "const brace = \"{\";\nimport express from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("brace_in_literal_or_comment", "brace_string_close.ts", format!("{EXPRESS_HEAD}const app = express();\nfunction f() {{\n  const s = \"}}\";\n  const express = 1;\n  return s + express;\n}}\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("brace_in_literal_or_comment", "brace_comment.ts", "// {\nimport express from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("brace_in_literal_or_comment", "brace_block_comment.ts", "/* open { */\nimport express from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("brace_in_literal_or_comment", "brace_regex.ts", "const open = /{/;\nimport express from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("brace_in_literal_or_comment", "brace_template.ts", "const t = `{`;\nimport express from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            // ---- extra_declarator
            ("extra_declarator", "extra_declarator.ts", format!("{EXPRESS_HEAD}const n = 1, app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("extra_declarator", "extra_declarator_dup.ts", format!("{EXPRESS_HEAD}const app = express(), n = 1;\nlet n2 = 2, app2 = 3, app = 4;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            // ---- rest_or_destructured_param
            ("rest_or_destructured_param", "rest_param.ts", format!("{EXPRESS_HEAD}const app = express();\nfunction inner(...app) {{\n  app.get(\"/x\", (req, res) => res.send(\"x\"));\n}}\n")),
            ("rest_or_destructured_param", "object_param.ts", format!("{EXPRESS_HEAD}const app = express();\nfunction inner({{ app }}) {{\n  app.get(\"/x\", (req, res) => res.send(\"x\"));\n}}\n")),
            ("rest_or_destructured_param", "array_param.ts", format!("{EXPRESS_HEAD}const app = express();\nconst inner = ([app]) => {{\n  app.get(\"/x\", (req, res) => res.send(\"x\"));\n}};\n")),
            ("rest_or_destructured_param", "default_param.ts", format!("{EXPRESS_HEAD}const app = express();\nfunction inner(a, {{ b: app = 1 }} = {{}}) {{\n  app.get(\"/x\", (req, res) => res.send(\"x\"));\n}}\n")),
            // ---- declaration_not_at_line_start
            ("declaration_not_at_line_start", "not_line_start.ts", format!("{EXPRESS_HEAD}start(); const app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("declaration_not_at_line_start", "import_not_line_start.ts", "start(); import express from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            // ---- keyword_lookalike
            ("keyword_lookalike", "template_lookalike.ts", format!("{EXPRESS_HEAD}const app = express();\nconst doc = `\nconst app = express();\nimport fastify from \"fastify\";\n`;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("keyword_lookalike", "require_member.js", "const express = require(\"express\").default;\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("keyword_lookalike", "type_import.ts", "import type { Router } from \"express\";\nimport express from \"express\";\nconst app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
            ("keyword_lookalike", "async_function.ts", format!("{EXPRESS_HEAD}const app = express();\nexport async function app() {{}}\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            // ---- syntax_error placements
            ("syntax_error", "error_top.ts", format!("{EXPRESS_HEAD}const app = express();\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n}}\n")),
            ("syntax_error", "error_unclosed.ts", format!("{EXPRESS_HEAD}const app = express();\napp.get(\"/a\", (req, res) => {{\n  res.send(\"a\");\n")),
            ("syntax_error", "error_missing.ts", format!("{EXPRESS_HEAD}const app = express();\nconst broken = ;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n")),
            ("syntax_error", "error_inside_function.ts", "import { PrismaClient } from \"@prisma/client\";\nconst prisma = new PrismaClient();\nexport async function list() {\n  const x = (;\n  return prisma.user.findMany();\n}\n".to_string()),
            ("syntax_error", "error_test.test.ts", "import { describe, it } from \"vitest\";\ndescribe(\"s\", () => {\n  it(\"t\", () => {\n    const = 1;\n  });\n});\n".to_string()),
            ("syntax_error", "error_jsx_in_ts.ts", "import express from \"express\";\nconst app = express();\nconst el = <div>;\napp.get(\"/a\", (req, res) => res.send(\"a\"));\n".to_string()),
        ]
    }

    fn decision_classes(report: &Report) -> BTreeSet<&'static str> {
        report
            .differences
            .iter()
            .map(|(class, _, _)| *class)
            .filter(|class| {
                [
                    "unknown_added",
                    "unknown_removed",
                    "anchor_changed",
                    "unknown_kind_changed",
                ]
                .contains(class)
            })
            .collect()
    }

    /// Every branch case differs exactly as attributed: the legacy reader
    /// misread the source (the listed class) and the tree reads it correctly,
    /// or the source is a syntax error (ADR-0059 D4).
    #[test]
    fn branch_corpus_decisions_match_their_attribution() {
        let removed = [
            "multi_import.ts",
            "multi_require.js",
            "multi_decl.ts",
            "brace_string_open.ts",
            "brace_string_close.ts",
            "brace_comment.ts",
            "brace_block_comment.ts",
            "brace_regex.ts",
            "brace_template.ts",
            "extra_declarator.ts",
            "not_line_start.ts",
            "import_not_line_start.ts",
            "template_lookalike.ts",
        ];
        let added = [
            "rest_param.ts",
            "object_param.ts",
            "array_param.ts",
            "default_param.ts",
            "require_member.js",
            // an `async function` the legacy reader did not see as a declaration
            "async_function.ts",
        ];
        for (label, path, text) in branch_corpus() {
            let mut report = Report::default();
            compare(path, &text, &mut report);
            let expected: BTreeSet<&str> = if removed.contains(&path) {
                ["unknown_removed"].into()
            } else if added.contains(&path) || label == "syntax_error" {
                ["unknown_added"].into()
            } else {
                BTreeSet::new()
            };
            assert_eq!(decision_classes(&report), expected, "{label} {path}");
            assert_eq!(
                report.degraded_files,
                usize::from(label == "syntax_error"),
                "{path}"
            );
        }
    }

    /// D4: a degraded file reads no binding and every considered unit gets
    /// exactly one `tsjs_parse_degraded` UNKNOWN.
    #[test]
    fn syntax_errors_refuse_every_considered_unit_once() {
        let mut checked = 0;
        for (label, path, text) in branch_corpus() {
            if label != "syntax_error" {
                continue;
            }
            let document = document(path, &text);
            let graph = ScopeGraphLite::analyze(&text, path, &document.language);
            assert!(graph.degraded && graph.imports.is_empty(), "{path}");
            let units = units_of(&document, None);
            let facts = super::super::super::anchors_with_bindings(&document, &units, None, graph)
                .expect("facts");
            let by_unit = decisions(&facts);
            assert!(!by_unit.is_empty(), "{path}");
            for (unit, decision) in by_unit {
                assert_eq!(decision.anchor, None, "{path} {unit}");
                assert_eq!(
                    decision.unknowns,
                    BTreeSet::from(["tsjs_parse_degraded".to_string()]),
                    "{path} {unit}"
                );
                assert_eq!(
                    facts.iter().filter(|fact| fact.subject == unit).count(),
                    1,
                    "{path} {unit}"
                );
            }
            checked += 1;
        }
        assert!(checked >= 5);
    }

    /// Input is untrusted: deep nesting with a declaration per level is
    /// refused inside the scope-work bound instead of running quadratically.
    #[test]
    fn deep_nesting_stays_bounded() {
        let depth = 3_000;
        let text = format!("{}{}", "{ let a = 1;\n".repeat(depth), "}".repeat(depth));
        let started = Instant::now();
        let graph = ScopeGraphLite::analyze(&text, "deep.ts", &Language::TypeScript);
        assert!(graph.degraded);
        assert!(started.elapsed() < Duration::from_secs(30));
        let started = Instant::now();
        let flat = "{ ".repeat(50_000) + &"}".repeat(50_000);
        let graph = ScopeGraphLite::analyze(&flat, "flat.ts", &Language::TypeScript);
        assert!(!graph.degraded && graph.scope_intervals.len() == 50_000);
        assert!(started.elapsed() < Duration::from_secs(30));
    }

    /// The tree reads the clause forms the line reader could not.
    #[test]
    fn tree_graph_reads_every_clause_form() {
        let text = "import d, { a as b, type T } from \"m\";\nimport * as ns from \"n\";\nconst { x, y: z, ...rest } = require(\"r\");\nconst q = require(\"q\"), w = 1;\nlet [p, [o]] = pair;\nexport default async function* gen() {}\nconst f = ({ a1 }, [a2], ...a3) => {\n  v = 1;\n};\nuse = 2;\n";
        let graph = ScopeGraphLite::analyze(text, "x.ts", &Language::TypeScript);
        let imports: BTreeSet<String> = graph
            .imports
            .iter()
            .map(|(name, binding)| format!("{name}={}", import_signature(binding)))
            .collect();
        assert_eq!(
            imports,
            BTreeSet::from(
                [
                    "d=m:Default",
                    "b=m:Named(\"a\")",
                    "T=m:Named(\"T\")",
                    "ns=n:Namespace",
                    "x=r:Named(\"x\")",
                    "z=r:Named(\"y\")",
                    "q=q:Default",
                ]
                .map(String::from)
            )
        );
        let decls: Vec<&str> = graph.local_decls.iter().map(String::as_str).collect();
        assert_eq!(decls, ["f", "gen", "o", "p", "rest", "w"]);
        assert!(graph.name_is_unsafe_at("a3", text.find("v = 1").unwrap()));
        assert!(graph.name_is_unsafe_at("v", text.find("v = 1").unwrap()));
        assert!(graph.name_is_unsafe_at("use", text.len()));
        assert!(!graph.name_is_unsafe_at("use", 0));
    }

    /// Pre-existing defect of the line-based unit extractor (slice 3): a
    /// class whose preceding six bytes split a multi-byte character panics in
    /// `text_ends_with_keyword`. Recorded, not fixed, in this slice.
    #[test]
    #[should_panic(expected = "not a char boundary")]
    fn extractor_panics_on_split_multibyte_before_class() {
        let text = "败败a class A {}\n";
        units_of(&document("a.ts", text), None);
    }

    #[test]
    fn committed_fixtures_differ_only_where_attributed() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures");
        let mut report = Report::default();
        compare_roots(&[root], &mut report);
        assert!(report.sources >= 82);
        assert_eq!(report.decision_differences(), 0, "{}", report.lines());
        assert_eq!(report.degraded_files, 0);
        // Field differences are the attributed ones: `async function`
        // declarations the legacy reader did not read, and a function or class
        // name the legacy reader filed inside its own body.
        assert_eq!(
            report.counts(),
            BTreeMap::from([("decl_added", 38), ("shadow_changed", 16)]),
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
            "sources={} degraded_files={} skipped={skipped} extractor_panics={} considered_units={} counts={:?} legacy_analyze={:?} tree_analyze={:?}\n",
            report.sources,
            report.degraded_files,
            report.extractor_panics.len(),
            report.considered_units,
            report.counts(),
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
