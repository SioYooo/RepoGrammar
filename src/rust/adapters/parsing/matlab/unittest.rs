//! Bounded MATLAB `matlab.unittest` frontend for the ADR-0046 admitted shape.
//!
//! Nothing here invokes MATLAB or Octave, opens a project, or evaluates code.
//! ADR-0037's prohibitions are carried forward.
//!
//! MATLAB's tick is overloaded exactly the way Ada's is, and it is decidable the
//! same way -- but the rule here is whitespace-sensitive rather than
//! token-sensitive: a tick immediately following an identifier character, `)`,
//! `]`, `}`, or another tick, with no intervening blank, is a transpose;
//! otherwise it opens a character array. That is what makes `[a' b']` two
//! transposes while `[a 'b']` is a concatenation with a character array.
//!
//! MATLAB's genuinely undecidable construct is command syntax -- `a -1` is a
//! subtraction or the call `a('-1')` depending on runtime binding -- and it does
//! not reach here: command arguments are unquoted, so they never change what is
//! a comment or a string, and this anchor is a declaration shape.

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};

pub const MATLAB_ANCHOR_ENGINE: &str = "repogrammar-matlab-unittest-scanner";
pub const MATLAB_ANCHOR_METHOD: &str = "bounded_matlab_unittest_class_v1";

/// Fixed support target for the one admitted exact anchor.
pub const MATLAB_TEST_TARGET: &str = "matlab_unittest.TestMethod";

const TEST_CASE_BASE: &str = "matlab.unittest.TestCase";
const MAX_UNITS: usize = 4_096;

/// Every block keyword MATLAB closes with `end`.
const BLOCK_OPENERS: &[&str] = &[
    "if",
    "for",
    "while",
    "switch",
    "try",
    "parfor",
    "spmd",
    "arguments",
    "function",
    "methods",
    "properties",
    "events",
    "enumeration",
    "classdef",
];

/// True for the only suffix this frontend may read.
pub fn is_matlab_source_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
        .ends_with(".m")
}

#[derive(Debug, Default)]
pub struct MatlabUnitTestParser;

impl SourceParser for MatlabUnitTestParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        parse_output(document).map(|output| output.report)
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        self.parse(document)
    }

    fn parse_with_context_output(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<SourceParseOutput, ParseError> {
        parse_output(document)
    }
}

pub(crate) fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Matlab || !is_matlab_source_path(document.path) {
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
            "unit:{}#matlab_file:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Matlab,
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
            "matlab_declaration_scan",
            "source_byte_limit",
            full_range,
            "MATLAB source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts);
    }

    let lines = source_lines(document.text);
    let scan = admitted_anchors(&lines);

    for anchor in scan.anchors {
        if units.len() >= MAX_UNITS {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "matlab_declaration_scan",
                "scanner_resource_limit",
                module.range.clone(),
                "MATLAB scanner exceeded the bounded unit limit",
            )?);
            break;
        }
        let (kind, target, assumption, note) = match anchor.kind {
            AnchorKind::TestClass => (
                CodeUnitKind::MatlabTestClass,
                "matlab_unittest.TestCase",
                "matlab_anchor_kind=unittest_test_class",
                "bounded MATLAB matlab.unittest.TestCase class anchor",
            ),
            AnchorKind::TestMethod => (
                CodeUnitKind::MatlabTestMethod,
                MATLAB_TEST_TARGET,
                "matlab_anchor_kind=unittest_test_method",
                "bounded MATLAB matlab.unittest Test methods-block anchor",
            ),
        };
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#{}:{}-{}",
                document.path,
                kind.as_str(),
                anchor.start,
                anchor.end
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Matlab,
            kind,
            range: SourceRange::new(anchor.start, anchor.end).map_err(ParseError::Internal)?,
            provenance: provenance.clone(),
        };
        facts.push(anchor_fact(&unit, target, assumption, note)?);
        units.push(unit);
    }

    if scan.unbound_test_block {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::UnresolvedImport,
            "matlab_unittest_class_binding",
            "test_methods_block_without_testcase_base",
            module.range.clone(),
            "a Test methods block appears in a class that does not derive from matlab.unittest.TestCase, so the framework is unproven",
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

struct SourceLine {
    start: usize,
    end: usize,
    code: String,
}

fn source_lines(text: &str) -> Vec<SourceLine> {
    let mut lines = Vec::new();
    let mut start = 0usize;
    let mut in_block_comment = false;
    for raw in text.split_inclusive('\n') {
        let end = start + raw.len();
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let trimmed = body.trim();
        // A `%{` / `%}` block delimiter must stand alone on its line.
        if in_block_comment {
            if trimmed == "%}" {
                in_block_comment = false;
            }
            lines.push(SourceLine {
                start,
                end,
                code: String::new(),
            });
            start = end;
            continue;
        }
        if trimmed == "%{" {
            in_block_comment = true;
            lines.push(SourceLine {
                start,
                end,
                code: String::new(),
            });
            start = end;
            continue;
        }
        lines.push(SourceLine {
            start,
            end,
            code: strip_line(body),
        });
        start = end;
    }
    lines
}

/// Drop `%` comments, character arrays, and strings, keeping byte positions
/// stable by replacing each removed byte's contribution with a blank.
fn strip_line(line: &str) -> String {
    let bytes = line.as_bytes();
    let mut code = String::with_capacity(line.len());
    let mut index = 0usize;
    // The byte immediately before the tick decides what it means, with no
    // whitespace skipping: `a'` is a transpose, `a '` opens a character array.
    let mut previous_byte: Option<u8> = None;
    while index < bytes.len() {
        match bytes[index] {
            b'%' => break,
            // `...` continues the statement on the next line, and everything
            // after it to end of line is ignored the way a comment is.
            b'.' if bytes.get(index + 1) == Some(&b'.') && bytes.get(index + 2) == Some(&b'.') => {
                break
            }
            b'.' if bytes.get(index + 1) == Some(&b'\'')
                && tick_is_transpose(previous_byte)
                && index > 0 =>
            {
                // `.'` is the non-conjugate transpose, not a character array.
                code.push_str(".'");
                previous_byte = Some(b'\'');
                index += 2;
            }
            b'\'' if tick_is_transpose(previous_byte) => {
                code.push('\'');
                previous_byte = Some(b'\'');
                index += 1;
            }
            b'\'' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\'' {
                        if bytes.get(index + 1) == Some(&b'\'') {
                            index += 2;
                            continue;
                        }
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                code.push(' ');
                previous_byte = Some(b' ');
            }
            b'"' => {
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'"' {
                        if bytes.get(index + 1) == Some(&b'"') {
                            index += 2;
                            continue;
                        }
                        index += 1;
                        break;
                    }
                    index += 1;
                }
                code.push(' ');
                previous_byte = Some(b' ');
            }
            byte => {
                code.push(byte as char);
                previous_byte = Some(byte);
                index += 1;
            }
        }
    }
    code
}

fn tick_is_transpose(previous: Option<u8>) -> bool {
    previous.is_some_and(|byte| {
        is_identifier_byte(byte) || byte == b')' || byte == b']' || byte == b'}' || byte == b'\''
    })
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnchorKind {
    TestClass,
    TestMethod,
}

struct Anchor {
    kind: AnchorKind,
    start: usize,
    end: usize,
}

#[derive(Default)]
struct Scan {
    anchors: Vec<Anchor>,
    unbound_test_block: bool,
}

/// Depth is exact for MATLAB because every block keyword is closed by `end`,
/// and an `end` used as an index sits inside brackets, so only bracket depth
/// zero counts.
fn admitted_anchors(lines: &[SourceLine]) -> Scan {
    let mut scan = Scan::default();
    let mut depth = 0usize;
    let mut derives_test_case = false;
    let mut in_class = false;
    let mut test_block_depth: Option<usize> = None;

    for line in lines {
        let code = line.code.trim();
        if code.is_empty() {
            continue;
        }
        let tokens = statement_tokens(code);
        let opens_class = tokens.first().is_some_and(|token| *token == "classdef");
        let opens_test_methods = tokens.first().is_some_and(|token| *token == "methods")
            && attribute_list_has_test(code);
        let opens_function = tokens.first().is_some_and(|token| *token == "function");

        if opens_class {
            in_class = true;
            derives_test_case = superclass_list_has_test_case(code);
            if derives_test_case {
                scan.anchors.push(Anchor {
                    kind: AnchorKind::TestClass,
                    start: line.start,
                    end: line.end,
                });
            }
        } else if opens_test_methods {
            if in_class && derives_test_case {
                test_block_depth = Some(depth);
            } else if in_class {
                scan.unbound_test_block = true;
            }
        } else if opens_function
            && test_block_depth.is_some_and(|block| depth == block.saturating_add(1))
        {
            scan.anchors.push(Anchor {
                kind: AnchorKind::TestMethod,
                start: line.start,
                end: line.end,
            });
        }

        let (opened, closed) = block_delta(code);
        depth = depth.saturating_add(opened);
        for _ in 0..closed {
            depth = depth.saturating_sub(1);
            if test_block_depth == Some(depth) {
                test_block_depth = None;
            }
            if in_class && depth == 0 {
                in_class = false;
                derives_test_case = false;
            }
        }
    }
    scan
}

/// Count block openers and `end` terminators on one line, at bracket depth zero.
fn block_delta(code: &str) -> (usize, usize) {
    let mut opened = 0usize;
    let mut closed = 0usize;
    let mut statement_start = true;
    for (token, bracket_depth, at_statement_start) in tokens_with_context(code) {
        if bracket_depth > 0 {
            statement_start = false;
            continue;
        }
        if token == "end" {
            closed += 1;
        } else if at_statement_start && BLOCK_OPENERS.contains(&token.as_str()) {
            opened += 1;
        }
        statement_start = false;
    }
    let _ = statement_start;
    (opened, closed)
}

/// Tokens of one code line, each with its bracket depth and whether it begins a
/// statement (line start, or after `;` or `,` at bracket depth zero).
fn tokens_with_context(code: &str) -> Vec<(String, usize, bool)> {
    let bytes = code.as_bytes();
    let mut tokens = Vec::new();
    let mut depth = 0usize;
    let mut at_statement_start = true;
    let mut index = 0usize;
    while index < bytes.len() {
        let byte = bytes[index];
        if is_identifier_byte(byte) && !byte.is_ascii_digit() {
            let start = index;
            while index < bytes.len() && is_identifier_byte(bytes[index]) {
                index += 1;
            }
            tokens.push((code[start..index].to_string(), depth, at_statement_start));
            at_statement_start = false;
            continue;
        }
        match byte {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b';' | b',' if depth == 0 => at_statement_start = true,
            byte if byte.is_ascii_whitespace() => {}
            _ => at_statement_start = false,
        }
        index += 1;
    }
    tokens
}

fn statement_tokens(code: &str) -> Vec<String> {
    tokens_with_context(code)
        .into_iter()
        .map(|(token, _, _)| token)
        .collect()
}

/// `classdef X < matlab.unittest.TestCase`, including multiple-inheritance
/// lists written with `&`.
fn superclass_list_has_test_case(code: &str) -> bool {
    let Some((_, bases)) = code.split_once('<') else {
        return false;
    };
    bases
        .split('&')
        .any(|base| base.trim().trim_end_matches(&[';', ',', ' '][..]) == TEST_CASE_BASE)
}

/// `methods (Test)`, `methods(Test)`, `methods (Test, TestTags = {'unit'})`.
fn attribute_list_has_test(code: &str) -> bool {
    let Some((_, rest)) = code.split_once('(') else {
        return false;
    };
    let attributes = rest.split_once(')').map_or(rest, |(inside, _)| inside);
    attributes.split(',').any(|attribute| {
        attribute
            .split_once('=')
            .map_or(attribute, |(name, _)| name)
            .trim()
            == "Test"
    })
}

fn anchor_fact(
    unit: &CodeUnit,
    target: &str,
    assumption: &str,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            assumption.to_string(),
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
            format!("matlab_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: MATLAB_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: MATLAB_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        MatlabUnitTestParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/CatalogTest.m",
                    language: Language::Matlab,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse MATLAB source")
    }

    fn tests_found(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(MATLAB_TEST_TARGET))
            .count()
    }

    fn unknown_kinds(parsed: &SourceParseOutput) -> Vec<String> {
        parsed
            .report
            .semantic_facts
            .iter()
            .flat_map(|fact| fact.assumptions.iter())
            .filter_map(|assumption| {
                assumption
                    .strip_prefix("matlab_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    #[test]
    fn admitted_methods_anchor_and_helpers_after_the_classdef_do_not() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           testCase.verifyTrue(true);\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n\
             \x20           if true\n                   x = 1;\n               end\n\
             \x20       end\n\
             \x20   end\n\
             \x20   methods (Access = private)\n\
             \x20       function helper(testCase)\n            end\n\
             \x20   end\n\
             end\n\
             function localHelper()\n\
             end\n",
        );
        assert_eq!(
            tests_found(&parsed),
            2,
            "the private method and the file-local function are not tests"
        );
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::MatlabTestClass)
                .count(),
            1
        );
    }

    #[test]
    fn a_test_block_in_a_class_that_is_not_a_testcase_anchors_nothing() {
        let parsed = output(
            "classdef CatalogHelper < handle\n    methods (Test)\n\
             \x20       function looksLikeATest(obj)\n        end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 0);
        assert!(unknown_kinds(&parsed)
            .contains(&"test_methods_block_without_testcase_base".to_string()));
    }

    #[test]
    fn a_transpose_is_not_a_character_array() {
        // `[a' b']` is two transposes; `[a 'b']` is a concatenation with a
        // character array. Only the blank tells them apart.
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           v = [a' b'];\n\
             \x20           w = [a 'end'];\n\
             \x20           u = a.' * b;\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(
            tests_found(&parsed),
            2,
            "a stray `end` inside a character array must not close the block"
        );
    }

    #[test]
    fn a_comment_or_a_string_never_declares_a_test() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           % function inAComment(testCase)\n\
             \x20           s = 'function inAString(testCase)';\n\
             \x20           d = \"function inADoubleQuoted(testCase)\";\n\
             \x20       end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
    }

    #[test]
    fn a_block_comment_hides_everything_inside_it() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n\
             %{\n        function hidden(testCase)\n        end\n%}\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
    }

    #[test]
    fn an_ellipsis_continuation_comment_does_not_close_a_block() {
        // Everything after `...` is ignored to end of line, so the bare `end`
        // in that trailing text is not a block terminator.
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           total = 1 + ... sum everything through to the end\n\
             \x20                   2;\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 2);
    }

    #[test]
    fn an_end_used_as_an_index_does_not_close_a_block() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           v = data(2:end);\n            w = c{end};\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 2);
    }

    #[test]
    fn a_one_line_block_stays_balanced() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n\
             \x20           if true, x = 1; end\n\
             \x20       end\n\
             \x20       function filtersCatalog(testCase)\n        end\n\
             \x20   end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 2);
    }

    #[test]
    fn only_the_test_attribute_opens_the_block() {
        for attributes in ["(Access = public)", "(Static)", ""] {
            let parsed = output(&format!(
                "classdef CatalogTest < matlab.unittest.TestCase\n    methods {attributes}\n\
                 \x20       function notATest(testCase)\n        end\n    end\nend\n"
            ));
            assert_eq!(tests_found(&parsed), 0, "{attributes}");
        }
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n\
             \x20   methods (Test, TestTags = {'unit'})\n\
             \x20       function loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 1);
    }

    #[test]
    fn matching_is_case_sensitive_unlike_the_scanner_languages_before_it() {
        let parsed = output(
            "CLASSDEF CatalogTest < MATLAB.UNITTEST.TESTCASE\n    METHODS (TEST)\n\
             \x20       FUNCTION loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert_eq!(tests_found(&parsed), 0);
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(
            "classdef CatalogTest < matlab.unittest.TestCase\n    methods (Test)\n\
             \x20       function loadsCatalog(testCase)\n        end\n    end\nend\n",
        );
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn only_matlab_sources_are_admitted() {
        let parser = MatlabUnitTestParser;
        assert_eq!(
            parser.parse(SourceDocument {
                path: "Contents.mlx",
                language: Language::Matlab,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                text: "",
            }),
            Err(ParseError::UnsupportedLanguage)
        );
        assert!(is_matlab_source_path("tests/CatalogTest.m"));
    }
}
