//! Bounded Delphi DUnitX frontend for the ADR-0044 admitted shape.
//!
//! The dialect evidence is the import, never the suffix: ADR-0032 forbids `.pas`
//! as a dialect oracle and insists Delphi and Free Pascal are never equated, so
//! this frontend requires the unit to name `DUnitX.TestFramework` in a `uses`
//! clause before anything anchors.
//!
//! Nothing here invokes Delphi, `dcc32`, `dcc64`, RAD Studio, MSBuild, Free
//! Pascal, `fpc`, Lazarus, a package, a project, a child process, or the
//! network.
//!
//! Object Pascal separates its delimiters completely -- `//`, `{ }`, `(* *)`
//! for comments and single quotes for strings, escaping by doubling -- so no
//! delimiter serves two purposes and a bounded scan stays exact.

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use super::pascal;
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};

pub const DELPHI_ANCHOR_ENGINE: &str = "repogrammar-delphi-dunitx-scanner";
pub const DELPHI_ANCHOR_METHOD: &str = "bounded_delphi_dunitx_attribute_v1";

/// Fixed support target for the one admitted exact anchor.
pub const DELPHI_TEST_TARGET: &str = "dunitx.Test";

const MAX_UNITS: usize = 4_096;

/// True for the only suffix this frontend may read.
pub fn is_pascal_unit_path(path: &str) -> bool {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .to_ascii_lowercase()
        .ends_with(".pas")
}

#[derive(Debug, Default)]
pub struct DelphiDUnitXParser;

impl SourceParser for DelphiDUnitXParser {
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
    if document.language != Language::ObjectPascal || !is_pascal_unit_path(document.path) {
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
            "unit:{}#pascal_module:0-{}",
            document.path,
            document.text.len()
        ))
        .map_err(ParseError::Internal)?,
        language: Language::ObjectPascal,
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
            "delphi_declaration_scan",
            "source_byte_limit",
            full_range,
            "Object Pascal source exceeded the bounded input-byte limit",
        )?);
        return finish(units, facts, Vec::new());
    }

    let (tokens, flags) = pascal::lex(document.text);
    let mut diagnostics = Vec::new();
    if flags.dialect_selector {
        // ADR-0044's invariance argument holds only while the dialect is the one
        // the import implies. A `{$MODE}` / `{$MODESWITCH}` directive re-selects
        // it, and this frontend cannot evaluate the selection.
        diagnostics.push(degraded(
            document.path,
            "an Object Pascal dialect directive re-selects the language mode, which this frontend does not evaluate",
        ));
    }
    if flags.unterminated_block_comment {
        diagnostics.push(degraded(
            document.path,
            "an Object Pascal block comment is left open at end of file, so any declaration after it was read as comment",
        ));
    }

    let parsed = pascal::parse_unit(document.text, &tokens);
    let unresolved_attribute = parsed.attributed_without_uses;

    if parsed.abstained {
        // A class body held a construct outside the declared subset, so that
        // class yielded no anchor at all. Saying so is the difference between a
        // parser and a scanner: the absence is reported, not silent.
        diagnostics.push(degraded(
            document.path,
            "an Object Pascal class body holds a declaration outside the admitted subset, so that class was not read",
        ));
    }
    if parsed.skipped_conditional {
        // Both branches of a `{$IFDEF}` cannot compile, and RepoGrammar cannot
        // evaluate the define, so neither branch may anchor.
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::BuildVariantAmbiguity,
            "delphi_conditional_compilation",
            "declaration_under_conditional_compilation",
            module.range.clone(),
            "an admitted declaration sits inside conditional compilation, so the define that selects it is unevaluated",
        )?);
    }

    for fixture in &parsed.fixtures {
        for (kind, target, assumption, note, range) in std::iter::once((
            CodeUnitKind::DelphiTestFixture,
            "dunitx.TestFixture",
            "delphi_anchor_kind=dunitx_test_fixture",
            "bounded Delphi DUnitX TestFixture class declaration",
            (fixture.start, fixture.end),
        ))
        .chain(fixture.tests.iter().map(|test| {
            (
                CodeUnitKind::DelphiTestProcedure,
                DELPHI_TEST_TARGET,
                "delphi_anchor_kind=dunitx_test_procedure",
                "bounded Delphi DUnitX Test procedure declaration",
                (test.start, test.end),
            )
        })) {
            if units.len() >= MAX_UNITS {
                facts.push(unknown_fact(
                    &module,
                    UnknownReasonCode::InsufficientSupport,
                    "delphi_declaration_scan",
                    "parser_resource_limit",
                    module.range.clone(),
                    "Object Pascal parser exceeded the bounded unit limit",
                )?);
                break;
            }
            let unit = CodeUnit {
                id: CodeUnitId::new(format!(
                    "unit:{}#{}:{}-{}",
                    document.path,
                    kind.as_str(),
                    range.0,
                    range.1
                ))
                .map_err(ParseError::Internal)?,
                language: Language::ObjectPascal,
                kind,
                range: SourceRange::new(range.0, range.1).map_err(ParseError::Internal)?,
                provenance: provenance.clone(),
            };
            facts.push(anchor_fact(&unit, target, assumption, note)?);
            units.push(unit);
        }
    }

    if unresolved_attribute {
        facts.push(unknown_fact(
            &module,
            UnknownReasonCode::UnresolvedImport,
            "delphi_dunitx_attribute_binding",
            "dunitx_attribute_without_uses",
            module.range.clone(),
            "a DUnitX attribute name appears without a DUnitX.TestFramework uses clause, so the framework and dialect are unproven",
        )?);
    }

    finish(units, facts, diagnostics)
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
            format!("delphi_unknown_kind={kind}"),
        ],
    })
}

fn origin() -> FactOrigin {
    FactOrigin {
        engine: DELPHI_ANCHOR_ENGINE.to_string(),
        engine_version: env!("CARGO_PKG_VERSION").to_string(),
        method: DELPHI_ANCHOR_METHOD.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn output(text: &str) -> SourceParseOutput {
        DelphiDUnitXParser
            .parse_with_context_output(
                SourceDocument {
                    path: "tests/Tests.Catalog.pas",
                    language: Language::ObjectPascal,
                    content_hash: ContentHash::new(
                        "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                    )
                    .expect("hash"),
                    repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
                    text,
                },
                &ParserProjectContext::default(),
            )
            .expect("parse Object Pascal source")
    }

    fn tests(parsed: &SourceParseOutput) -> usize {
        parsed
            .report
            .semantic_facts
            .iter()
            .filter(|fact| fact.target.as_ref().map(SymbolId::as_str) == Some(DELPHI_TEST_TARGET))
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
                    .strip_prefix("delphi_unknown_kind=")
                    .map(String::from)
            })
            .collect()
    }

    const UNIT_HEAD: &str = "unit Tests.Catalog;\ninterface\nuses DUnitX.TestFramework;\ntype\n";

    #[test]
    fn admitted_attributes_anchor_the_fixture_and_its_procedures() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n\
                 [Test]\n    procedure FiltersCatalog;\n\
                 procedure Helper;\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(
            tests(&parsed),
            2,
            "the unattributed procedure is not a test"
        );
        assert_eq!(
            parsed
                .report
                .units
                .iter()
                .filter(|unit| unit.kind == CodeUnitKind::DelphiTestFixture)
                .count(),
            1
        );
    }

    #[test]
    fn without_the_dunitx_uses_clause_nothing_anchors() {
        let parsed = output(
            "unit Tests.Catalog;\ninterface\nuses System.SysUtils;\ntype\n\
               [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\nimplementation\nend.\n",
        );
        assert_eq!(tests(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"dunitx_attribute_without_uses".to_string()));
    }

    #[test]
    fn attribute_text_in_comments_and_strings_never_anchors() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  // [TestFixture]\n  {{ [TestFixture] }}\n  (* [Test] *)\n\
               Sample = '[TestFixture]';\nimplementation\nend.\n"
        ));
        assert_eq!(tests(&parsed), 0);
        assert!(parsed
            .report
            .units
            .iter()
            .all(|unit| unit.kind == CodeUnitKind::Module));
    }

    #[test]
    fn a_block_comment_spanning_lines_hides_everything_inside_it() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  {{\n  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\n  }}\nimplementation\nend.\n"
        ));
        assert_eq!(tests(&parsed), 0);
    }

    #[test]
    fn a_test_after_a_plain_class_does_not_inherit_the_fixture() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\n\
               TPlainHelper = class\n  public\n\
                 [Test]\n    procedure NotDiscovered;\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(
            tests(&parsed),
            1,
            "the plain class clears the fixture state"
        );
    }

    #[test]
    fn the_fixtures_end_closes_it_so_a_later_bare_procedure_is_not_a_test() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\n\n\
               [Test]\n  procedure NotInAnyClass;\nimplementation\nend.\n"
        ));
        assert_eq!(
            tests(&parsed),
            1,
            "DUnitX discovers methods of a fixture class, not unit-level procedures"
        );
    }

    #[test]
    fn a_function_is_not_an_admitted_test() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    function LoadsCatalog: Boolean;\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(tests(&parsed), 0);
    }

    #[test]
    fn a_metaclass_declaration_is_not_a_class_declaration() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogClass = class of TObject;\n\
               TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\nimplementation\nend.\n"
        ));
        // The metaclass is not the fixture's class, so the fixture never opens
        // and the later plain class does not inherit it.
        assert_eq!(tests(&parsed), 0);
    }

    #[test]
    fn case_insensitivity_follows_the_language() {
        let parsed = output(
            "unit Tests.Catalog;\ninterface\nuses dunitx.testframework;\ntype\n\
               [testfixture]\n  TCatalogTests = CLASS\n  public\n\
                 [test]\n    PROCEDURE LoadsCatalog;\n  end;\nimplementation\nend.\n",
        );
        assert_eq!(tests(&parsed), 1);
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\nimplementation\nend.\n"
        ));
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn an_unclosed_block_comment_reports_a_degraded_parse() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\n  {{ never closed\n"
        ));
        assert_eq!(
            parsed.report.diagnostics.len(),
            1,
            "an open block comment is a decidable well-formedness violation"
        );
        assert_eq!(
            parsed.report.diagnostics[0].severity,
            ParseDiagnosticSeverity::Error
        );
        // The anchors that were found are still real; the diagnostic says the
        // ones that were not found prove nothing.
        assert_eq!(tests(&parsed), 1);
    }

    #[test]
    fn a_well_formed_unit_reports_no_diagnostic() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\nimplementation\nend.\n"
        ));
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn conditional_compilation_branches_must_not_both_anchor() {
        // `{$IFDEF}` selects one branch at compile time. RepoGrammar cannot
        // evaluate the define, so admitting both branches invents a member that
        // never compiles -- and admitting either one asserts a define we do not
        // know.
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
             {{$IFDEF DEBUG}}\n\
                 [Test]\n    procedure LoadsCatalogDebug;\n\
             {{$ELSE}}\n\
                 [Test]\n    procedure LoadsCatalogRelease;\n\
             {{$ENDIF}}\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(
            tests(&parsed),
            0,
            "neither branch may anchor: the define that selects one is unevaluated"
        );
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn declarations_outside_a_conditional_still_anchor() {
        // The skipped branch understates support; it does not unprove what sits
        // in unconditional code.
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n\
             {{$IFDEF DEBUG}}\n\
                 [Test]\n    procedure OnlyInDebug;\n\
             {{$ENDIF}}\n\
                 [Test]\n    procedure FiltersCatalog;\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(tests(&parsed), 2);
        assert!(unknown_kinds(&parsed)
            .contains(&"declaration_under_conditional_compilation".to_string()));
    }

    #[test]
    fn a_dialect_directive_reports_a_degraded_parse() {
        // `{$MODE}` re-selects the language, and ADR-0044's invariance argument
        // only holds for the dialect the import implies.
        let parsed = output(
            "unit Tests.Catalog;\n{$MODE OBJFPC}\ninterface\nuses DUnitX.TestFramework;\ntype\n\
               [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n  end;\nimplementation\nend.\n",
        );
        assert!(parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error));
    }

    #[test]
    fn a_nested_type_no_longer_closes_the_fixture_early() {
        // ADR-0044 D2 had to record this as a chosen false negative while the
        // frontend was line-oriented: a nested type inside the fixture closes
        // with its own `end`, which ended the block early and dropped every
        // later `[Test]`. The grammar closes the nested type instead.
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog;\n\
             \x20   type\n      TInner = class\n      public\n        procedure Helper;\n      end;\n\
                 [Test]\n    procedure FiltersCatalog;\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(
            tests(&parsed),
            2,
            "the nested type's own end must not close the fixture"
        );
    }

    #[test]
    fn ordinary_members_do_not_stop_the_fixture_being_read() {
        // Fields, properties, visibility sections, class methods and method
        // directives are structure the parser must understand well enough to
        // keep going -- the type each of them names is never interpreted.
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class(TObject)\n\
             \x20 strict private\n    FCount: Integer;\n\
             \x20   FNames: array[0..3] of string;\n\
             \x20 public\n    property Count: Integer read FCount;\n\
             \x20   class procedure Prepare; static;\n\
             \x20   constructor Create; overload; virtual;\n\
                 [Test]\n    procedure LoadsCatalog;\n\
                 [Test]\n    procedure FiltersCatalog(const Name: string); overload;\n  end;\n\
             implementation\nend.\n"
        ));
        assert_eq!(tests(&parsed), 2);
        assert!(
            parsed.report.diagnostics.is_empty(),
            "these are all in the declared subset: {:?}",
            parsed.report.diagnostics
        );
    }

    #[test]
    fn an_attribute_binds_to_the_declaration_the_grammar_gives_it() {
        // A `[Test]` sitting before a field binds to that field, not to a
        // procedure further down. Line-oriented attribution could not tell.
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    FMisplaced: Integer;\n\
             \x20   procedure NotAttributed;\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(tests(&parsed), 0);
    }

    #[test]
    fn a_declaration_spanning_lines_is_one_declaration() {
        let parsed = output(&format!(
            "{UNIT_HEAD}  [TestFixture]\n  TCatalogTests = class\n  public\n\
                 [Test]\n    procedure LoadsCatalog(\n      const Name: string;\n\
             \x20     Count: Integer\n    );\n  end;\nimplementation\nend.\n"
        ));
        assert_eq!(tests(&parsed), 1);
    }

    #[test]
    fn only_pascal_units_are_admitted() {
        let parser = DelphiDUnitXParser;
        for path in ["src/App.dpr", "src/App.dpk"] {
            assert_eq!(
                parser.parse(SourceDocument {
                    path,
                    language: Language::ObjectPascal,
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
        assert!(is_pascal_unit_path("tests/Tests.Catalog.pas"));
        assert!(is_pascal_unit_path("tests/Tests.Catalog.PAS"));
    }
}
