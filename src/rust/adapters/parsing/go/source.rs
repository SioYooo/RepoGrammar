//! Bounded Go test-declaration frontend for the ADR-0041 admitted shape.
//!
//! Only `*_test.go` bytes reach this scanner, because the filename is part of
//! the anchor's meaning: `go test` compiles only those files as tests, so a
//! function with a test's exact signature in an ordinary file is not a test.
//!
//! Nothing here starts a process, spawns a descendant, consults a
//! `GOPACKAGESDRIVER`, touches cgo, resolves a module graph, or evaluates a
//! build constraint. ADR-0021's containment obligation is unreached rather than
//! waived: there is nothing running to contain.
//!
//! The scanner is string- and comment-aware by construction. A declaration is
//! recognized only from text that is source, never from a string literal, a
//! comment, or a build directive -- asking "does this text appear" instead of
//! "is this a declaration" is how a scanner mints anchors for prose.

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};

pub const GO_ANCHOR_ENGINE: &str = "repogrammar-go-test-scanner";
pub const GO_ANCHOR_METHOD: &str = "bounded_go_test_declaration_v1";

/// Fixed support target for the one admitted exact anchor.
pub const GO_TEST_FUNCTION_TARGET: &str = "go.testing.T";

const TESTING_IMPORT_PATH: &str = "testing";
const MAX_FUNCTION_UNITS: usize = 4_096;
const MAX_FACTS: usize = 16_384;

/// True for the only filenames this frontend may read.
pub fn is_go_test_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.ends_with("_test.go") && name.len() > "_test.go".len()
}

#[derive(Debug, Default)]
pub struct GoTestSourceParser;

impl SourceParser for GoTestSourceParser {
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

/// How the file binds the `testing` package, which decides the anchor spelling.
#[derive(Debug, Clone, PartialEq, Eq)]
enum TestingBinding {
    /// Not imported at all: no test function can be declared here.
    Absent,
    /// Imported under this local name, so the anchor type is `*<name>.T`.
    Qualified(String),
    /// `import . "testing"` puts `T` in file scope with no qualifier.
    Dot,
    /// `import _ "testing"` binds no name at all.
    Blank,
}

pub(crate) fn parse_output(document: SourceDocument<'_>) -> Result<SourceParseOutput, ParseError> {
    if document.language != Language::Go || !is_go_test_path(document.path) {
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
            "unit:{}#go_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Go,
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
            "go_declaration_scan",
            "source_byte_limit",
            full_range,
            "Go source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts);
    }

    let code = CodeMask::new(document.text);
    let binding = testing_binding(document.text, &code);

    match &binding {
        TestingBinding::Dot | TestingBinding::Blank => {
            // Neither form yields a resolvable parameter spelling, so the file's
            // test declarations are unproven rather than absent.
            facts.push(unknown_fact(
                &module,
                UnknownReasonCode::UnresolvedImport,
                "go_test_declaration",
                if matches!(binding, TestingBinding::Dot) {
                    "dot_testing_import"
                } else {
                    "blank_testing_import"
                },
                module.range.clone(),
                "a dot or blank testing import binds no qualified name, so no test signature resolves",
            )?);
        }
        TestingBinding::Absent | TestingBinding::Qualified(_) => {}
    }

    if code.has_build_directive {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::BuildVariantAmbiguity,
            "go_build_constraint",
            "build_constraint_not_evaluated",
            module.range.clone(),
            "Go build constraints are not evaluated, so this file may be excluded on the target platform",
        )?);
    }

    let mut limit_hit = false;
    for (ordinal, declaration) in function_declarations(document.text, &code)
        .into_iter()
        .enumerate()
    {
        if ordinal >= MAX_FUNCTION_UNITS || facts.len() >= MAX_FACTS - 2 {
            limit_hit = true;
            break;
        }
        let is_test = matches!(&binding, TestingBinding::Qualified(local)
            if declaration.is_admitted_test(local));
        let kind = if is_test {
            CodeUnitKind::GoTestFunction
        } else {
            CodeUnitKind::GoFunction
        };
        let range =
            SourceRange::new(declaration.start, declaration.end).map_err(ParseError::Internal)?;
        let unit = CodeUnit {
            id: CodeUnitId::new(format!(
                "unit:{}#{}:{}-{}:{}",
                document.path,
                kind.as_str(),
                declaration.start,
                declaration.end,
                declaration.name
            ))
            .map_err(ParseError::Internal)?,
            language: Language::Go,
            kind,
            range,
            provenance: provenance.clone(),
        };
        if is_test {
            facts.push(anchor_fact(&unit)?);
        }
        units.push(unit);
    }

    if limit_hit {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::InsufficientSupport,
            "go_declaration_scan",
            "scanner_resource_limit",
            module.range.clone(),
            "Go scanner exceeded a bounded declaration or fact limit",
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

/// Per-byte record of which source is code, so every later question is asked
/// about source rather than about text that merely looks like it.
struct CodeMask {
    is_code: Vec<bool>,
    /// Brace depth immediately before each byte, counting only code braces.
    depth: Vec<u32>,
    has_build_directive: bool,
}

impl CodeMask {
    fn new(text: &str) -> Self {
        let bytes = text.as_bytes();
        let mut is_code = vec![false; bytes.len()];
        let mut depth = vec![0u32; bytes.len()];
        let mut has_build_directive = false;
        let mut index = 0usize;
        let mut brace_depth = 0u32;
        while index < bytes.len() {
            depth[index] = brace_depth;
            match bytes[index] {
                b'/' if bytes.get(index + 1) == Some(&b'/') => {
                    let end = line_end(bytes, index);
                    if text[index..end].starts_with("//go:") {
                        has_build_directive = true;
                    }
                    for slot in depth.iter_mut().take(end).skip(index) {
                        *slot = brace_depth;
                    }
                    index = end;
                }
                b'/' if bytes.get(index + 1) == Some(&b'*') => {
                    let end = block_comment_end(bytes, index);
                    for slot in depth.iter_mut().take(end).skip(index) {
                        *slot = brace_depth;
                    }
                    index = end;
                }
                b'"' => index = interpreted_string_end(bytes, index),
                b'`' => index = raw_string_end(bytes, index),
                b'\'' => index = rune_end(bytes, index),
                byte => {
                    if byte == b'{' {
                        brace_depth = brace_depth.saturating_add(1);
                    } else if byte == b'}' {
                        brace_depth = brace_depth.saturating_sub(1);
                    }
                    is_code[index] = true;
                    index += 1;
                }
            }
        }
        Self {
            is_code,
            depth,
            has_build_directive,
        }
    }

    fn is_code_at(&self, index: usize) -> bool {
        self.is_code.get(index).copied().unwrap_or(false)
    }

    fn depth_at(&self, index: usize) -> u32 {
        self.depth.get(index).copied().unwrap_or(u32::MAX)
    }
}

fn line_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn block_comment_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index < bytes.len() {
        if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
            return index + 2;
        }
        index += 1;
    }
    bytes.len()
}

fn interpreted_string_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'"' => return index + 1,
            b'\n' => return index,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn raw_string_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'`' {
            return index + 1;
        }
        index += 1;
    }
    bytes.len()
}

fn rune_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index += 2,
            b'\'' => return index + 1,
            b'\n' => return index,
            _ => index += 1,
        }
    }
    bytes.len()
}

/// Resolve how this file binds the `testing` package.
fn testing_binding(text: &str, code: &CodeMask) -> TestingBinding {
    let mut binding = TestingBinding::Absent;
    for (index, _) in text.match_indices('"') {
        if code.is_code_at(index) {
            // A quote that survives as code cannot happen; strings are masked.
            continue;
        }
        let bytes = text.as_bytes();
        let end = interpreted_string_end(bytes, index);
        if end <= index + 1 {
            continue;
        }
        let Some(path) = text.get(index + 1..end.saturating_sub(1)) else {
            continue;
        };
        if path != TESTING_IMPORT_PATH {
            continue;
        }
        let Some(prefix) = text.get(..index) else {
            continue;
        };
        if !prefix_is_import_context(prefix, code, index) {
            continue;
        }
        binding = match import_qualifier(prefix) {
            Some(qualifier) if qualifier == "." => TestingBinding::Dot,
            Some(qualifier) if qualifier == "_" => TestingBinding::Blank,
            Some(qualifier) => TestingBinding::Qualified(qualifier),
            None => TestingBinding::Qualified(TESTING_IMPORT_PATH.to_string()),
        };
        break;
    }
    binding
}

/// True when the quoted path sits inside an `import` declaration.
fn prefix_is_import_context(prefix: &str, code: &CodeMask, quote_index: usize) -> bool {
    if code.depth_at(quote_index) != 0 {
        return false;
    }
    // The nearest preceding `import` keyword in code, with no intervening
    // top-level declaration keyword.
    let mut best = None;
    for (index, _) in prefix.match_indices("import") {
        if code.is_code_at(index) {
            best = Some(index);
        }
    }
    let Some(import_index) = best else {
        return false;
    };
    for keyword in ["func ", "type ", "var ", "const "] {
        for (index, _) in prefix.match_indices(keyword) {
            if index > import_index && code.is_code_at(index) {
                return false;
            }
        }
    }
    true
}

/// The alias immediately preceding the quoted import path, if any.
fn import_qualifier(prefix: &str) -> Option<String> {
    let trimmed = prefix.trim_end_matches([' ', '\t']);
    if trimmed.len() == prefix.len() {
        // No space before the quote: `import"testing"` has no alias.
        return None;
    }
    let token_start = trimmed
        .rfind(|character: char| character.is_whitespace() || character == '(')
        .map(|index| index + 1)
        .unwrap_or(0);
    let token = trimmed.get(token_start..)?.trim();
    if token.is_empty() || token == "import" {
        return None;
    }
    if token == "." || token == "_" || is_go_identifier(token) {
        Some(token.to_string())
    } else {
        None
    }
}

/// One package-level function or method declaration.
struct GoDeclaration {
    name: String,
    parameters: String,
    has_result: bool,
    is_method: bool,
    start: usize,
    end: usize,
}

impl GoDeclaration {
    /// The ADR-0041 anchor, whole or not at all.
    fn is_admitted_test(&self, testing_local: &str) -> bool {
        !self.is_method
            && !self.has_result
            && name_is_test(&self.name)
            && parameter_is_testing_t(&self.parameters, testing_local)
    }
}

/// `TestXxx` where `Xxx` does not begin with a lowercase letter. `TestMain` is
/// the package entry point rather than a test and is out of scope.
fn name_is_test(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("Test") else {
        return false;
    };
    if name == "TestMain" {
        return false;
    }
    match rest.chars().next() {
        None => true,
        Some(character) => !character.is_lowercase(),
    }
}

/// Exactly one parameter whose type is a pointer to `T` in the file's own
/// spelling of the testing package. Both `t *testing.T` and `*testing.T` are
/// the declared shape; anything else is not.
fn parameter_is_testing_t(parameters: &str, testing_local: &str) -> bool {
    let parameters = parameters.trim();
    if parameters.is_empty() || parameters.contains(',') {
        return false;
    }
    let expected = format!("*{testing_local}.T");
    if parameters == expected {
        return true;
    }
    match parameters.rsplit_once(char::is_whitespace) {
        Some((name, kind)) => {
            let name = name.trim();
            (is_go_identifier(name) || name == "_") && kind.trim() == expected
        }
        None => false,
    }
}

fn function_declarations(text: &str, code: &CodeMask) -> Vec<GoDeclaration> {
    let bytes = text.as_bytes();
    let mut declarations = Vec::new();
    for (index, _) in text.match_indices("func") {
        if !code.is_code_at(index) || code.depth_at(index) != 0 {
            continue;
        }
        if index > 0 && is_identifier_byte(bytes[index - 1]) {
            continue;
        }
        let after = index + "func".len();
        if bytes
            .get(after)
            .is_some_and(|byte| is_identifier_byte(*byte))
        {
            continue;
        }
        let mut cursor = skip_spaces(bytes, after);
        let mut is_method = false;
        if bytes.get(cursor) == Some(&b'(') {
            // Method receiver; the declaration name follows the receiver list.
            let Some(receiver_end) = matching_paren(bytes, code, cursor) else {
                continue;
            };
            is_method = true;
            cursor = skip_spaces(bytes, receiver_end);
        }
        let name_end = identifier_end(bytes, cursor);
        if name_end == cursor {
            continue;
        }
        let Some(name) = text.get(cursor..name_end) else {
            continue;
        };
        let open = skip_spaces(bytes, name_end);
        if bytes.get(open) != Some(&b'(') {
            continue;
        }
        let Some(close) = matching_paren(bytes, code, open) else {
            continue;
        };
        let Some(parameters) = text.get(open + 1..close.saturating_sub(1)) else {
            continue;
        };
        let after_params = skip_spaces(bytes, close);
        let has_result = bytes.get(after_params) != Some(&b'{');
        let end = declaration_end(bytes, code, after_params);
        declarations.push(GoDeclaration {
            name: name.to_string(),
            parameters: parameters.to_string(),
            has_result,
            is_method,
            start: index,
            end,
        });
    }
    declarations
}

/// Byte just past the `)` matching the `(` at `open`.
fn matching_paren(bytes: &[u8], code: &CodeMask, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        if code.is_code_at(index) {
            match bytes[index] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index + 1);
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    None
}

/// End of the declaration: past its body's closing brace, or the signature end
/// for a body-less declaration.
fn declaration_end(bytes: &[u8], code: &CodeMask, from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && !(code.is_code_at(index) && bytes[index] == b'{') {
        if code.is_code_at(index) && bytes[index] == b'\n' {
            return index;
        }
        index += 1;
    }
    if index >= bytes.len() {
        return bytes.len();
    }
    let mut depth = 0usize;
    while index < bytes.len() {
        if code.is_code_at(index) {
            match bytes[index] {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return index + 1;
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    bytes.len()
}

fn skip_spaces(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    index
}

fn identifier_end(bytes: &[u8], from: usize) -> usize {
    let mut index = from;
    while index < bytes.len() && is_identifier_byte(bytes[index]) {
        index += 1;
    }
    index
}

fn is_identifier_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || !byte.is_ascii()
}

fn is_go_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .next()
            .is_some_and(|character| character == '_' || character.is_alphabetic())
        && value
            .chars()
            .all(|character| character == '_' || character.is_alphanumeric())
}

fn anchor_fact(unit: &CodeUnit) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(GO_TEST_FUNCTION_TARGET).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded Go testing test-function declaration anchor",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            "go_anchor_kind=go_test_function".to_string(),
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
            format!("go_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: GO_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: GO_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output_at(path: &str, text: &str) -> SourceParseOutput {
        GoTestSourceParser
            .parse_with_context_output(
                SourceDocument {
                    path,
                    language: Language::Go,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse Go test source")
    }

    fn output(text: &str) -> SourceParseOutput {
        output_at("pkg/catalog_test.go", text)
    }

    fn anchors(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.target.as_ref().map(SymbolId::as_str) == Some(GO_TEST_FUNCTION_TARGET)
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
                    .strip_prefix("go_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    #[test]
    fn admitted_test_declarations_anchor_in_both_parameter_spellings() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n\n\
             func TestFilters(*testing.T) {}\n\n\
             func Test_underscore(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 3);
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::GoTestFunction)
                .count(),
            3
        );
    }

    #[test]
    fn an_import_alias_moves_the_anchor_spelling() {
        let aliased = output(
            "package catalog\n\nimport tt \"testing\"\n\n\
             func TestLoads(t *tt.T) {}\n",
        );
        assert_eq!(anchors(&aliased), 1);

        // In that same file the unaliased spelling names nothing.
        let stale = output(
            "package catalog\n\nimport tt \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&stale), 0);
    }

    #[test]
    fn dot_and_blank_testing_imports_abstain_instead_of_guessing() {
        for (source, kind) in [
            (
                "package catalog\n\nimport . \"testing\"\n\nfunc TestLoads(t *T) {}\n",
                "dot_testing_import",
            ),
            (
                "package catalog\n\nimport _ \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n",
                "blank_testing_import",
            ),
        ] {
            let parsed = output(source);
            assert_eq!(anchors(&parsed), 0, "{kind} must not anchor");
            assert!(unknown_kinds(&parsed).contains(&kind.to_string()), "{kind}");
        }
    }

    #[test]
    fn declaration_text_in_strings_and_comments_never_anchors() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             // func TestCommented(t *testing.T) {}\n\
             /* func TestBlockCommented(t *testing.T) {} */\n\
             var golden = `func TestRaw(t *testing.T) {}`\n\
             var quoted = \"func TestQuoted(t *testing.T) {}\"\n",
        );
        assert_eq!(anchors(&parsed), 0);
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn a_file_without_the_testing_import_anchors_nothing() {
        let parsed = output("package catalog\n\nfunc TestLoads(t *testing.T) {}\n");
        assert_eq!(anchors(&parsed), 0);
    }

    #[test]
    fn near_miss_signatures_are_functions_not_tests() {
        for source in [
            // lowercase after Test
            "func Testify(t *testing.T) {}",
            // two parameters
            "func TestPair(t *testing.T, extra int) {}",
            // a result
            "func TestResult(t *testing.T) error { return nil }",
            // wrong parameter type
            "func TestBench(b *testing.B) {}",
            // value rather than pointer
            "func TestValue(t testing.T) {}",
            // method, not a package-level function
            "type S struct{}\nfunc (s S) TestMethod(t *testing.T) {}",
            // the package entry point is not a test
            "func TestMain(m *testing.M) {}",
        ] {
            let parsed = output(&format!(
                "package catalog\n\nimport \"testing\"\n\n{source}\n"
            ));
            assert_eq!(anchors(&parsed), 0, "must not anchor: {source}");
        }
    }

    #[test]
    fn nested_declarations_are_not_package_level() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestOuter(t *testing.T) {\n\
             \tinner := func(t *testing.T) {}\n\
             \t_ = inner\n\
             }\n",
        );
        assert_eq!(anchors(&parsed), 1, "only the package-level declaration");
    }

    #[test]
    fn build_directives_are_recorded_not_evaluated() {
        let parsed = output(
            "//go:build linux\n\npackage catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n",
        );
        assert_eq!(anchors(&parsed), 1, "the file still anchors");
        assert!(unknown_kinds(&parsed).contains(&"build_constraint_not_evaluated".to_string()));
    }

    #[test]
    fn only_test_filenames_are_admitted() {
        let parser = GoTestSourceParser;
        for path in ["pkg/catalog.go", "pkg/_test.go", "pkg/testing.go"] {
            assert_eq!(
                parser.parse(SourceDocument {
                    path,
                    language: Language::Go,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text: "package catalog\n",
                }),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
        assert!(is_go_test_path("pkg/catalog_test.go"));
        assert!(!is_go_test_path("pkg/_test.go"));
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed =
            output("package catalog\n\nimport \"testing\"\n\nfunc TestLoads(t *testing.T) {}\n");
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn ir_contains_edges_link_declarations_to_their_module() {
        let parsed = output(
            "package catalog\n\nimport \"testing\"\n\n\
             func TestLoads(t *testing.T) {}\n\nfunc helper() {}\n",
        );
        assert_eq!(parsed.report.ir_nodes.len(), 3);
        assert_eq!(parsed.report.ir_edges.len(), 2);
    }
}
