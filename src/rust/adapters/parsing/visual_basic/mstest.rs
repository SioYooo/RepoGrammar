//! Bounded VB.NET MSTest frontend for the ADR-0043 admitted shape.
//!
//! MSTest discovers tests by attribute wherever they compile, so unlike Go and
//! R there is no runner-defined file set to narrow on. Every `.vb` file is
//! decoded and the bound is what this frontend emits: a module unit, an
//! admitted `TestClass`, and its admitted `TestMethod`s. Ordinary declarations
//! produce no unit, so the absence of a unit is not evidence that a file has no
//! code.
//!
//! Nothing here invokes MSBuild, Roslyn, `vbc`, `dotnet`, NuGet, an analyzer, a
//! source generator, a package script, a child process, or the network.
//!
//! The scanner is comment- and string-aware. VB.NET makes that unusually clean:
//! `'` starts a comment and is never a string delimiter, and strings are
//! double-quoted only, escaping by doubling. The delimiter ambiguity that rules
//! Ruby out does not exist here.

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};

pub const VB_ANCHOR_ENGINE: &str = "repogrammar-vbnet-mstest-scanner";
pub const VB_ANCHOR_METHOD: &str = "bounded_vbnet_mstest_attribute_v1";

/// Fixed support target for the one admitted exact anchor.
pub const VB_TEST_METHOD_TARGET: &str = "mstest.TestMethod";

const MSTEST_NAMESPACE: &str = "microsoft.visualstudio.testtools.unittesting";
const MAX_UNITS: usize = 4_096;

#[derive(Debug, Default)]
pub struct VisualBasicMsTestParser;

impl SourceParser for VisualBasicMsTestParser {
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
    if document.language != Language::VisualBasic {
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
            "unit:{}#vb_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::VisualBasic,
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
            "vb_declaration_scan",
            "source_byte_limit",
            full_range,
            "VB.NET source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts, Vec::new());
    }

    let (lines, unterminated_string) = source_lines(document.text);
    // A scanner cannot fail on malformed VB the way a parser does, but an
    // unterminated string is decidable per line and hides that line's
    // declarations, so a missing anchor stops being informative.
    let mut diagnostics = Vec::new();
    if unterminated_string {
        diagnostics.push(degraded(
            document.path,
            "a VB.NET string literal is left open at end of line, so that line's declarations were read as string",
        ));
    }
    // An `Imports` selected by an unevaluated constant cannot make a bare
    // attribute name resolve, so only unconditional code binds the spelling.
    let imports_mstest = lines
        .iter()
        .any(|line| line.conditional_depth == 0 && line_imports_mstest(&line.code));

    let mut unresolved_attribute = false;
    let scan = test_class_groups(&lines, imports_mstest, &mut unresolved_attribute);
    if scan.skipped_conditional {
        // Both branches of a `#If` cannot compile, and RepoGrammar cannot
        // evaluate the constant that picks one, so neither branch may anchor.
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::BuildVariantAmbiguity,
            "vb_conditional_compilation",
            "declaration_under_conditional_compilation",
            module.range.clone(),
            "an admitted declaration sits inside conditional compilation, so the constant that selects it is unevaluated",
        )?);
    }
    for group in scan.groups {
        if units.len() >= MAX_UNITS {
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::InsufficientSupport,
                "vb_declaration_scan",
                "scanner_resource_limit",
                module.range.clone(),
                "VB.NET scanner exceeded the bounded unit limit",
            )?);
            break;
        }
        let class_unit = declaration_unit(
            &document,
            &provenance,
            CodeUnitKind::VbTestClass,
            group.class_start,
            group.class_end,
        )?;
        facts.push(anchor_fact(
            &class_unit,
            "mstest.TestClass",
            "vb_anchor_kind=mstest_test_class",
            "bounded VB.NET MSTest TestClass attribute anchor",
        )?);
        units.push(class_unit);
        for method in group.methods {
            if units.len() >= MAX_UNITS {
                break;
            }
            let unit = declaration_unit(
                &document,
                &provenance,
                CodeUnitKind::VbTestMethod,
                method.0,
                method.1,
            )?;
            facts.push(anchor_fact(
                &unit,
                VB_TEST_METHOD_TARGET,
                "vb_anchor_kind=mstest_test_method",
                "bounded VB.NET MSTest TestMethod attribute anchor",
            )?);
            units.push(unit);
        }
    }

    if unresolved_attribute {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::UnresolvedImport,
            "vb_mstest_attribute_binding",
            "mstest_attribute_without_import",
            module.range.clone(),
            "an MSTest attribute name appears without an exact import or fully qualified namespace",
        )?);
    }

    finish(units, facts, diagnostics)
}

fn declaration_unit(
    document: &SourceDocument<'_>,
    provenance: &Provenance,
    kind: CodeUnitKind,
    start: usize,
    end: usize,
) -> Result<CodeUnit, ParseError> {
    Ok(CodeUnit {
        id: CodeUnitId::new(format!(
            "unit:{}#{}:{}-{}",
            document.path,
            kind.as_str(),
            start,
            end
        ))
        .map_err(ParseError::Internal)?,
        language: Language::VisualBasic,
        kind,
        range: SourceRange::new(start, end).map_err(ParseError::Internal)?,
        provenance: provenance.clone(),
    })
}

fn degraded(path: &str, message: &str) -> ParseDiagnostic {
    ParseDiagnostic {
        path: path.to_string(),
        range: None,
        severity: ParseDiagnosticSeverity::Error,
        message: message.to_string(),
    }
}

fn finish(
    mut units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
    diagnostics: Vec<ParseDiagnostic>,
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
            diagnostics,
        },
        python_interface_hash: None,
        dependencies: Vec::new(),
    })
}

/// One source line with its comment- and string-stripped code text.
struct SourceLine {
    start: usize,
    end: usize,
    code: String,
    /// Conditional-compilation depth after this line's own directive. A
    /// declaration at depth greater than zero is compiled only under a
    /// constant this frontend does not evaluate.
    conditional_depth: usize,
}

/// The conditional-compilation directives that decide what may be claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Directive {
    /// `#If` opens a region selected by an unevaluated constant.
    ConditionalOpen,
    /// `#End If` closes that region. `#ElseIf` and `#Else` stay inside it.
    ConditionalClose,
    /// `#Region`, `#ExternalSource`, `#Const`, `#Enable`/`#Disable Warning`,
    /// and their `#End` forms select no branch and change no claim.
    Neutral,
}

/// Classify a stripped code line as a conditional-compilation directive.
///
/// The rule is deliberately asymmetric: liberal about what opens a region,
/// strict about what closes one. A missed open would anchor a declaration the
/// build may not contain, which is unsound; a missed close only leaves the
/// depth high, which understates support and cannot invent a member. `#If` is
/// therefore matched on its name alone rather than on a trailing `Then`, and
/// only `End` followed by `If` closes.
fn classify_directive(code: &str) -> Option<Directive> {
    let rest = code.trim_start().strip_prefix('#')?;
    let (first, rest) = directive_word(rest);
    if first.eq_ignore_ascii_case("if") {
        return Some(Directive::ConditionalOpen);
    }
    if first.eq_ignore_ascii_case("end") && directive_word(rest).0.eq_ignore_ascii_case("if") {
        return Some(Directive::ConditionalClose);
    }
    Some(Directive::Neutral)
}

/// The next maximal ASCII-alphabetic run, and the text after it.
///
/// Taking the alphabetic run rather than the whitespace-delimited word is what
/// lets `#ExternalSource("Catalog.vb", 1)` and `#Region "Tests"` classify by
/// name despite their trailing punctuation.
fn directive_word(text: &str) -> (&str, &str) {
    let trimmed = text.trim_start();
    let end = trimmed
        .find(|character: char| !character.is_ascii_alphabetic())
        .unwrap_or(trimmed.len());
    trimmed.split_at(end)
}

/// Split into lines and strip what is not code from each.
///
/// `'` and `REM` open comments and VB has no single-quoted string, so a comment
/// can never be mistaken for a delimiter. Double-quoted strings escape by
/// doubling the quote, which the scan handles by consuming the pair.
fn source_lines(text: &str) -> (Vec<SourceLine>, bool) {
    let mut lines = Vec::new();
    let mut start = 0usize;
    // VB strings do not span lines, so an unterminated quote is decidable per
    // line and makes that line's declarations invisible to the scan.
    let mut unterminated_string = false;
    let mut conditional_depth = 0usize;
    for raw in text.split_inclusive('\n') {
        let end = start + raw.len();
        let body = raw.strip_suffix('\n').unwrap_or(raw);
        let body = body.strip_suffix('\r').unwrap_or(body);
        let (code, closed) = strip_comment_and_strings(body);
        unterminated_string |= !closed;
        // A directive line is not a declaration, but which directive it is
        // decides what the lines around it may claim.
        match classify_directive(&code) {
            Some(Directive::ConditionalOpen) => conditional_depth += 1,
            Some(Directive::ConditionalClose) => {
                conditional_depth = conditional_depth.saturating_sub(1)
            }
            Some(Directive::Neutral) | None => {}
        }
        lines.push(SourceLine {
            start,
            end,
            code,
            conditional_depth,
        });
        start = end;
    }
    (lines, unterminated_string)
}

fn strip_comment_and_strings(line: &str) -> (String, bool) {
    let bytes = line.as_bytes();
    let mut code = String::with_capacity(line.len());
    let mut index = 0usize;
    let mut closed = true;
    while index < bytes.len() {
        match bytes[index] {
            b'\'' => break,
            b'"' => {
                index += 1;
                let mut closed_here = false;
                while index < bytes.len() {
                    if bytes[index] == b'"' {
                        if bytes.get(index + 1) == Some(&b'"') {
                            index += 2;
                            continue;
                        }
                        index += 1;
                        closed_here = true;
                        break;
                    }
                    index += 1;
                }
                closed &= closed_here;
                // A consumed string still separates tokens.
                code.push(' ');
            }
            byte => {
                code.push(byte as char);
                index += 1;
            }
        }
    }
    let trimmed = code.trim_start();
    if trimmed.len() >= 3 && trimmed[..3].eq_ignore_ascii_case("rem") {
        let rest = &trimmed[3..];
        if rest.is_empty() || rest.starts_with(|character: char| character.is_whitespace()) {
            return (String::new(), closed);
        }
    }
    (code, closed)
}

fn line_imports_mstest(code: &str) -> bool {
    let trimmed = code.trim();
    let Some(rest) = strip_keyword(trimmed, "imports") else {
        return false;
    };
    // `Imports Alias = Namespace` binds a different name; only the plain form
    // makes the bare attribute spelling resolve.
    !rest.contains('=') && rest.trim().eq_ignore_ascii_case(MSTEST_NAMESPACE)
}

fn strip_keyword<'a>(text: &'a str, keyword: &str) -> Option<&'a str> {
    let trimmed = text.trim_start();
    if trimmed.len() < keyword.len() || !trimmed[..keyword.len()].eq_ignore_ascii_case(keyword) {
        return None;
    }
    let rest = &trimmed[keyword.len()..];
    if rest.is_empty() || rest.starts_with(|character: char| character.is_whitespace()) {
        Some(rest)
    } else {
        None
    }
}

/// The exact attribute spelling on a line that begins with it.
///
/// Accepts `<Name>` and `<Name()>`, bare when the file imports the namespace,
/// and always when written fully qualified. XML literals are not attributes:
/// the attribute must begin the line.
fn line_attribute_is(code: &str, name: &str, imports_mstest: bool) -> Option<bool> {
    let trimmed = code.trim();
    let inner = trimmed.strip_prefix('<')?.strip_suffix('>')?.trim();
    let inner = inner.strip_suffix("()").unwrap_or(inner).trim();
    let (namespace, simple) = match inner.rsplit_once('.') {
        Some((namespace, simple)) => (Some(namespace), simple),
        None => (None, inner),
    };
    if !simple.eq_ignore_ascii_case(name) {
        return None;
    }
    match namespace {
        Some(namespace) if namespace.eq_ignore_ascii_case(MSTEST_NAMESPACE) => Some(true),
        Some(_) => Some(false),
        None => Some(imports_mstest),
    }
}

struct TestClassGroup {
    class_start: usize,
    class_end: usize,
    methods: Vec<(usize, usize)>,
}

/// What the scan found, and what it deliberately refused to anchor.
#[derive(Default)]
struct AnchorScan {
    groups: Vec<TestClassGroup>,
    /// An otherwise admitted shape sat inside conditional compilation and was
    /// not anchored.
    skipped_conditional: bool,
}

/// True when any line of the span is compiled only under an unevaluated
/// constant.
///
/// All three lines are checked, not just the attribute, because an anchor's
/// recorded evidence spans from its attribute to its `End`. A span whose end
/// line sits in a branch has a branch-dependent range even when its opening
/// does not.
fn spans_conditional(lines: &[SourceLine], span: [usize; 3]) -> bool {
    span.iter().any(|line| lines[*line].conditional_depth > 0)
}

fn test_class_groups(
    lines: &[SourceLine],
    imports_mstest: bool,
    unresolved_attribute: &mut bool,
) -> AnchorScan {
    let mut scan = AnchorScan::default();
    let mut index = 0usize;
    while index < lines.len() {
        match line_attribute_is(&lines[index].code, "testclass", imports_mstest) {
            Some(true) => {}
            Some(false) => {
                *unresolved_attribute = true;
                index += 1;
                continue;
            }
            None => {
                if line_attribute_is(&lines[index].code, "testmethod", imports_mstest)
                    == Some(false)
                {
                    *unresolved_attribute = true;
                }
                index += 1;
                continue;
            }
        }
        let Some(class_line) = next_declaration(lines, index + 1, "class") else {
            index += 1;
            continue;
        };
        let Some(class_end_line) = closing_line(lines, class_line, "class") else {
            index += 1;
            continue;
        };
        if spans_conditional(lines, [index, class_line, class_end_line]) {
            // A whole class compiled only under a constant is a build variant,
            // not a proven declaration. Its methods go with it.
            scan.skipped_conditional = true;
            index = class_end_line + 1;
            continue;
        }
        let mut methods = Vec::new();
        let mut cursor = class_line + 1;
        while cursor < class_end_line {
            match line_attribute_is(&lines[cursor].code, "testmethod", imports_mstest) {
                Some(true) => {
                    if let Some(sub_line) = next_declaration(lines, cursor + 1, "sub") {
                        if let Some(sub_end) = closing_line(lines, sub_line, "sub") {
                            if spans_conditional(lines, [cursor, sub_line, sub_end]) {
                                scan.skipped_conditional = true;
                            } else {
                                methods.push((lines[cursor].start, lines[sub_end].end));
                            }
                            cursor = sub_end + 1;
                            continue;
                        }
                    }
                }
                Some(false) => *unresolved_attribute = true,
                None => {}
            }
            cursor += 1;
        }
        scan.groups.push(TestClassGroup {
            class_start: lines[index].start,
            class_end: lines[class_end_line].end,
            methods,
        });
        index = class_end_line + 1;
    }
    scan
}

/// The next line declaring `keyword`, skipping only further attribute lines.
fn next_declaration(lines: &[SourceLine], from: usize, keyword: &str) -> Option<usize> {
    let mut index = from;
    while index < lines.len() {
        let code = lines[index].code.trim();
        if code.is_empty() {
            index += 1;
            continue;
        }
        if code.starts_with('<') {
            index += 1;
            continue;
        }
        return declaration_keyword_at(code, keyword).then_some(index);
    }
    None
}

/// True when the line declares `keyword`, allowing the ordinary VB modifiers.
fn declaration_keyword_at(code: &str, keyword: &str) -> bool {
    const MODIFIERS: [&str; 12] = [
        "public",
        "private",
        "friend",
        "protected",
        "shared",
        "overridable",
        "overrides",
        "notoverridable",
        "mustoverride",
        "partial",
        "notinheritable",
        "mustinherit",
    ];
    let mut rest = code.trim();
    loop {
        let Some(word) = rest.split_whitespace().next() else {
            return false;
        };
        if word.eq_ignore_ascii_case(keyword) {
            return true;
        }
        if !MODIFIERS
            .iter()
            .any(|modifier| word.eq_ignore_ascii_case(modifier))
        {
            return false;
        }
        rest = rest[word.len()..].trim_start();
    }
}

/// The line closing the block opened at `from`, matching nested opens.
fn closing_line(lines: &[SourceLine], from: usize, keyword: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = from;
    while index < lines.len() {
        let code = lines[index].code.trim();
        if declaration_keyword_at(code, keyword) {
            depth += 1;
        } else if let Some(rest) = strip_keyword(code, "end") {
            if rest.trim().eq_ignore_ascii_case(keyword) {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
        }
        index += 1;
    }
    None
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
            format!("vb_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: VB_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: VB_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        VisualBasicMsTestParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/CatalogTests.vb",
                    language: Language::VisualBasic,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse VB.NET source")
    }

    fn methods(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(VB_TEST_METHOD_TARGET)
            })
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
                    .strip_prefix("vb_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    const IMPORTED: &str = "Imports Microsoft.VisualStudio.TestTools.UnitTesting\n\n";

    #[test]
    fn admitted_attributes_anchor_the_class_and_its_methods() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\n\
             <TestMethod>\nPublic Sub FiltersCatalog()\nEnd Sub\n\
             Public Sub Helper()\nEnd Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 2, "the helper is not a test");
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::VbTestClass)
                .count(),
            1
        );
    }

    #[test]
    fn case_insensitivity_follows_the_language() {
        let parsed = output(
            "imports microsoft.visualstudio.testtools.unittesting\n\n\
             <testclass()>\npublic class CatalogTests\n\
             <testmethod()>\npublic sub LoadsCatalog()\nend sub\nend class\n",
        );
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn a_fully_qualified_attribute_needs_no_import() {
        let parsed = output(
            "<Microsoft.VisualStudio.TestTools.UnitTesting.TestClass()>\n\
             Public Class CatalogTests\n\
             <Microsoft.VisualStudio.TestTools.UnitTesting.TestMethod()>\n\
             Public Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn without_an_import_or_qualification_nothing_anchors() {
        let parsed = output(
            "<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn a_foreign_namespace_is_reported_rather_than_claimed() {
        let parsed = output(
            "<Other.Framework.TestClass()>\nPublic Class CatalogTests\n\
             <Other.Framework.TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"mstest_attribute_without_import".to_string()));
    }

    #[test]
    fn attribute_text_in_comments_and_strings_never_anchors() {
        let parsed = output(&format!(
            "{IMPORTED}' <TestClass()>\nREM <TestClass()>\n\
             Dim sample As String = \"<TestClass()>\"\n\
             Public Class CatalogTests\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn a_function_is_not_a_test_method() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Function LoadsCatalog() As Boolean\nReturn True\nEnd Function\n\
             End Class\n"
        ));
        assert_eq!(methods(&parsed), 0, "an MSTest test method returns nothing");
    }

    #[test]
    fn a_test_method_outside_a_test_class_is_not_discovered() {
        let parsed = output(&format!(
            "{IMPORTED}Public Class PlainClass\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn an_aliased_import_does_not_bind_the_bare_spelling() {
        let parsed = output(
            "Imports MS = Microsoft.VisualStudio.TestTools.UnitTesting\n\n\
             <TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn nested_classes_close_at_their_own_end() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             Public Class Nested\nEnd Class\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n"
        ));
        assert_eq!(
            methods(&parsed),
            1,
            "the method after a nested class still counts"
        );
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             <TestMethod()>\nPublic Sub LoadsCatalog()\nEnd Sub\nEnd Class\n"
        ));
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn an_unterminated_string_reports_a_degraded_parse() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             \x20       Dim note As String = \"never closed\n    End Sub\nEnd Class\n"
        ));
        assert_eq!(parsed.report.diagnostics.len(), 1);
        assert_eq!(
            parsed.report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );
    }

    #[test]
    fn conditional_compilation_branches_must_not_both_anchor() {
        // `#If` selects one branch at compile time from a constant this
        // frontend does not evaluate. Admitting both invents a member that
        // never compiles; admitting either asserts a constant we do not know.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalogDebug()\n    End Sub\n\
             #Else\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalogRelease()\n    End Sub\n\
             #End If\nEnd Class\n"
        ));
        assert_eq!(
            methods(&parsed),
            0,
            "neither branch may anchor: the constant that selects one is unevaluated"
        );
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn declarations_outside_a_conditional_still_anchor() {
        // The skipped branch understates support; it does not unprove what
        // sits in unconditional code.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #If DEBUG Then\n\
             \x20   <TestMethod()>\n    Public Sub OnlyInDebug()\n    End Sub\n\
             #End If\n\
             \x20   <TestMethod()>\n    Public Sub FiltersCatalog()\n    End Sub\n\
             End Class\n"
        ));
        assert_eq!(methods(&parsed), 2);
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn a_conditional_test_class_does_not_anchor() {
        // The same rule applies one level up: a whole class compiled only
        // under a constant is a build variant, not a proven declaration.
        let parsed = output(&format!(
            "{IMPORTED}#If DEBUG Then\n<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             End Class\n#End If\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::VbTestClass)
                .count(),
            0
        );
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn a_region_is_not_a_conditional() {
        // `#Region` is an editor fold. It compiles unconditionally, so it must
        // not cost an anchor -- and `#End Region` must not be read as the
        // `#End If` that would reopen one.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #Region \"Tests\"\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End Region\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
        assert!(!unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn an_external_source_directive_is_not_a_conditional() {
        // `#ExternalSource` remaps debugger line numbers. It selects no branch.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #ExternalSource(\"Catalog.vb\", 1)\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End ExternalSource\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
        assert!(!unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn a_conditional_import_does_not_bind_the_bare_spelling() {
        // The same unsoundness one level up: an `Imports` selected by an
        // unevaluated constant cannot make a bare attribute name resolve.
        let parsed = output(
            "#If CUSTOM_BUILD Then\n\
             Imports Microsoft.VisualStudio.TestTools.UnitTesting\n\
             #End If\n\n\
             <TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             End Class\n",
        );
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"mstest_attribute_without_import".to_string()));
    }

    #[test]
    fn a_fully_qualified_attribute_survives_a_conditional_import() {
        // A fully qualified name never needed the import, so a conditional one
        // costs it nothing.
        let parsed = output(
            "#If CUSTOM_BUILD Then\n\
             Imports Microsoft.VisualStudio.TestTools.UnitTesting\n\
             #End If\n\n\
             <Microsoft.VisualStudio.TestTools.UnitTesting.TestClass()>\n\
             Public Class CatalogTests\n\
             \x20   <Microsoft.VisualStudio.TestTools.UnitTesting.TestMethod()>\n\
             \x20   Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n",
        );
        assert_eq!(methods(&parsed), 1);
    }

    #[test]
    fn an_end_region_does_not_close_an_open_conditional() {
        // Reading any `#End ...` as a close would reopen the file mid-branch
        // and anchor a declaration the build may not contain.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n#Region \"Tests\"\n#End Region\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End If\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn an_end_external_source_does_not_close_an_open_conditional() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n#ExternalSource(\"Catalog.vb\", 1)\n#End ExternalSource\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #End If\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn directive_matching_follows_the_language_case_rules() {
        // VB.NET is case-insensitive, so a lowercase directive selects a
        // branch exactly as the documented spelling does.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #if debug then\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             #end if\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn a_conditional_inside_a_method_body_still_anchors_it() {
        // The declaration itself is unconditional; only statements inside it
        // vary. Abstaining here would understate support for no reason.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n\
             #If DEBUG Then\n        Assert.IsTrue(True)\n#End If\n\
             \x20   End Sub\nEnd Class\n"
        ));
        assert_eq!(methods(&parsed), 1);
        assert!(!unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn an_unclosed_conditional_keeps_abstaining() {
        // A `#If` with no `#End If` leaves every later declaration selected by
        // an unevaluated constant. Understating support is the safe direction.
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             #If DEBUG Then\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\n\
             End Class\n"
        ));
        assert_eq!(methods(&parsed), 0);
    }

    #[test]
    fn a_well_formed_class_reports_no_diagnostic() {
        let parsed = output(&format!(
            "{IMPORTED}<TestClass()>\nPublic Class CatalogTests\n\
             \x20   <TestMethod()>\n    Public Sub LoadsCatalog()\n    End Sub\nEnd Class\n"
        ));
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn parser_rejects_other_languages() {
        assert_eq!(
            VisualBasicMsTestParser.parse(SourceDocument {
                path: "tests/CatalogTests.vb",
                language: Language::CSharp,
                content_hash: ContentHash::new(
                    "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("hash"),
                repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                text: "",
            }),
            Err(ParseError::UnsupportedLanguage)
        );
    }
}
