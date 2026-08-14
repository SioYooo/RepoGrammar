//! Bounded R testthat frontend for the ADR-0042 admitted shape.
//!
//! Only `tests/testthat/test-*.R` bytes reach this scanner, because that is the
//! set testthat's own `test_dir()` runner reads. The path is identity evidence
//! rather than a style preference, and the repository must additionally declare
//! `testthat` in `DESCRIPTION` before any anchor forms.
//!
//! Nothing here invokes R, `parse`, `eval`, `source`, renv, package
//! restoration, or a repository script. ADR-0036's execution prohibitions all
//! name running R and are unreached: nothing runs.
//!
//! The scanner is string- and comment-aware by construction, including R 4.0
//! raw strings. This repository has shipped the opposite defect three times --
//! a Rust attribute matched inside a function body, a TS/JS runner matched
//! inside a member call, a Go declaration matched inside a string -- each from
//! asking whether text appears rather than whether a construct exists.

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};

pub const R_ANCHOR_ENGINE: &str = "repogrammar-r-testthat-scanner";
pub const R_ANCHOR_METHOD: &str = "bounded_r_test_that_v1";

/// Fixed support target for the one admitted exact anchor.
pub const R_TEST_THAT_TARGET: &str = "testthat.test_that";

const CALLEE: &str = "test_that";
const MAX_BLOCK_UNITS: usize = 4_096;

/// True for the only paths this frontend may read.
///
/// `test_dir()` scans `tests/testthat/` and runs files whose names begin with
/// `test`; this admits the `test-` spelling the package convention uses.
pub fn is_testthat_path(path: &str) -> bool {
    let mut components = path.split('/').collect::<Vec<_>>();
    let Some(name) = components.pop() else {
        return false;
    };
    if !name.starts_with("test-") || !(name.ends_with(".R") || name.ends_with(".r")) {
        return false;
    }
    components
        .windows(2)
        .any(|pair| pair == ["tests", "testthat"])
}

#[derive(Debug, Default)]
pub struct RTestThatParser;

impl SourceParser for RTestThatParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.parse_with_context(document, &ParserProjectContext::default())
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        parse_output(document, context).map(|output| output.report)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document, context)
    }
}

pub(crate) fn parse_output(
    document: SourceDocument<'_>,
    context: &ParserProjectContext,
) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::R || !is_testthat_path(document.path) {
        return Err(ParseError::UnsupportedLanguage);
    }
    let full_range = SourceRange::new(0, document.text.len()).map_err(ParseError::Internal)?;
    let provenance = Provenance::new(
        document.path,
        document.content_hash.clone(),
        document.repository_revision.clone(),
    )
    .map_err(ParseError::Internal)?;
    let module = CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#r_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::R,
        kind: CodeUnitKind::Module,
        range: full_range.clone(),
        provenance: provenance.clone(),
    };
    let mut units = vec![module.clone()];
    let mut facts = Vec::new();

    if document.text.len() > usize::try_from(DEFAULT_MAX_FILE_BYTES).unwrap_or(usize::MAX) {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "r_test_scan",
            "source_byte_limit",
            full_range,
            "R source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts);
    }

    if !context.r_declares_testthat {
        // A directory named tests/testthat in a project that does not depend on
        // testthat establishes nothing.
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::MissingDependency,
            "r_testthat_identity",
            "testthat_not_declared",
            full_range,
            "no DESCRIPTION in this repository declares testthat, so the framework identity is unproven",
        )?);
        return finish(units, facts);
    }

    let code = CodeMask::new(document.text);
    let mut limit_hit = false;
    for (ordinal, block) in test_that_blocks(document.text, &code)
        .into_iter()
        .enumerate()
    {
        if ordinal >= MAX_BLOCK_UNITS {
            limit_hit = true;
            break;
        }
        let range = SourceRange::new(block.start, block.end).map_err(ParseError::Internal)?;
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#r_test_that_block:{}-{}:{}",
                document.path, block.start, block.end, ordinal
            ))
            .map_err(ParseError::Internal)?,
            language: Language::R,
            kind: CodeUnitKind::RTestThatBlock,
            range: range.clone(),
            provenance: provenance.clone(),
        };
        facts.push(anchor_fact(&unit)?);
        units.push(unit);
    }

    if limit_hit {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "r_test_scan",
            "scanner_resource_limit",
            module.range.clone(),
            "R scanner exceeded the bounded block limit",
        )?);
    }

    finish(units, facts)
}

fn finish(
    mut units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
) -> Result<SourceParseOutput, ParseError> {
    units.sort_by(|left, right| {
        (left.range.start_byte, left.range.end_byte, left.id.as_str()).cmp(&(
            right.range.start_byte,
            right.range.end_byte,
            right.id.as_str(),
        ))
    });
    facts.sort_by(|left, right| {
        (
            left.evidence.range.start_byte,
            left.evidence.range.end_byte,
            left.kind.as_protocol_str(),
            left.target.as_ref().map(SymbolId::as_str),
        )
            .cmp(&(
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
                right.kind.as_protocol_str(),
                right.target.as_ref().map(SymbolId::as_str),
            ))
    });
    let ir_nodes = ir_nodes_for_units(&units).map_err(ParseError::Internal)?;
    let ir_edges = ir_edges_for_units(&units).map_err(ParseError::Internal)?;
    Ok(SourceParseOutput {
        report: ParseReport {
            units,
            ir_nodes,
            ir_edges,
            semantic_facts: facts,
            diagnostics: Vec::new(),
        },
        python_interface_hash: None,
        dependencies: Vec::new(),
    })
}

/// Per-byte record of which source is code, plus the nesting immediately before
/// each byte, so every later question is asked about source rather than about
/// text that merely looks like it.
struct CodeMask {
    is_code: Vec<bool>,
    paren_depth: Vec<u32>,
    brace_depth: Vec<u32>,
}

impl CodeMask {
    fn new(text: &str) -> Self {
        let bytes = text.as_bytes();
        let mut is_code = vec![false; bytes.len()];
        let mut paren_depth = vec![0u32; bytes.len()];
        let mut brace_depth = vec![0u32; bytes.len()];
        let mut index = 0usize;
        let mut parens = 0u32;
        let mut braces = 0u32;
        while index < bytes.len() {
            paren_depth[index] = parens;
            brace_depth[index] = braces;
            if let Some(end) = raw_string_end(bytes, index) {
                index = end;
                continue;
            }
            match bytes[index] {
                b'#' => index = line_end(bytes, index),
                b'"' | b'\'' | b'`' => index = quoted_end(bytes, index, bytes[index]),
                byte => {
                    match byte {
                        b'(' => parens = parens.saturating_add(1),
                        b')' => parens = parens.saturating_sub(1),
                        b'{' => braces = braces.saturating_add(1),
                        b'}' => braces = braces.saturating_sub(1),
                        _ => {}
                    }
                    is_code[index] = true;
                    index += 1;
                }
            }
        }
        Self {
            is_code,
            paren_depth,
            brace_depth,
        }
    }

    fn is_code_at(&self, index: usize) -> bool {
        self.is_code.get(index).copied().unwrap_or(false)
    }

    fn is_top_level(&self, index: usize) -> bool {
        self.paren_depth.get(index).copied().unwrap_or(u32::MAX) == 0
            && self.brace_depth.get(index).copied().unwrap_or(u32::MAX) == 0
    }
}

fn line_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn quoted_end(bytes: &[u8], start: usize, delimiter: u8) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            byte if byte == delimiter => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

/// End of an R 4.0 raw string starting at `start`, if one starts there.
///
/// The forms are `r"(...)"`, `r"[...]"`, `r"{...}"`, any number of dashes
/// between the quote and the opener, and an uppercase `R` prefix.
fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    if !matches!(bytes.get(start), Some(b'r') | Some(b'R')) {
        return None;
    }
    if start > 0 && is_identifier_byte(bytes[start - 1]) {
        return None;
    }
    let quote = *bytes.get(start + 1)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let mut index = start + 2;
    let dash_start = index;
    while bytes.get(index) == Some(&b'-') {
        index += 1;
    }
    let dashes = index - dash_start;
    let close = match bytes.get(index)? {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        _ => return None,
    };
    index += 1;
    let mut terminator = vec![close];
    terminator.extend(std::iter::repeat_n(b'-', dashes));
    terminator.push(quote);
    while index < bytes.len() {
        if bytes[index..].starts_with(&terminator) {
            return Some(index + terminator.len());
        }
        index += 1;
    }
    Some(bytes.len())
}

/// One admitted `test_that("...", { ... })` call.
struct TestThatBlock {
    start: usize,
    end: usize,
}

fn test_that_blocks(text: &str, code: &CodeMask) -> Vec<TestThatBlock> {
    let bytes = text.as_bytes();
    let mut blocks = Vec::new();
    for (index, _) in text.match_indices(CALLEE) {
        if !code.is_code_at(index) || !code.is_top_level(index) {
            continue;
        }
        // A bare identifier callee only: not `pkg::test_that`, not `x$test_that`.
        if index > 0 {
            let previous = bytes[index - 1];
            if is_identifier_byte(previous) || matches!(previous, b':' | b'$' | b'@') {
                continue;
            }
        }
        let after_name = index + CALLEE.len();
        if bytes
            .get(after_name)
            .is_some_and(|byte| is_identifier_byte(*byte))
        {
            continue;
        }
        let open = skip_spaces(bytes, after_name);
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(end) = admitted_call_end(bytes, code, open) else {
            continue;
        };
        blocks.push(TestThatBlock { start: index, end });
    }
    blocks
}

/// End of the call when its arguments are exactly a string literal and a brace
/// block, or `None` when any condition fails.
fn admitted_call_end(bytes: &[u8], code: &CodeMask, open: usize) -> Option<usize> {
    let description = skip_spaces(bytes, open + 1);
    let quote = *bytes.get(description)?;
    if quote != b'"' && quote != b'\'' {
        return None;
    }
    let description_end = quoted_end(bytes, description, quote);
    if description_end <= description + 2 {
        // An empty description distinguishes nothing.
        return None;
    }
    let comma = skip_spaces(bytes, description_end);
    if bytes.get(comma) != Some(&b',') {
        return None;
    }
    let block = skip_spaces(bytes, comma + 1);
    if bytes.get(block) != Some(&b'{') {
        return None;
    }
    let block_end = matching(bytes, code, block, b'{', b'}')?;
    let close = skip_spaces(bytes, block_end);
    if bytes.get(close) != Some(&b')') {
        return None;
    }
    Some(close + 1)
}

fn matching(bytes: &[u8], code: &CodeMask, open: usize, opener: u8, closer: u8) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        if code.is_code_at(index) {
            if bytes[index] == opener {
                depth += 1;
            } else if bytes[index] == closer {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
        }
        index += 1;
    }
    None
}

fn skip_spaces(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn is_identifier_byte(byte: u8) -> bool {
    byte == b'_' || byte == b'.' || byte.is_ascii_alphanumeric() || !byte.is_ascii()
}

fn anchor_fact(unit: &CodeUnit) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(R_TEST_THAT_TARGET).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded R testthat test_that block anchor",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            "r_anchor_kind=test_that_block".to_string(),
        ],
    })
}

fn unknown_fact(
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    affected_claim: &str,
    kind: &str,
    range: SourceRange,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(unit.id.clone(), range, unit.provenance.clone(), note)
            .map_err(ParseError::Internal)?,
        assumptions: vec![
            format!("affected_claim={affected_claim}"),
            format!("r_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: R_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: R_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn declared() -> ParserProjectContext {
        ParserProjectContext {
            r_declares_testthat: true,
            ..ParserProjectContext::default()
        }
    }

    fn output_with(context: &ParserProjectContext, text: &str) -> SourceParseOutput {
        RTestThatParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/testthat/test-catalog.R",
                    language: Language::R,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                context,
            )
            .expect("parse R testthat source")
    }

    fn output(text: &str) -> SourceParseOutput {
        output_with(&declared(), text)
    }

    fn anchors(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(R_TEST_THAT_TARGET))
            .count()
    }

    fn unknown_kinds(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| assumption.strip_prefix("r_unknown_kind=").map(String::from))
            .collect()
    }

    #[test]
    fn admitted_top_level_blocks_anchor() {
        let parsed = output(
            "test_that(\"loads the catalog\", {\n  expect_true(TRUE)\n})\n\n\
             test_that('filters the catalog', {\n  expect_equal(1, 1)\n})\n",
        );
        assert_eq!(anchors(&parsed), 2);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::RTestThatBlock)
                .count(),
            2
        );
    }

    #[test]
    fn without_a_declared_testthat_dependency_nothing_anchors() {
        let parsed = output_with(
            &ParserProjectContext::default(),
            "test_that(\"loads\", {\n  expect_true(TRUE)\n})\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"testthat_not_declared".to_string()));
    }

    #[test]
    fn call_text_in_comments_strings_and_raw_strings_never_anchors() {
        let parsed = output(
            "# test_that(\"commented\", { })\n\
             x <- \"test_that('quoted', { })\"\n\
             y <- r\"(test_that('raw', { }))\"\n\
             z <- r\"---[test_that('dashed raw', { })]---\"\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn nested_and_namespaced_calls_are_not_admitted() {
        for source in [
            // nested inside another call
            "wrapper(test_that(\"nested\", { }))",
            // nested inside a block
            "local({\n  test_that(\"nested\", { })\n})",
            // namespaced callee
            "testthat::test_that(\"namespaced\", { })",
            // member callee
            "obj$test_that(\"member\", { })",
            // a longer identifier that merely contains the name
            "my_test_that(\"lookalike\", { })",
        ] {
            let parsed = output(&format!("{source}\n"));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn only_a_literal_description_and_a_brace_block_are_admitted() {
        for source in [
            // computed description
            "test_that(paste0(\"a\", \"b\"), { })",
            // variable description
            "test_that(description, { })",
            // empty description
            "test_that(\"\", { })",
            // second argument is not a block
            "test_that(\"desc\", expect_true(TRUE))",
            // a third argument
            "test_that(\"desc\", { }, extra)",
            // no arguments
            "test_that()",
        ] {
            let parsed = output(&format!("{source}\n"));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn braces_inside_strings_do_not_close_the_block() {
        let parsed = output(
            "test_that(\"loads\", {\n  x <- \"}\"\n  expect_true(TRUE)\n})\n\n\
             test_that(\"filters\", {\n  expect_true(TRUE)\n})\n",
        );
        assert_eq!(
            anchors(&parsed),
            2,
            "a brace in a string must not end a block"
        );
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output("test_that(\"loads\", { expect_true(TRUE) })\n");
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn only_the_testthat_runner_paths_are_admitted() {
        let parser = RTestThatParser;
        for path in [
            "R/catalog.R",
            "tests/test-catalog.R",
            "tests/testthat/helper-catalog.R",
            "tests/testthat/catalog.R",
            "testthat/test-catalog.R",
        ] {
            assert_eq!(
                parser.parse(SourceDocument {
                    path,
                    language: Language::R,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text: "test_that(\"x\", { })\n",
                }),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
        assert!(is_testthat_path("tests/testthat/test-catalog.R"));
        assert!(is_testthat_path("pkg/tests/testthat/test-catalog.r"));
    }

    #[test]
    fn ir_contains_edges_link_blocks_to_their_module() {
        let parsed = output("test_that(\"a\", { })\ntest_that(\"b\", { })\n");
        assert_eq!(parsed.report.ir_nodes.len(), 3);
        assert_eq!(parsed.report.ir_edges.len(), 2);
    }
}
