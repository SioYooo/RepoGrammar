//! Bounded Ada AUnit frontend for the ADR-0045 admitted shape.
//!
//! Nothing here invokes GNAT, `gprbuild`, `gnattest`, Alire, Libadalang, a
//! child process, or the network. ADR-0033 D3's Libadalang `NO_GO` stands.
//!
//! The anchor is read off a parse, not off the text. [`super::lexer`] produces
//! an Ada token stream and [`super::syntax`] parses the whole compilation unit
//! against the declared subset; this module turns the admitted registration
//! calls into code units and facts, and turns every refusal into a typed
//! `UNKNOWN`.
//!
//! There is no third outcome. A file is parsed and its registrations are
//! reported, or it is refused and no registration is reported at all. Nothing
//! is recovered, because a recovered anchor cannot be told apart from a real
//! one.

use super::super::{ir_edges_for_units, ir_nodes_for_units};
use super::lexer::Refusal;
use super::syntax::parse_compilation;
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::file_discovery::DEFAULT_MAX_FILE_BYTES;
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParseOutput, SourceParser,
};

pub const ADA_ANCHOR_ENGINE: &str = "repogrammar-ada-aunit-scanner";
pub const ADA_ANCHOR_METHOD: &str = "bounded_ada_aunit_registration_v1";

/// Fixed support target for the one admitted exact anchor.
pub const ADA_TEST_TARGET: &str = "aunit.Register_Routine";

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
        return finish(units, facts, Vec::new());
    }

    let admitted = match parse_compilation(document.text) {
        Ok(admitted) => admitted,
        Err(refusal) => return refused(units, facts, module, document.path, refusal),
    };

    let mut unresolved_registration = false;

    if !admitted.aunit_context_clause {
        unresolved_registration = !admitted.registrations.is_empty();
    } else {
        for call in admitted.registrations {
            if units.len() >= MAX_UNITS {
                facts.push(unknown_fact(
                    &module,
                    UnknownReasonCode::InsufficientSupport,
                    "ada_registration_scan",
                    "scanner_resource_limit",
                    module.range.clone(),
                    "Ada frontend exceeded the bounded unit limit",
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

    // A file that parsed is not degraded: the parser consumed the whole
    // compilation unit or it would have refused it.
    finish(units, facts, Vec::new())
}

/// How a refusal reaches the operator.
///
/// Every field is fixed vocabulary. The offending token, identifier, literal,
/// and line never appear, because a refusal reaches `index --json`, `unknowns`,
/// and the MCP readiness payloads.
struct RefusalRecord {
    reason: UnknownReasonCode,
    affected_claim: &'static str,
    kind: &'static str,
    note: &'static str,
    /// A degraded parse is reported only when the file claims to be Ada and is
    /// malformed, or when a build variant selects between texts. An ordinary
    /// construct outside the declared subset is a bounded-frontend limit, not a
    /// broken file, so it must not raise an operator warning on every real Ada
    /// repository.
    degraded: Option<&'static str>,
}

fn refusal_record(refusal: Refusal) -> RefusalRecord {
    match refusal {
        Refusal::PreprocessorDirective => RefusalRecord {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "ada_conditional_compilation",
            kind: "gnatprep_conditional_source",
            note: "the file carries a gnatprep conditional directive, so it is preprocessor input whose selected branch is unevaluated",
            degraded: Some(
                "an Ada source file carries gnatprep conditional directives, so the compiled text is selected by symbols this frontend does not evaluate",
            ),
        },
        Refusal::PreprocessorSubstitution => RefusalRecord {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "ada_conditional_compilation",
            kind: "gnatprep_symbol_substitution",
            note: "the file carries a gnatprep symbol substitution, so its compiled text is unevaluated",
            degraded: Some(
                "an Ada source file carries a gnatprep symbol substitution, so its compiled text is selected by symbols this frontend does not evaluate",
            ),
        },
        Refusal::LanguageEditionPragma => RefusalRecord {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "ada_language_edition",
            kind: "language_edition_pragma",
            note: "a configuration pragma re-selects the Ada edition or enables non-standard syntax, which the declared invariance set assumes fixed",
            degraded: Some(
                "an Ada configuration pragma re-selects the language edition, which this frontend does not evaluate",
            ),
        },
        Refusal::EditionSensitiveWord => RefusalRecord {
            reason: UnknownReasonCode::BuildVariantAmbiguity,
            affected_claim: "ada_language_edition",
            kind: "edition_sensitive_reserved_word",
            note: "a word that is reserved in some editions of the declared set and an identifier in others appears outside the one position where both readings agree",
            degraded: Some(
                "an Ada source file uses a word whose reserved status depends on the language edition, which this frontend does not select",
            ),
        },
        Refusal::UnterminatedLiteral => RefusalRecord {
            reason: UnknownReasonCode::InsufficientSupport,
            affected_claim: "ada_registration_scan",
            kind: "unterminated_literal",
            note: "an Ada literal is left open, so the token stream after it is unproven",
            degraded: Some(
                "an Ada literal is left open, so every registration after it was read as literal text",
            ),
        },
        Refusal::ReplacementCharacter => RefusalRecord {
            reason: UnknownReasonCode::InsufficientSupport,
            affected_claim: "ada_registration_scan",
            kind: "obsolescent_replacement_character",
            note: "an obsolescent Ada RM J.2 replacement character moves literal boundaries, so the token stream is unproven",
            degraded: None,
        },
        Refusal::UnsupportedCharacter => RefusalRecord {
            reason: UnknownReasonCode::InsufficientSupport,
            affected_claim: "ada_registration_scan",
            kind: "character_outside_declared_subset",
            note: "the file uses a lexical element outside the declared Ada subset",
            degraded: None,
        },
        Refusal::OutsideDeclaredSubset => RefusalRecord {
            reason: UnknownReasonCode::InsufficientSupport,
            affected_claim: "ada_registration_scan",
            kind: "construct_outside_declared_subset",
            note: "the compilation unit uses a construct outside the declared Ada subset, so it was not parsed and its registrations are unproven",
            degraded: None,
        },
        Refusal::NestingLimit => RefusalRecord {
            reason: UnknownReasonCode::InsufficientSupport,
            affected_claim: "ada_registration_scan",
            kind: "parser_nesting_limit",
            note: "the compilation unit nests beyond the bounded parser ceiling",
            degraded: None,
        },
    }
}

fn refused(
    units: Vec<CodeUnit>,
    mut facts: Vec<SemanticFact>,
    module: CodeUnit,
    path: &str,
    refusal: Refusal,
) -> Result<SourceParseOutput, ParseError> {
    let record = refusal_record(refusal);
    facts.push(unknown_fact(
        &module,
        record.reason,
        record.affected_claim,
        record.kind,
        module.range.clone(),
        record.note,
    )?);
    let diagnostics = record
        .degraded
        .map(|message| vec![degraded(path, message)])
        .unwrap_or_default();
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
            diagnostics,
        },
        python_interface_hash: None,
        dependencies: Vec::new(),
    })
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

    fn is_degraded(parsed: &SourceParseOutput) -> bool {
        parsed
            .report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.severity == ParseDiagnosticSeverity::Error)
    }

    const HEAD: &str = "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
                        procedure Register_Tests (T : in out Test_Case) is\nbegin\n";
    const TAIL: &str = "end Register_Tests;\nend Catalog_Tests;\n";

    fn body(statements: &str) -> String {
        format!("{HEAD}{statements}{TAIL}")
    }

    #[test]
    fn admitted_registrations_anchor_including_multi_line_and_prefixed_calls() {
        let parsed = output(&body(
            "Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
             Registration.Register_Routine\n  (T, Filters_Catalog'Access, \"filters\");\n\
             AUnit.Test_Cases.Registration.Register_Routine (T, Sorts'Access, \"sorts\");\n",
        ));
        assert_eq!(registrations(&parsed), 3);
        assert!(parsed.report.diagnostics.is_empty());
        assert!(unknown_kinds(&parsed).is_empty());
    }

    #[test]
    fn without_an_aunit_with_clause_nothing_anchors() {
        let parsed = output(
            "with Ada.Text_IO;\npackage body Catalog_Tests is\n\
             procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
             Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
             end Register_Tests;\nend Catalog_Tests;\n",
        );
        assert_eq!(registrations(&parsed), 0);
        assert!(
            unknown_kinds(&parsed).contains(&"registration_without_aunit_with_clause".to_string())
        );
    }

    #[test]
    fn a_registration_in_a_comment_or_a_string_never_anchors() {
        let parsed = output(&body(
            "--  Register_Routine (T, In_A_Comment'Access, \"no\");\n\
             Note := \"Register_Routine (T, In_A_String'Access, \"\"no\"\")\";\n",
        ));
        assert_eq!(registrations(&parsed), 0);
    }

    #[test]
    fn a_quote_character_literal_does_not_swallow_the_rest_of_the_file() {
        // `'''` is the shape that breaks a lexer searching for a closing tick.
        let parsed = output(
            "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
             Tick : constant Character := ''';\n\
             Paren : constant Character := '(';\n\
             procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
             Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
             Register_Routine (T, Filters'Access, \"filters\");\n\
             end Register_Tests;\nend Catalog_Tests;\n",
        );
        assert_eq!(registrations(&parsed), 2);
    }

    #[test]
    fn an_attribute_tick_after_an_identifier_or_a_paren_is_not_a_character_literal() {
        let parsed = output(&body(
            "Size := Items (I)'Length + Integer'First;\n\
             Address := Ptr.all'Address;\n\
             Register_Routine (T, Loads_Catalog'Access, \"loads\");\n",
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
            "Register_Routine (T, Loads_Catalog'Access);",
            "Register_Routine (T, Routine_Ptr, \"loads\");",
            "Register_Routine (T, Loads_Catalog'Access, Description);",
            "Register_Routine (T, Loads_Catalog'Address, \"loads\");",
            "Register_Routine (T, Loads_Catalog'Access, \"loads\", Extra);",
            "Register_Routine (T, Loads_Catalog'Access, \"a\" & \"b\");",
            "My_Register_Routine (T, Loads_Catalog'Access, \"loads\");",
        ] {
            let parsed = output(&body(&format!("{call}\n")));
            assert_eq!(registrations(&parsed), 0, "{call}");
            assert!(
                unknown_kinds(&parsed).is_empty(),
                "an unadmitted call shape is not an unknown: {call}"
            );
        }
    }

    #[test]
    fn a_nested_call_in_the_first_argument_does_not_split_the_arguments() {
        let parsed = output(&body(
            "Register_Routine (Fixture (T, 1), Loads_Catalog'Access, \"loads\");\n",
        ));
        assert_eq!(registrations(&parsed), 1);
    }

    #[test]
    fn case_insensitivity_follows_the_language() {
        let parsed = output(
            "WITH Aunit.Test_Cases;\nPACKAGE BODY Catalog_Tests IS\n\
             PROCEDURE Register_Tests (T : IN OUT Test_Case) IS\nBEGIN\n\
             REGISTER_ROUTINE (T, Loads_Catalog'ACCESS, \"loads\");\n\
             END Register_Tests;\nEND Catalog_Tests;\n",
        );
        assert_eq!(registrations(&parsed), 1);
    }

    #[test]
    fn every_frontend_fact_stays_below_family_supporting_certainty() {
        let parsed = output(&body(
            "Register_Routine (T, Loads_Catalog'Access, \"loads\");\n",
        ));
        assert!(parsed
            .report
            .semantic_facts
            .iter()
            .all(|fact| !fact.certainty.supports_family_membership()));
    }

    #[test]
    fn an_unterminated_string_reports_a_degraded_parse_and_abstains() {
        // A scanner kept the anchors it had already found. A parser cannot:
        // the token stream after an open literal is unproven, so the file
        // abstains and says so.
        let parsed = output(
            "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
             procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
             Register_Routine (T, Loads_Catalog'Access, \"loads\");\n\
             Note := \"never closed\nend Register_Tests;\nend Catalog_Tests;\n",
        );
        assert!(is_degraded(&parsed));
        assert_eq!(registrations(&parsed), 0);
        assert!(unknown_kinds(&parsed).contains(&"unterminated_literal".to_string()));
    }

    #[test]
    fn a_well_formed_body_reports_no_diagnostic() {
        let parsed = output(&body(
            "Register_Routine (T, Loads_Catalog'Access, \"loads\");\n",
        ));
        assert!(parsed.report.diagnostics.is_empty());
    }

    #[test]
    fn a_build_variant_selects_the_text_so_the_file_abstains_and_reports_it() {
        // Each row is a construct whose compiled text or grammar RepoGrammar
        // cannot determine. None of them may anchor, and each records a typed
        // `UNKNOWN` under a claim that names what it affects.
        for (source, kind) in [
            (
                "#if DEBUG\nwith AUnit.Test_Cases;\n#end if;\npackage body Catalog_Tests is\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n",
                "gnatprep_conditional_source",
            ),
            (
                &body("Name := $Build_Name;\nRegister_Routine (T, Loads'Access, \"loads\");\n"),
                "gnatprep_symbol_substitution",
            ),
            (
                &format!(
                    "pragma Ada_83;\n{}",
                    body("Register_Routine (T, Loads'Access, \"loads\");\n")
                ),
                "language_edition_pragma",
            ),
            (
                "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
                 Interface : Boolean := False;\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n",
                "edition_sensitive_reserved_word",
            ),
        ] {
            let parsed = output(source);
            assert_eq!(registrations(&parsed), 0, "{kind}");
            assert!(unknown_kinds(&parsed).contains(&kind.to_string()), "{kind}");
            assert!(is_degraded(&parsed), "{kind}");
        }
    }

    #[test]
    fn a_construct_outside_the_subset_abstains_without_warning_the_operator() {
        // Most real Ada is outside any bounded subset. That is a limit of this
        // frontend, not a broken file, so it must not raise a degraded-parse
        // warning on every Ada repository.
        for (source, kind) in [
            (
                "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
                 task body Worker is\nbegin\n   null;\nend Worker;\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n",
                "construct_outside_declared_subset",
            ),
            (
                &body("X := @ + 1;\nRegister_Routine (T, Loads'Access, \"loads\");\n"),
                "character_outside_declared_subset",
            ),
            (
                &body("X := 16:FF:;\nRegister_Routine (T, Loads'Access, \"loads\");\n"),
                "obsolescent_replacement_character",
            ),
        ] {
            let parsed = output(source);
            assert_eq!(registrations(&parsed), 0, "{kind}");
            assert!(unknown_kinds(&parsed).contains(&kind.to_string()), "{kind}");
            assert!(!is_degraded(&parsed), "{kind}");
        }
    }

    #[test]
    fn no_refusal_surface_carries_source_text() {
        // A parser naturally wants to name the token it choked on. None of
        // these surfaces may, because they reach `index --json`, `unknowns`,
        // and the MCP readiness payloads.
        let secrets = [
            "Secret_Identifier",
            "s3cr3t-literal",
            "Build_Name",
            "Worker",
            "@",
        ];
        for source in [
            body("Secret_Identifier := $Build_Name;\n"),
            body("Secret_Identifier := @ + 1;\n"),
            body("Note := \"s3cr3t-literal\nX := 1;\n"),
            "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
             task body Worker is\nbegin\n   null;\nend Worker;\n\
             procedure Secret_Identifier (T : in out Test_Case) is\nbegin\n   null;\n\
             end Secret_Identifier;\nend Catalog_Tests;\n"
                .to_string(),
        ] {
            let parsed = output(&source);
            let mut surfaces = parsed
                .report
                .diagnostics
                .iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>();
            for fact in &parsed.report.semantic_facts {
                surfaces.push(fact.evidence.note.clone());
                surfaces.extend(fact.assumptions.iter().cloned());
            }
            for surface in surfaces {
                for secret in secrets {
                    assert!(
                        !surface.contains(secret),
                        "refusal surface leaked source text: {surface}"
                    );
                }
            }
        }
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
