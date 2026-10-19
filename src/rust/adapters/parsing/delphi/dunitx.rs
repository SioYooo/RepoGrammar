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

use super::super::{ir_edges_for_units, ir_nodes_for_units, sort_anchor_facts};
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
    parse_output_with(document, pascal::parse_unit)
}

type UnitReader =
    fn(&str, &[pascal::Token], &pascal::LexFlags) -> Result<pascal::UnitParse, ParseError>;

/// The reader is a parameter only so the migration differential can run the
/// legacy parser through the identical downstream code.
fn parse_output_with(
    document: SourceDocument<'_>,
    read_unit: UnitReader,
) -> Result<SourceParseOutput, ParseError> {
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

    let parsed = read_unit(document.text, &tokens, &flags)?;
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
    sort_anchor_facts(&mut facts);
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
               TSample = Integer;\nconst\n  Sample = '[TestFixture]';\nimplementation\nend.\n"
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

    // ---- ADR-0058 migration differential (temporary; deleted with the legacy parser) ----

    fn legacy_reader(
        text: &str,
        tokens: &[pascal::Token],
        _flags: &pascal::LexFlags,
    ) -> Result<pascal::UnitParse, ParseError> {
        Ok(pascal::legacy::parse_unit(text, tokens))
    }

    fn document(text: &str) -> SourceDocument<'_> {
        SourceDocument {
            path: "tests/Tests.Catalog.pas",
            language: Language::ObjectPascal,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("revision"),
            text,
        }
    }

    fn both(text: &str) -> (SourceParseOutput, SourceParseOutput) {
        (
            parse_output_with(document(text), legacy_reader).expect("legacy"),
            parse_output_with(document(text), pascal::parse_unit).expect("tree-sitter"),
        )
    }

    fn unit_with(uses: &str, body: &str) -> String {
        format!(
            "unit Tests.Catalog;\ninterface\nuses {uses};\ntype\n{body}\nimplementation\nend.\n"
        )
    }

    fn dunitx_unit(body: &str) -> String {
        unit_with("System.SysUtils, DUnitX.TestFramework", body)
    }

    fn branch_corpus() -> Vec<(&'static str, String)> {
        let fixture = |members: &str| {
            dunitx_unit(&format!(
                "  [TestFixture]\n  TCatalogTests = class\n  public\n{members}\n  end;\n"
            ))
        };
        let mut corpus: Vec<(&'static str, String)> = vec![
            (
                "exact",
                fixture("[Test]\n procedure A;\n [Test]\n procedure B;\n procedure Helper;"),
            ),
            (
                "function_not_test",
                fixture("[Test]\n function A: Boolean;\n [Test]\n constructor Create;\n [Test]\n destructor Destroy; override;"),
            ),
            (
                "attribute_variants",
                fixture("[Test, Ignore]\n procedure A;\n [Test(1)]\n procedure B;\n [TestCase('a','1,2')]\n procedure C;\n [DUnitX.Test]\n procedure D;\n [Ignore]\n [Test]\n procedure E;\n [Ignore('x')][Test]\n procedure F;\n [Ignore]\n procedure G;"),
            ),
            (
                "directives",
                fixture("[Test]\n procedure A; overload;\n [Test]\n procedure B; virtual; abstract;\n [Test]\n procedure C(const N: string; X: Integer); stdcall;\n [Test]\n procedure D; deprecated 'old';"),
            ),
            (
                "members",
                dunitx_unit("  [TestFixture]\n  TCatalogTests = class(TObject)\n  strict private\n    FCount: Integer;\n    FNames: array[0..3] of string;\n  public\n    property Count: Integer read FCount;\n    class procedure Prepare; static;\n    constructor Create; overload; virtual;\n    [Test]\n    procedure A;\n    [Test]\n    procedure B(const Name: string); overload;\n  end;\n"),
            ),
            (
                "multiline_decl",
                fixture("[Test]\n procedure A(\n const Name: string;\n Count: Integer\n );"),
            ),
            (
                "attribute_before_field",
                fixture("[Test]\n FMisplaced: Integer;\n procedure NotAttributed;"),
            ),
            (
                "nested_type",
                dunitx_unit("  [TestFixture]\n  TCatalogTests = class\n  public\n    [Test]\n    procedure A;\n    type\n      TInner = class\n      public\n        [Test]\n        procedure Helper;\n      end;\n    [Test]\n    procedure B;\n  end;\n"),
            ),
            (
                "two_fixtures_and_plain",
                dunitx_unit("  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n  TPlain = class\n  public\n    [Test]\n    procedure N;\n  end;\n  [TestFixture]\n  TB = class(TObject, IFoo)\n  public\n    [Test]\n    procedure B;\n  end;\n"),
            ),
            (
                "metaclass_then_class",
                dunitx_unit("  [TestFixture]\n  TCatalogClass = class of TObject;\n  TCatalogTests = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "forward_declarations",
                dunitx_unit("  [TestFixture]\n  TFwd = class;\n  TOther = class;\n  [TestFixture]\n  TReal = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "forward_with_ancestor_abstains",
                dunitx_unit("  [TestFixture]\n  TFwd = class(TObject);\n  [TestFixture]\n  TReal = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "generic_fixture",
                dunitx_unit("  [TestFixture]\n  TGen<T> = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "no_uses_unbound",
                "unit T;\ninterface\nuses System.SysUtils;\ntype\n[TestFixture]\nTA = class\npublic\n[Test]\nprocedure A;\nend;\nimplementation\nend.\n".to_string(),
            ),
            (
                "no_uses_at_all_plain",
                "unit T;\ninterface\ntype\nTA = class\npublic\nprocedure A;\nend;\nimplementation\nend.\n".to_string(),
            ),
            (
                "case_insensitive",
                "unit T;\ninterface\nuses dunitx.testframework;\ntype\n[testfixture]\nTA = CLASS\npublic\n[test]\nPROCEDURE A;\nend;\nimplementation\nend.\n".to_string(),
            ),
            (
                "uses_in_clause",
                unit_with("System.SysUtils in 'x.pas', DUnitX.TestFramework", "  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "uses_prefix_qualified_does_not_bind",
                unit_with("Other.DUnitX.TestFramework", "  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "comments_and_strings",
                dunitx_unit("  // [TestFixture]\n  { [TestFixture] }\n  (* [Test] *)\n  [TestFixture]\n  TA = class\n  public\n    // [Test]\n    {[Test]}\n    procedure NotTest;\n  end;\nconst\n  Sample = '[TestFixture]';\n  Other = '[Test]';\n"),
            ),
            (
                "non_ascii",
                dunitx_unit("  // caf\u{e9} \u{4e2d}\u{6587}\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A; // \u{4e2d}\n  end;\n"),
            ),
            (
                "enum_set_proc_types_before",
                dunitx_unit("  TColor = (Red, Green);\n  TColors = set of TColor;\n  TProc2 = procedure(X: Integer);\n  TArr = array[0..2] of Integer;\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "record_after_fixture",
                dunitx_unit("  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n  TRec = record\n    X: Integer;\n  end;\n"),
            ),
            (
                "two_type_sections",
                "unit T;\ninterface\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nconst\n  C = 1;\ntype\n  [TestFixture]\n  TB = class\n  public\n    [Test]\n    procedure B;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            (
                "implementation_type_not_read",
                "unit T;\ninterface\nuses DUnitX.TestFramework;\nimplementation\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nend.\n".to_string(),
            ),
            (
                "uses_in_implementation_only",
                "unit T;\ninterface\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nuses DUnitX.TestFramework;\nend.\n".to_string(),
            ),
            (
                "class_method_test",
                fixture("[Test]\n class procedure A; static;"),
            ),
            (
                "properties_vars_consts",
                dunitx_unit("  [TestFixture]\n  TA = class\n  private\n    FX: Integer;\n  public\n    class var Shared: Integer;\n    const K = 3;\n    property X: Integer read FX write FX;\n    property Items[I: Integer]: string read GetItem; default;\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "visibility_sections",
                dunitx_unit("  [TestFixture]\n  TA = class\n  private\n    procedure P;\n  protected\n    procedure Q;\n  published\n    [Test]\n    procedure A;\n  strict protected\n    procedure R;\n  public\n    [Test]\n    procedure B;\n  end;\n"),
            ),
            (
                "members_without_visibility",
                dunitx_unit("  [TestFixture]\n  TA = class\n    [Test]\n    procedure A;\n  end;\n"),
            ),
            (
                "empty_class",
                dunitx_unit("  [TestFixture]\n  TA = class\n  end;\n"),
            ),
            (
                "fixture_attribute_on_record_is_not_a_class",
                dunitx_unit("  [TestFixture]\n  TRec = record\n    X: Integer;\n  end;\n"),
            ),
            (
                "fixture_in_both_sections_plus_function_fixture_member",
                dunitx_unit("  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  private\n    [Test]\n    procedure B;\n    function C: Integer;\n  end;\n"),
            ),
            (
                "empty_source",
                String::new(),
            ),
            (
                "program_file",
                "program P;\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nbegin\nend.\n".to_string(),
            ),
            (
                "mixed_stacked_attrs_after_class_attr",
                dunitx_unit("  [TestFixture][Category('x')]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n"),
            ),
        ];

        // Conditional compilation, in every placement the legacy depth tracker
        // distinguishes.
        let conditional = |name: &'static str, members: &str| (name, fixture(members));
        corpus.extend([
            conditional("ifdef_else_both_branches", "{$IFDEF DEBUG}\n[Test]\n procedure D;\n{$ELSE}\n[Test]\n procedure R;\n{$ENDIF}"),
            conditional("ifdef_one_branch_among_others", "[Test]\n procedure A;\n{$IFDEF DEBUG}\n[Test]\n procedure B;\n{$ENDIF}\n[Test]\n procedure C;"),
            conditional("ifndef", "{$IFNDEF X}\n[Test]\n procedure A;\n{$ENDIF}"),
            conditional("lowercase_ifdef", "{$ifdef x}\n[Test]\n procedure A;\n{$endif}"),
            conditional("if_ifend", "{$IF Defined(X)}\n[Test]\n procedure A;\n{$IFEND}"),
            conditional("ifopt", "{$IFOPT R+}\n[Test]\n procedure A;\n{$ENDIF}"),
            conditional("nested_ifdef", "{$IFDEF A}\n{$IFDEF B}\n[Test]\n procedure A;\n{$ENDIF}\n[Test]\n procedure B;\n{$ENDIF}\n[Test]\n procedure C;"),
            conditional("attribute_only_in_ifdef", "{$IFDEF X}\n[Test]\n{$ENDIF}\n procedure A;"),
            conditional("procedure_keyword_in_ifdef", "[Test]\n{$IFDEF X}\n procedure A;\n{$ENDIF}"),
            conditional("define_is_not_conditional", "{$DEFINE X}\n[Test]\n procedure A;\n{$UNDEF X}"),
            conditional("paren_star_directive_is_a_comment", "(*$IFDEF X*)\n[Test]\n procedure A;\n(*$ENDIF*)"),
            conditional("stray_endif_saturates", "{$ENDIF}\n[Test]\n procedure A;"),
            conditional("ifdef_function_only", "{$IFDEF X}\n function F: Integer;\n{$ENDIF}\n[Test]\n procedure A;"),
            conditional("directive_with_switch_suffix", "{$IFDEF X+}\n[Test]\n procedure A;\n{$ENDIF}"),
            conditional("compiler_switch_directive", "{$R+}\n[Test]\n procedure A;\n{$R-}"),
        ]);
        corpus.extend([
            (
                "ifdef_around_whole_fixture",
                "unit T;\ninterface\nuses DUnitX.TestFramework;\ntype\n{$IFDEF X}\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n{$ENDIF}\nimplementation\nend.\n".to_string(),
            ),
            (
                "ifdef_around_attribute_of_fixture_only",
                "unit T;\ninterface\nuses DUnitX.TestFramework;\ntype\n{$IFDEF X}\n  [TestFixture]\n{$ENDIF}\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            (
                "ifdef_around_class_keyword_only",
                "unit T;\ninterface\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA =\n{$IFDEF X}\n  class\n{$ENDIF}\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            (
                "ifdef_outside_before_unit_balanced",
                "{$IFDEF X}\n{$ENDIF}\nunit T;\ninterface\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            (
                "ifdef_in_uses_clause",
                "unit T;\ninterface\nuses\n{$IFDEF X}\n  System.SysUtils,\n{$ENDIF}\n  DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            (
                "ifdef_in_implementation",
                "unit T;\ninterface\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\n{$IFDEF X}\nprocedure TA.A;\nbegin\nend;\n{$ENDIF}\nend.\n".to_string(),
            ),
            // Dialect selectors.
            (
                "mode_objfpc",
                "unit T;\n{$MODE OBJFPC}\ninterface\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            (
                "modeswitch",
                "unit T;\n{$modeswitch advancedrecords}\ninterface\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            (
                "mode_delphi_in_ifdef",
                "unit T;\n{$IFDEF FPC}\n{$MODE DELPHI}\n{$ENDIF}\ninterface\nuses DUnitX.TestFramework;\ntype\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\nimplementation\nend.\n".to_string(),
            ),
            // Unterminated comments after a complete, well-formed class.
            (
                "unterminated_brace_after_class",
                dunitx_unit("  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n  { never closed\n"),
            ),
            (
                "unterminated_paren_star_after_class",
                dunitx_unit("  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n  (* never closed\n"),
            ),
            (
                "unterminated_comment_hides_fixture",
                "unit T;\ninterface\nuses DUnitX.TestFramework;\ntype\n{ never closed\n  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n".to_string(),
            ),
            // Class bodies outside the declared subset abstain that class only.
            (
                "abstain_field_without_semicolon",
                dunitx_unit("  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n    FX: Integer\n  end;\n"),
            ),
        ]);
        corpus
    }

    #[test]
    fn differential_branch_corpus_matches_the_legacy_parser() {
        let mut mismatches = Vec::new();
        for (name, source) in branch_corpus() {
            let (old, new) = both(&source);
            if old != new {
                mismatches.push(name);
            }
        }
        assert!(
            mismatches.is_empty(),
            "differing branch sources: {mismatches:?}"
        );
    }

    #[test]
    fn differential_committed_fixtures_match_the_legacy_parser() {
        fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).expect("read dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk(&path, found);
                } else if is_pascal_unit_path(&path.to_string_lossy()) {
                    found.push(path);
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/fixtures/delphi");
        let mut found = Vec::new();
        walk(&root, &mut found);
        assert!(!found.is_empty());
        for path in found {
            let text = std::fs::read_to_string(&path).expect("fixture");
            let (old, new) = both(&text);
            assert_eq!(old, new, "{}", path.display());
        }
    }

    #[test]
    fn branch_corpus_exercises_every_outcome_class() {
        // Guard against a corpus that compares empty reports on both sides.
        let mut anchored = 0;
        let mut abstained = 0;
        let mut conditional = 0;
        let mut unbound = 0;
        let mut degraded = 0;
        for (_, source) in branch_corpus() {
            let (_, new) = both(&source);
            anchored += usize::from(tests(&new) > 0);
            abstained += usize::from(!new.report.diagnostics.is_empty());
            let kinds = unknown_kinds(&new);
            conditional += usize::from(
                kinds.contains(&"declaration_under_conditional_compilation".to_string()),
            );
            unbound += usize::from(kinds.contains(&"dunitx_attribute_without_uses".to_string()));
            degraded += usize::from(new.report.diagnostics.len() > 1);
        }
        assert!(anchored > 20 && abstained > 5 && conditional > 5 && unbound >= 1);
        let _ = degraded;
    }

    /// `REPOGRAMMAR_DELPHI_DIFFERENTIAL_CORPUS=<dir>` compares the legacy parser
    /// with the Tree-sitter reader over every `.pas` file below `<dir>`.
    #[test]
    #[ignore = "reads an external corpus named by REPOGRAMMAR_DELPHI_DIFFERENTIAL_CORPUS"]
    fn differential_external_corpus() {
        fn walk(dir: &std::path::Path, found: &mut Vec<std::path::PathBuf>) {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, found);
                } else if is_pascal_unit_path(&path.to_string_lossy()) {
                    found.push(path);
                }
            }
        }
        let root = std::env::var("REPOGRAMMAR_DELPHI_DIFFERENTIAL_CORPUS").expect("corpus dir");
        let mut found = Vec::new();
        walk(std::path::Path::new(&root), &mut found);
        found.sort();
        let mut differing = 0;
        for path in &found {
            let Ok(text) = std::fs::read_to_string(path) else {
                println!("SKIP (not utf-8) {}", path.display());
                continue;
            };
            let old =
                std::panic::catch_unwind(|| parse_output_with(document(&text), legacy_reader));
            let new = parse_output_with(document(&text), pascal::parse_unit);
            match (old, new) {
                (Ok(Ok(old)), Ok(new)) if old == new => {}
                (Ok(Ok(old)), Ok(new)) => {
                    differing += 1;
                    println!(
                        "DIFF {} legacy(units={} facts={} diags={}) new(units={} facts={} diags={})",
                        path.display(),
                        old.report.units.len(),
                        old.report.semantic_facts.len(),
                        old.report.diagnostics.len(),
                        new.report.units.len(),
                        new.report.semantic_facts.len(),
                        new.report.diagnostics.len(),
                    );
                }
                (Err(_), _) => {
                    differing += 1;
                    println!("DIFF {} legacy panicked", path.display());
                }
                (_, new) => {
                    differing += 1;
                    println!("DIFF {} error {:?}", path.display(), new.err());
                }
            }
        }
        println!("compared {} files, {} differing", found.len(), differing);
    }

    #[test]
    fn a_syntax_error_after_the_class_refuses_the_file() {
        // Declared divergence `syntax_error_in_skipped_region`: the legacy
        // parser never read past `implementation`, so garbage there was
        // invisible to it. The grammar sees it, and any ERROR node outside a
        // class declaration refuses the file.
        let source = dunitx_unit(
            "  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n",
        )
        .replace("end.\n", "procedure ) ( ;;; begin begin\nend.\n");
        let parsed = output(&source);
        assert_eq!(tests(&parsed), 0);
        assert!(parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("outside the admitted subset")));
    }

    fn fixture_after(preamble: &str) -> String {
        dunitx_unit(&format!(
            "{preamble}  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n"
        ))
    }

    #[test]
    fn a_fixture_after_a_record_or_interface_is_anchored() {
        // The legacy parser skipped a non-class type to its first `;`, which sits
        // inside a record or interface body, and then lost the rest of the type
        // section. Declared as a legacy limitation, not reproduced.
        for preamble in [
            "  TRec = record\n    X: Integer;\n  end;\n",
            "  TRec = record\n  end;\n",
            "  IFoo = interface\n    procedure P;\n  end;\n",
        ] {
            assert_eq!(tests(&output(&fixture_after(preamble))), 1, "{preamble}");
        }
    }

    #[test]
    fn an_abstaining_class_does_not_hide_the_next_fixture() {
        // The last field may omit its `;` in Pascal, which is outside the
        // declared subset, so that class abstains. The legacy parser then lost
        // its place and also dropped the following fixture.
        let parsed = output(&fixture_after(
            "  TPlain = class\n    FX: Integer\n  end;\n",
        ));
        assert_eq!(tests(&parsed), 1);
        assert_eq!(parsed.report.diagnostics.len(), 1);
    }

    #[test]
    fn a_record_nested_in_a_fixture_does_not_hide_later_tests() {
        let parsed = output(&dunitx_unit(
            "  [TestFixture]\n  TA = class\n  public\n    type TR = record X: Integer; end;\n    [Test]\n    procedure A;\n  end;\n",
        ));
        assert_eq!(tests(&parsed), 1);
    }

    #[test]
    fn a_class_sealed_header_is_a_grammar_gap_that_abstains() {
        // tree-sitter-pascal 0.10.2 does not accept `class sealed`. The ERROR
        // node sits inside the class, so the class abstains with a diagnostic
        // instead of the legacy silent loss.
        let parsed = output(&dunitx_unit(
            "  [TestFixture]\n  TA = class sealed\n  public\n    [Test]\n    procedure A;\n  end;\n",
        ));
        assert_eq!(tests(&parsed), 0);
        assert_eq!(parsed.report.diagnostics.len(), 1);
    }

    #[test]
    fn an_ancestor_list_after_an_abstract_class_is_part_of_the_header() {
        let parsed = output(&dunitx_unit(
            "  [TestFixture]\n  TA = class abstract(TObject)\n  public\n    [Test]\n    procedure A;\n  end;\n",
        ));
        assert_eq!(tests(&parsed), 1);
    }

    #[test]
    fn a_uses_clause_with_an_in_file_clause_does_not_refuse_the_file() {
        // The grammar has no `in 'file'` production; the lexer reads the uses
        // clause, so the gap is not an error of the declarations.
        let parsed = output(&unit_with(
            "System.SysUtils in 'x.pas', DUnitX.TestFramework",
            "  [TestFixture]\n  TA = class\n  public\n    [Test]\n    procedure A;\n  end;\n",
        ));
        assert_eq!(tests(&parsed), 1);
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn an_attribute_before_a_visibility_keyword_abstains_the_class() {
        // Not valid Object Pascal: an attribute must precede a declaration.
        // The legacy parser skipped the dangling attribute silently.
        let parsed = output(&dunitx_unit(
            "  [TestFixture]\n  TA = class\n  public\n    [Test]\n  private\n    procedure A;\n  end;\n",
        ));
        assert_eq!(tests(&parsed), 0);
        assert_eq!(parsed.report.diagnostics.len(), 1);
    }
}
