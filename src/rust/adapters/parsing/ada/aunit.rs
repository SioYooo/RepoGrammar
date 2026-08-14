//! Bounded Ada AUnit frontend for the ADR-0045 admitted shape.
//!
//! Nothing here invokes GNAT, `gprbuild`, `gnattest`, Alire, Libadalang, a
//! child process, or the network. ADR-0033 D3's Libadalang `NO_GO` stands.
//!
//! Ada's single quote is genuinely overloaded -- it opens a character literal
//! and it introduces an attribute -- and the overload is resolved by the local
//! rule real Ada lexers use: a tick whose preceding non-blank character is an
//! identifier character or `)` is an attribute tick; otherwise it opens a
//! character literal, which is then exactly three bytes wide. The fixed width
//! is what makes `'''` work without a special case.

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseError, ParseReport, ParserProjectContext, SourceDocument, SourceParseOutput, SourceParser,
};

pub const ADA_ANCHOR_ENGINE: &str = "repogrammar-ada-aunit-scanner";
pub const ADA_ANCHOR_METHOD: &str = "bounded_ada_aunit_registration_v1";

/// Fixed support target for the one admitted exact anchor.
pub const ADA_TEST_TARGET: &str = "aunit.Register_Routine";

const REGISTER_ROUTINE: &str = "register_routine";
const MAX_UNITS: usize = 4_096;

/// True for the only suffix this frontend may read.
pub fn is_ada_body_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
        .ends_with(".adb")
}

#[derive(Debug, Default)]
pub struct AdaAUnitParser;

impl SourceParser for AdaAUnitParser {
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
    if document.language != Language::Ada || !is_ada_body_path(document.path) {
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
            "unit:{}#ada_body:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::Ada,
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
            "ada_registration_scan",
            "source_byte_limit",
            full_range,
            "Ada source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts);
    }

    let mask = CodeMask::new(document.text);
    let calls = register_routine_calls(document.text, &mask);
    let mut unresolved_registration = false;

    if !withs_aunit(document.text, &mask) {
        unresolved_registration = !calls.is_empty();
    } else {
        for call in calls {
            if units.len() >= MAX_UNITS {
                facts.push(unknown_fact(
                    &module,
                    UnknownReasonCode::InsufficientSupport,
                    "ada_registration_scan",
                    "scanner_resource_limit",
                    module.range.clone(),
                    "Ada scanner exceeded the bounded unit limit",
                )?);
                break;
            }
            let unit = CodeUnit {
                id: CodeUnitId::new(format!(
                    "unit:{}#ada_test_registration:{}-{}",
                    document.path, call.start, call.end
                ))
                .map_err(ParseError::Internal)?,
                language: Language::Ada,
                kind: CodeUnitKind::AdaTestRegistration,
                range: SourceRange::new(call.start, call.end).map_err(ParseError::Internal)?,
                provenance: provenance.clone(),
            };
            facts.push(anchor_fact(&unit)?);
            units.push(unit);
        }
    }

    if unresolved_registration {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::UnresolvedImport,
            "ada_aunit_registration_binding",
            "registration_without_aunit_with_clause",
            module.range.clone(),
            "a Register_Routine call appears without an AUnit with clause in the same file, so the framework is unproven",
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
        )
            .cmp(&(
                right.evidence.range.start_byte,
                right.evidence.range.end_byte,
                right.kind.as_protocol_str(),
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

/// Per-byte "this byte is code" mask over `--` comments, `"…"` strings, and
/// character literals.
struct CodeMask {
    is_code: Vec<bool>,
}

impl CodeMask {
    fn new(text: &str) -> Self {
        let bytes = text.as_bytes();
        let mut is_code = vec![false; bytes.len()];
        let mut index = 0usize;
        // The previous non-blank code byte decides what a tick means, and it may
        // sit on an earlier line, so it is tracked across the whole document.
        let mut previous_code_byte: Option<u8> = None;
        while index < bytes.len() {
            match bytes[index] {
                b'-' if bytes.get(index + 1) == Some(&b'-') => {
                    while index < bytes.len() && bytes[index] != b'\n' {
                        index += 1;
                    }
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
                    previous_code_byte = Some(b'"');
                }
                b'\''
                    if !tick_is_attribute(previous_code_byte)
                        && bytes.get(index + 2) == Some(&b'\'') =>
                {
                    // A character literal is exactly three bytes wide, which is
                    // what makes `'''` need no special case.
                    index += 3;
                    previous_code_byte = Some(b'\'');
                }
                byte => {
                    is_code[index] = true;
                    if !byte.is_ascii_whitespace() {
                        previous_code_byte = Some(byte);
                    }
                    index += 1;
                }
            }
        }
        Self { is_code }
    }

    fn is_code(&self, index: usize) -> bool {
        self.is_code.get(index).copied().unwrap_or(false)
    }
}

fn tick_is_attribute(previous: Option<u8>) -> bool {
    previous.is_some_and(|byte| is_identifier_byte(byte) || byte == b')')
}

fn is_identifier_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// True when a `with` context clause names a unit whose first identifier is
/// `AUnit`. `with` also introduces record extensions and aspects, where the
/// next identifier is never `AUnit`.
fn withs_aunit(text: &str, mask: &CodeMask) -> bool {
    let bytes = text.as_bytes();
    for (start, end) in identifier_tokens(text, mask) {
        if !text[start..end].eq_ignore_ascii_case("with") {
            continue;
        }
        let mut index = end;
        while index < bytes.len() && (!mask.is_code(index) || bytes[index].is_ascii_whitespace()) {
            index += 1;
        }
        let name_end = identifier_end(bytes, mask, index);
        if name_end > index && text[index..name_end].eq_ignore_ascii_case("aunit") {
            return true;
        }
    }
    false
}

struct RegistrationCall {
    start: usize,
    end: usize,
}

/// Every `Register_Routine (T, Name'Access, "literal")` call, bare or through a
/// dotted prefix, with the arguments possibly spanning lines.
fn register_routine_calls(text: &str, mask: &CodeMask) -> Vec<RegistrationCall> {
    let bytes = text.as_bytes();
    let mut calls = Vec::new();
    for (start, end) in identifier_tokens(text, mask) {
        if !text[start..end].eq_ignore_ascii_case(REGISTER_ROUTINE) {
            continue;
        }
        let mut index = end;
        while index < bytes.len() && (!mask.is_code(index) || bytes[index].is_ascii_whitespace()) {
            index += 1;
        }
        if bytes.get(index) != Some(&b'(') || !mask.is_code(index) {
            continue;
        }
        let Some(close) = matching_paren(bytes, mask, index) else {
            continue;
        };
        let arguments = split_top_level(text, mask, index + 1, close);
        if arguments.len() != 3 {
            continue;
        }
        if !argument_is_access_attribute(text, &arguments[1])
            || !argument_is_string_literal(text, &arguments[2])
        {
            continue;
        }
        calls.push(RegistrationCall {
            start: dotted_prefix_start(bytes, mask, start),
            end: close + 1,
        });
    }
    calls
}

/// Walk back over `Pkg.Sub.` so the unit covers the whole callee name.
fn dotted_prefix_start(bytes: &[u8], mask: &CodeMask, identifier_start: usize) -> usize {
    let mut start = identifier_start;
    loop {
        let mut index = start;
        while index > 0
            && mask.is_code(index - 1)
            && bytes[index - 1].is_ascii_whitespace()
            && bytes[index - 1] != b'\n'
        {
            index -= 1;
        }
        if index == 0 || !mask.is_code(index - 1) || bytes[index - 1] != b'.' {
            return start;
        }
        index -= 1;
        while index > 0 && mask.is_code(index - 1) && is_identifier_byte(bytes[index - 1]) {
            index -= 1;
        }
        if index == start {
            return start;
        }
        start = index;
    }
}

fn matching_paren(bytes: &[u8], mask: &CodeMask, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        if mask.is_code(index) {
            match bytes[index] {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(index);
                    }
                }
                _ => {}
            }
        }
        index += 1;
    }
    None
}

struct Argument {
    start: usize,
    end: usize,
}

fn split_top_level(text: &str, mask: &CodeMask, start: usize, end: usize) -> Vec<Argument> {
    let bytes = text.as_bytes();
    let mut arguments = Vec::new();
    let mut depth = 0usize;
    let mut argument_start = start;
    for (index, byte) in bytes.iter().enumerate().take(end).skip(start) {
        if !mask.is_code(index) {
            continue;
        }
        match *byte {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                arguments.push(Argument {
                    start: argument_start,
                    end: index,
                });
                argument_start = index + 1;
            }
            _ => {}
        }
    }
    arguments.push(Argument {
        start: argument_start,
        end,
    });
    arguments
}

/// A name followed by `'Access`, with the tick read as an attribute tick.
fn argument_is_access_attribute(text: &str, argument: &Argument) -> bool {
    let value = text[argument.start..argument.end].trim();
    let Some((name, attribute)) = value.rsplit_once('\'') else {
        return false;
    };
    attribute.trim().eq_ignore_ascii_case("access") && is_dotted_name(name.trim())
}

fn argument_is_string_literal(text: &str, argument: &Argument) -> bool {
    let value = text[argument.start..argument.end].trim();
    value.len() >= 2 && value.starts_with('"') && value.ends_with('"')
}

fn is_dotted_name(value: &str) -> bool {
    !value.is_empty()
        && value.split('.').all(|part| {
            let part = part.trim();
            !part.is_empty()
                && part.starts_with(|character: char| character.is_ascii_alphabetic())
                && part
                    .bytes()
                    .all(|byte| is_identifier_byte(byte) || byte == b' ')
        })
}

fn identifier_end(bytes: &[u8], mask: &CodeMask, start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && mask.is_code(index) && is_identifier_byte(bytes[index]) {
        index += 1;
    }
    index
}

/// Every maximal code identifier token, as byte ranges.
fn identifier_tokens(text: &str, mask: &CodeMask) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        if !mask.is_code(index) || !is_identifier_byte(bytes[index]) {
            index += 1;
            continue;
        }
        if index > 0 && mask.is_code(index - 1) && is_identifier_byte(bytes[index - 1]) {
            index += 1;
            continue;
        }
        let end = identifier_end(bytes, mask, index);
        tokens.push((index, end));
        index = end;
    }
    tokens
}

fn anchor_fact(unit: &CodeUnit) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind: SemanticFactKind::Symbol,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(ADA_TEST_TARGET).map_err(ParseError::Internal)?),
        origin: origin(),
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            unit.id.clone(),
            unit.range.clone(),
            unit.provenance.clone(),
            "bounded Ada AUnit Register_Routine anchor",
        )
        .map_err(ParseError::Internal)?,
        assumptions: vec![
            "provider_resolved=false".to_string(),
            "ada_anchor_kind=aunit_register_routine".to_string(),
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
            format!("ada_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: ADA_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: ADA_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        AdaAUnitParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/catalog_tests.adb",
                    language: Language::Ada,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse Ada source")
    }

    fn registrations(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(ADA_TEST_TARGET))
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
                    .strip_prefix("ada_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    const HEAD: &str = "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n";

    #[test]
    fn admitted_registrations_anchor_including_multi_line_and_prefixed_calls() {
        let parsed = output(&format!(
            "{HEAD}   procedure Register_Tests (T : in out Test_Case) is\n   begin\n\
                     Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
                     Registration.Register_Routine\n        (T, Filters_Catalog'Access, \"filters\");\n\
                     AUnit.Test_Cases.Registration.Register_Routine (T, Sorts'Access, \"sorts\");\n\
                  end Register_Tests;\nend Catalog_Tests;\n"
        ));
        assert_eq!(registrations(&parsed), 3);
    }

    #[test]
    fn without_an_aunit_with_clause_nothing_anchors() {
        let parsed = output(
            "with Ada.Text_IO;\npackage body Catalog_Tests is\n\
               begin\n   Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
             end Catalog_Tests;\n",
        );
        assert_eq!(registrations(&parsed), 0);
        assert!(
            unknown_kinds(&parsed).contains(&"registration_without_aunit_with_clause".to_string())
        );
    }

    #[test]
    fn a_registration_in_a_comment_or_a_string_never_anchors() {
        let parsed = output(&format!(
            "{HEAD}   --  Register_Routine (T, In_A_Comment'Access, \"no\");\n\
                  Note : constant String := \"Register_Routine (T, In_A_String'Access, \"\"no\"\")\";\n\
             end Catalog_Tests;\n"
        ));
        assert_eq!(registrations(&parsed), 0);
    }

    #[test]
    fn a_quote_character_literal_does_not_swallow_the_rest_of_the_file() {
        // `'''` is the shape that breaks a scanner searching for a closing tick.
        let parsed = output(&format!(
            "{HEAD}   Tick : constant Character := ''';\n\
                  Paren : constant Character := '(';\n\
                  Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
                  Register_Routine (T, Filters'Access, \"filters\");\n\
             end Catalog_Tests;\n"
        ));
        assert_eq!(registrations(&parsed), 2);
    }

    #[test]
    fn an_attribute_tick_after_an_identifier_or_a_paren_is_not_a_character_literal() {
        let parsed = output(&format!(
            "{HEAD}   Size : constant Natural := Items (I)'Length + Integer'First;\n\
                  Address : constant System.Address := Ptr.all'Address;\n\
                  Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
             end Catalog_Tests;\n"
        ));
        assert_eq!(
            registrations(&parsed),
            1,
            "the ticks above are attributes, so no character literal opens"
        );
    }

    #[test]
    fn the_argument_shape_is_required() {
        for call in [
            // Two arguments.
            "Register_Routine (T, Loads_Catalog'Access);",
            // A variable instead of an access attribute.
            "Register_Routine (T, Routine_Ptr, \"loads\");",
            // A non-literal description.
            "Register_Routine (T, Loads_Catalog'Access, Description);",
            // A different attribute.
            "Register_Routine (T, Loads_Catalog'Address, \"loads\");",
            // Four arguments.
            "Register_Routine (T, Loads_Catalog'Access, \"loads\", Extra);",
        ] {
            let parsed = output(&format!("{HEAD}   {call}\nend Catalog_Tests;\n"));
            assert_eq!(registrations(&parsed), 0, "{call}");
        }
    }

    #[test]
    fn a_nested_call_in_the_first_argument_does_not_split_the_arguments() {
        let parsed = output(&format!(
            "{HEAD}   Register_Routine (Fixture (T, 1), Loads_Catalog'Access, \"loads\");\n\
             end Catalog_Tests;\n"
        ));
        assert_eq!(registrations(&parsed), 1);
    }

    #[test]
    fn an_identifier_ending_in_the_callee_name_is_not_the_callee() {
        let parsed = output(&format!(
            "{HEAD}   My_Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
             end Catalog_Tests;\n"
        ));
        assert_eq!(registrations(&parsed), 0);
    }

    #[test]
    fn case_insensitivity_follows_the_language() {
        let parsed = output(
            "WITH Aunit.Test_Cases;\npackage body Catalog_Tests is\n\
               REGISTER_ROUTINE (T, Loads_Catalog'ACCESS, \"loads\");\nend Catalog_Tests;\n",
        );
        assert_eq!(registrations(&parsed), 1);
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(&format!(
            "{HEAD}   Register_Routine (T, Loads_Catalog'Access, \"loads\");\nend Catalog_Tests;\n"
        ));
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn only_ada_bodies_are_admitted() {
        let parser = AdaAUnitParser;
        for path in ["src/catalog.ads", "project.gpr"] {
            assert_eq!(
                parser.parse(SourceDocument {
                    path,
                    language: Language::Ada,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text: "",
                }),
                Err(ParseError::UnsupportedLanguage),
                "{path}"
            );
        }
        assert!(is_ada_body_path("tests/catalog_tests.adb"));
        assert!(is_ada_body_path("tests/CATALOG_TESTS.ADB"));
    }
}
