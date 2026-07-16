//! Tree-sitter-backed structural C/C++ code-unit extraction (bounded preview).
//!
//! This adapter parses `.c`/`.h` with the C grammar and the C++ extensions with
//! the C++ grammar. It never expands macros, never evaluates the preprocessor,
//! never runs a build, and never generates moc/protoc/flatc output. It emits
//! structural units, exact include-evidence-gated GoogleTest/Catch2/doctest/
//! Boost.Test registration-macro anchors only after bounded argument-contract
//! and Boost suite-state validation, and typed UNKNOWN facts for every
//! macro-boundary, build-variant, generated-code, or dispatch semantic that
//! remains unresolved. `compile_commands.json`, `vcpkg.json`, and
//! `conanfile.txt` are parsed as structural `PROJECT_CONFIG` inventory only,
//! never as family support.

mod preprocessor;
mod project_config;
mod test_framework;

use self::test_framework::{MacroShape, TestMacroKind};
use super::{ir_edges_for_units, ir_nodes_for_units};
use crate::core::model::{
    CodeUnit, CodeUnitId, CodeUnitKind, Evidence, FactCertainty, FactOrigin, Language, Provenance,
    SemanticFact, SemanticFactKind, SourceRange, SymbolId, UnknownReasonCode,
};
use crate::ports::parser::{
    ParseDiagnostic, ParseDiagnosticSeverity, ParseError, ParseReport, ParserProjectContext,
    SourceDocument, SourceParser,
};
use std::collections::BTreeSet;
use tree_sitter::{Node, Parser};

pub(crate) const CPP_ANCHOR_ENGINE: &str = "repogrammar-cpp-syntax";
pub(crate) const CPP_ANCHOR_METHOD: &str = "tree_sitter_c_cpp_structural_anchors_v1";

#[derive(Debug, Default)]
pub struct CppSyntaxParser;

impl SourceParser for CppSyntaxParser {
    fn parse(&self, document: SourceDocument<'_>) -> Result<ParseReport, ParseError> {
        self.parse_with_context(document, &ParserProjectContext::default())
    }

    fn parse_with_context(
        &self,
        document: SourceDocument<'_>,
        _context: &ParserProjectContext,
    ) -> Result<ParseReport, ParseError> {
        match document.language {
            Language::CppConfig => return project_config::parse(document),
            Language::C | Language::Cpp => {}
            _ => return Err(ParseError::UnsupportedLanguage),
        }

        let mut parser = Parser::new();
        let language = if document.language == Language::C {
            tree_sitter_c::LANGUAGE.into()
        } else {
            tree_sitter_cpp::LANGUAGE.into()
        };
        parser
            .set_language(&language)
            .map_err(|error| ParseError::Internal(format!("load C/C++ grammar: {error}")))?;
        let Some(tree) = parser.parse(document.text, None) else {
            return Err(ParseError::Internal(
                "Tree-sitter C/C++ parse failed".to_string(),
            ));
        };

        let root = tree.root_node();
        let mut scanner = CppTreeScanner::new(document, root);
        scanner.scan_tree(root)?;
        scanner.finish()
    }
}

struct CppTreeScanner<'a> {
    document: SourceDocument<'a>,
    language: Language,
    includes: preprocessor::IncludeEvidence,
    units: Vec<CodeUnit>,
    semantic_facts: Vec<SemanticFact>,
    diagnostics: Vec<ParseDiagnostic>,
    ordinal: usize,
    module_unit: Option<CodeUnit>,
    conditional_regions: Vec<preprocessor::ConditionalRegion>,
    boost_suite_analysis: test_framework::Analysis,
    emitted_module_unknowns: BTreeSet<&'static str>,
}

impl<'a> CppTreeScanner<'a> {
    fn new(document: SourceDocument<'a>, root: Node<'_>) -> Self {
        let preprocessor = preprocessor::analyze(document.text, root);
        let boost_suite_analysis = test_framework::analyze(document.text, root);
        let language = document.language.clone();
        Self {
            document,
            language,
            includes: preprocessor.includes,
            units: Vec::new(),
            semantic_facts: Vec::new(),
            diagnostics: Vec::new(),
            ordinal: 0,
            module_unit: None,
            conditional_regions: preprocessor.conditional_regions,
            boost_suite_analysis,
            emitted_module_unknowns: BTreeSet::new(),
        }
    }

    fn scan_tree(&mut self, root: Node<'_>) -> Result<(), ParseError> {
        let module_unit =
            self.add_unit(CodeUnitKind::Module, "file", 0, self.document.text.len())?;
        self.module_unit = Some(module_unit);
        if root.has_error() {
            self.diagnostics.push(ParseDiagnostic {
                path: self.document.path.to_string(),
                range: None,
                severity: ParseDiagnosticSeverity::Warning,
                message: "Tree-sitter C/C++ parse contains syntax errors; extraction is structural"
                    .to_string(),
            });
        }
        self.scan_scope(root)?;
        self.emit_signal_slot_context()?;
        Ok(())
    }

    fn scan_scope(&mut self, node: Node<'_>) -> Result<(), ParseError> {
        let mut cursor = node.walk();
        let children = node.named_children(&mut cursor).collect::<Vec<_>>();
        let mut index = 0;
        while index < children.len() {
            let child = children[index];
            match child.kind() {
                "function_definition" => {
                    self.scan_function_definition(child)?;
                }
                "expression_statement" => {
                    let next = children.get(index + 1).copied();
                    if self.scan_expression_statement(child, next)? {
                        index += 1;
                    }
                }
                "class_specifier" | "struct_specifier" => {
                    self.scan_class(child)?;
                }
                "namespace_definition"
                | "linkage_specification"
                | "declaration_list"
                | "preproc_ifdef"
                | "preproc_if"
                | "preproc_else"
                | "preproc_elif" => {
                    self.scan_scope(child)?;
                }
                _ => {}
            }
            index += 1;
        }
        Ok(())
    }

    fn scan_function_definition(&mut self, node: Node<'_>) -> Result<(), ParseError> {
        // A macro-call declarator shape has no return type field.
        if node.child_by_field_name("type").is_some() {
            // A real function definition; record a structural unit only.
            let name = declarator_identifier(self.document.text, node).unwrap_or("function");
            self.add_unit(
                CodeUnitKind::Function,
                name,
                node.start_byte(),
                node.end_byte(),
            )?;
            return Ok(());
        }
        let Some((name, shape)) = test_framework::function_macro_shape(self.document.text, node)
        else {
            return Ok(());
        };
        let has_error = preprocessor::contains_error_node(node);
        self.handle_macro_shape(name, &shape, has_error, node.start_byte(), node.end_byte())
    }

    fn scan_expression_statement(
        &mut self,
        node: Node<'_>,
        next_sibling: Option<Node<'_>>,
    ) -> Result<bool, ParseError> {
        let Some(call) = first_named_child_of_kind(node, "call_expression") else {
            return Ok(false);
        };
        let Some(name) = call
            .child_by_field_name("function")
            .filter(|function| function.kind() == "identifier")
            .and_then(|function| node_text_checked(self.document.text, function))
        else {
            return Ok(false);
        };
        let shape = test_framework::call_macro_shape(self.document.text, call);
        // A brace body may parse as a following sibling `compound_statement`.
        let (end_byte, consumed_body, body) = match next_sibling {
            Some(sibling) if sibling.kind() == "compound_statement" => {
                (sibling.end_byte(), true, Some(sibling))
            }
            _ => (node.end_byte(), false, None),
        };
        let has_error = preprocessor::contains_error_node(call)
            || body.is_some_and(preprocessor::contains_error_node);
        self.handle_macro_shape(name, &shape, has_error, node.start_byte(), end_byte)?;
        Ok(consumed_body)
    }

    /// Shared handling for both parse shapes (function-definition-with-macro
    /// declarator and call-expression statement).
    fn handle_macro_shape(
        &mut self,
        macro_name: &str,
        shape: &MacroShape,
        region_has_error: bool,
        region_start: usize,
        region_end: usize,
    ) -> Result<(), ParseError> {
        let Some(macro_kind) = test_framework::classify(macro_name) else {
            if preprocessor::is_all_caps_macro(macro_name) {
                self.emit_user_macro_context()?;
            }
            return Ok(());
        };

        // A registration region containing an ERROR node is parse-degraded; never
        // anchor it, emit a blocking macro-boundary UNKNOWN instead.
        if region_has_error {
            let unit =
                self.add_unit(CodeUnitKind::Function, macro_name, region_start, region_end)?;
            self.push_unknown(
                &unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "cpp_macro_boundary",
                "parse_degraded_macro_region",
                "C/C++ registration macro region contains a Tree-sitter ERROR node and is not anchored",
                Vec::new(),
            )?;
            return Ok(());
        }

        if !test_framework::contract_is_supported(
            &macro_kind,
            shape,
            self.includes.catch2,
            self.includes.doctest,
        ) {
            let unit =
                self.add_unit(CodeUnitKind::Function, macro_name, region_start, region_end)?;
            self.push_unknown(
                &unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "cpp_test_framework_identity",
                "unsupported_test_macro_contract",
                "C/C++ test registration macro arguments fall outside the audited bounded framework contract",
                vec![
                    format!("test_macro={macro_name}"),
                    format!("test_macro_arity={}", shape.argument_count()),
                ],
            )?;
            self.maybe_emit_build_variant_unknown(&unit)?;
            return Ok(());
        }

        if let Some(issue) = self.boost_suite_analysis.take_issue_at(region_start) {
            let unit =
                self.add_unit(CodeUnitKind::Function, macro_name, region_start, region_end)?;
            self.push_unknown(
                &unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "cpp_test_framework_identity",
                issue.kind,
                issue.note,
                vec![format!("test_macro={macro_name}")],
            )?;
            self.maybe_emit_build_variant_unknown(&unit)?;
            return Ok(());
        }

        if matches!(macro_kind, TestMacroKind::BoostSuiteEnd) {
            return Ok(());
        }

        let name_shape = if shape.first_is_string_literal() {
            "string_literal"
        } else if shape.argument_count() >= 2 {
            "identifier_pair"
        } else {
            "identifier"
        };

        match self.resolve_test_framework(&macro_kind) {
            TestOutcome::Anchor(spec) => {
                let unit =
                    self.add_unit(spec.unit_kind.clone(), macro_name, region_start, region_end)?;
                let mut assumptions = vec![
                    "provider_resolved=false".to_string(),
                    format!("cpp_anchor_kind={}", spec.unit_kind.as_str()),
                    format!("test_framework={}", spec.framework),
                    format!("test_macro={}", macro_name),
                ];
                if spec.record_name_shape {
                    assumptions.push(format!("test_name_shape={name_shape}"));
                }
                if let Some(fixture_shape) = spec.fixture_shape {
                    assumptions.push(format!("fixture_shape={fixture_shape}"));
                }
                if let Some(suite_shape) = spec.suite_shape {
                    assumptions.push(format!("suite_shape={suite_shape}"));
                }
                self.push_anchor(
                    &unit,
                    SemanticFactKind::ResolvedCall,
                    spec.target,
                    assumptions,
                    "bounded C/C++ test registration macro anchor",
                )?;
                self.maybe_emit_build_variant_unknown(&unit)?;
                self.maybe_emit_indirect_dispatch(&unit, region_start, region_end)?;
            }
            TestOutcome::Blocked { reason, kind, note } => {
                let unit =
                    self.add_unit(CodeUnitKind::Function, macro_name, region_start, region_end)?;
                self.push_unknown(
                    &unit,
                    reason,
                    "cpp_test_framework_identity",
                    kind,
                    note,
                    Vec::new(),
                )?;
                self.maybe_emit_build_variant_unknown(&unit)?;
            }
        }
        Ok(())
    }

    fn resolve_test_framework(&self, macro_kind: &TestMacroKind) -> TestOutcome {
        match macro_kind {
            TestMacroKind::Gtest {
                target, fixture, ..
            } => {
                if self.includes.gtest {
                    TestOutcome::Anchor(AnchorSpec {
                        unit_kind: CodeUnitKind::GtestTestCase,
                        target,
                        framework: "gtest",
                        record_name_shape: true,
                        fixture_shape: Some(if *fixture { "fixture" } else { "free" }),
                        suite_shape: None,
                    })
                } else {
                    TestOutcome::Blocked {
                        reason: UnknownReasonCode::UnresolvedImport,
                        kind: "gtest_macro_without_include",
                        note: "GoogleTest macro lacks a gtest/gmock include corroboration",
                    }
                }
            }
            TestMacroKind::TestCase => match (self.includes.catch2, self.includes.doctest) {
                (true, false) => TestOutcome::Anchor(AnchorSpec {
                    unit_kind: CodeUnitKind::Catch2TestCase,
                    target: "catch2.TEST_CASE",
                    framework: "catch2",
                    record_name_shape: true,
                    fixture_shape: None,
                    suite_shape: None,
                }),
                (false, true) => TestOutcome::Anchor(AnchorSpec {
                    unit_kind: CodeUnitKind::DoctestTestCase,
                    target: "doctest.TEST_CASE",
                    framework: "doctest",
                    record_name_shape: true,
                    fixture_shape: None,
                    suite_shape: None,
                }),
                (true, true) => TestOutcome::Blocked {
                    reason: UnknownReasonCode::ConflictingFacts,
                    kind: "test_case_catch2_and_doctest",
                    note: "TEST_CASE include evidence matches both Catch2 and doctest",
                },
                (false, false) => TestOutcome::Blocked {
                    reason: UnknownReasonCode::UnresolvedImport,
                    kind: "test_case_without_include",
                    note: "TEST_CASE lacks Catch2 or doctest include corroboration",
                },
            },
            TestMacroKind::Scenario => {
                if self.includes.catch2 {
                    TestOutcome::Anchor(AnchorSpec {
                        unit_kind: CodeUnitKind::Catch2TestCase,
                        target: "catch2.SCENARIO",
                        framework: "catch2",
                        record_name_shape: true,
                        fixture_shape: None,
                        suite_shape: None,
                    })
                } else {
                    TestOutcome::Blocked {
                        reason: UnknownReasonCode::UnresolvedImport,
                        kind: "scenario_without_catch2",
                        note: "SCENARIO lacks a Catch2 include corroboration",
                    }
                }
            }
            kind @ (TestMacroKind::BoostAutoCase
            | TestMacroKind::BoostFixtureCase
            | TestMacroKind::BoostSuite) => {
                let (target, unit_kind, fixture, suite) = match kind {
                    TestMacroKind::BoostAutoCase => (
                        "boost_test.BOOST_AUTO_TEST_CASE",
                        CodeUnitKind::BoostTestCase,
                        false,
                        false,
                    ),
                    TestMacroKind::BoostFixtureCase => (
                        "boost_test.BOOST_FIXTURE_TEST_CASE",
                        CodeUnitKind::BoostTestCase,
                        true,
                        false,
                    ),
                    TestMacroKind::BoostSuite => (
                        "boost_test.BOOST_AUTO_TEST_SUITE",
                        CodeUnitKind::BoostTestSuite,
                        false,
                        true,
                    ),
                    _ => unreachable!("matched Boost registration kind"),
                };
                if self.includes.boost_test {
                    TestOutcome::Anchor(AnchorSpec {
                        unit_kind,
                        target,
                        framework: "boost_test",
                        record_name_shape: false,
                        fixture_shape: fixture.then_some("fixture"),
                        suite_shape: suite.then_some("auto"),
                    })
                } else {
                    TestOutcome::Blocked {
                        reason: UnknownReasonCode::UnresolvedImport,
                        kind: "boost_macro_without_include",
                        note: "Boost.Test macro lacks a boost/test include corroboration",
                    }
                }
            }
            TestMacroKind::BoostSuiteEnd => {
                unreachable!("validated Boost suite terminators are handled before resolution")
            }
        }
    }

    fn scan_class(&mut self, node: Node<'_>) -> Result<(), ParseError> {
        let name = node
            .child_by_field_name("name")
            .and_then(|child| node_text_checked(self.document.text, child))
            .or_else(|| first_type_identifier(self.document.text, node))
            .unwrap_or("type");
        let bases = base_class_names(self.document.text, node);
        let is_gtest_fixture = bases
            .iter()
            .any(|base| base == "::testing::Test" || base == "testing::Test");
        let body = node.child_by_field_name("body");
        let has_q_object = body
            .and_then(|body| node_text_checked(self.document.text, body))
            .is_some_and(class_body_has_q_object);

        if is_gtest_fixture && self.includes.gtest {
            let unit = self.add_unit(
                CodeUnitKind::GtestTestFixture,
                name,
                node.start_byte(),
                node.end_byte(),
            )?;
            self.push_anchor(
                &unit,
                SemanticFactKind::Type,
                "gtest.testing.Test",
                vec![
                    "provider_resolved=false".to_string(),
                    "cpp_anchor_kind=gtest_test_fixture".to_string(),
                    "test_framework=gtest".to_string(),
                    "fixture_shape=fixture".to_string(),
                ],
                "bounded C/C++ GoogleTest fixture base anchor",
            )?;
            self.maybe_emit_build_variant_unknown(&unit)?;
        } else if is_gtest_fixture {
            let unit = self.add_unit(
                CodeUnitKind::Class,
                name,
                node.start_byte(),
                node.end_byte(),
            )?;
            self.push_unknown(
                &unit,
                UnknownReasonCode::UnresolvedImport,
                "cpp_test_framework_identity",
                "gtest_fixture_without_include",
                "GoogleTest fixture base shape lacks an exact unconditional gtest/gmock include corroboration",
                Vec::new(),
            )?;
            self.maybe_emit_build_variant_unknown(&unit)?;
        } else if has_q_object {
            let unit = self.add_unit(
                CodeUnitKind::QtObjectClass,
                name,
                node.start_byte(),
                node.end_byte(),
            )?;
            self.push_unknown(
                &unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "cpp_generated_code",
                "qt_moc_metaobject_absent",
                "Qt Q_OBJECT metaobject (moc-generated) output is not generated or simulated",
                Vec::new(),
            )?;
        } else {
            self.add_unit(
                CodeUnitKind::Class,
                name,
                node.start_byte(),
                node.end_byte(),
            )?;
        }
        Ok(())
    }

    fn emit_signal_slot_context(&mut self) -> Result<(), ParseError> {
        if self.document.text.contains("SIGNAL(")
            && self.document.text.contains("SLOT(")
            && self.emitted_module_unknowns.insert("signal_slot")
        {
            let module_unit = self.module_unit.clone().expect("module unit exists");
            self.push_unknown(
                &module_unit,
                UnknownReasonCode::FrameworkMagic,
                "cpp_signal_slot_string_dispatch",
                "qt_signal_slot_string_dispatch",
                "Qt string-based SIGNAL/SLOT connect endpoints are resolved by the runtime metaobject",
                Vec::new(),
            )?;
        }
        Ok(())
    }

    fn emit_user_macro_context(&mut self) -> Result<(), ParseError> {
        if self.emitted_module_unknowns.insert("user_macro") {
            let module_unit = self.module_unit.clone().expect("module unit exists");
            self.push_unknown(
                &module_unit,
                UnknownReasonCode::MacroOrPreprocessor,
                "cpp_macro_boundary",
                "user_macro_registration",
                "Top-level user macro invocation may expand to registrations that are not evaluated",
                Vec::new(),
            )?;
        }
        Ok(())
    }

    fn maybe_emit_indirect_dispatch(
        &mut self,
        unit: &CodeUnit,
        start: usize,
        end: usize,
    ) -> Result<(), ParseError> {
        let slice = self.document.text.get(start..end).unwrap_or("");
        if slice.contains("G_CALLBACK(") {
            self.push_unknown(
                unit,
                UnknownReasonCode::FrameworkMagic,
                "cpp_indirect_dispatch",
                "function_pointer_callback_registration",
                "C/C++ function-pointer callback dispatch is not resolved without points-to analysis",
                Vec::new(),
            )?;
        }
        Ok(())
    }

    fn maybe_emit_build_variant_unknown(&mut self, unit: &CodeUnit) -> Result<(), ParseError> {
        let Some(region) = preprocessor::first_overlapping_region(
            &self.conditional_regions,
            unit.range.start_byte,
            unit.range.end_byte,
        ) else {
            return Ok(());
        };
        self.push_unknown(
            unit,
            UnknownReasonCode::BuildVariantAmbiguity,
            "cpp_build_variant",
            "conditional_compilation_region",
            "C/C++ preprocessor conditional selects a build variant that is never evaluated",
            vec![format!("cpp_variant_condition_shape={}", region.shape())],
        )?;
        Ok(())
    }

    fn add_unit(
        &mut self,
        kind: CodeUnitKind,
        name: &str,
        start_byte: usize,
        end_byte: usize,
    ) -> Result<CodeUnit, ParseError> {
        let range = SourceRange::new(start_byte, end_byte).map_err(ParseError::Internal)?;
        let provenance = Provenance::new(
            self.document.path,
            self.document.content_hash.clone(),
            self.document.repository_revision.clone(),
        )
        .map_err(ParseError::Internal)?;
        let id = CodeUnitId::new(format!(
            "unit:{}#{}:{}-{}:{}",
            self.document.path,
            kind.as_str(),
            slug(name),
            start_byte,
            self.ordinal
        ))
        .map_err(ParseError::Internal)?;
        self.ordinal += 1;
        let unit = CodeUnit {
            id,
            language: self.language.clone(),
            kind,
            range,
            provenance,
        };
        self.units.push(unit.clone());
        Ok(unit)
    }

    fn push_anchor(
        &mut self,
        unit: &CodeUnit,
        kind: SemanticFactKind,
        target: &str,
        assumptions: Vec<String>,
        note: &str,
    ) -> Result<(), ParseError> {
        let fact = structural_anchor_fact(&self.document, unit, kind, target, assumptions, note)?;
        self.semantic_facts.push(fact);
        Ok(())
    }

    fn push_unknown(
        &mut self,
        unit: &CodeUnit,
        reason: UnknownReasonCode,
        affected_claim: &str,
        kind: &str,
        note: &str,
        extra_assumptions: Vec<String>,
    ) -> Result<(), ParseError> {
        let fact = unknown_fact(
            &self.document,
            unit,
            reason,
            affected_claim,
            kind,
            note,
            extra_assumptions,
        )?;
        self.semantic_facts.push(fact);
        Ok(())
    }

    fn finish(mut self) -> Result<ParseReport, ParseError> {
        self.units.sort_by(|left, right| {
            (
                left.range.start_byte,
                left.range.end_byte,
                left.kind.as_str(),
                left.id.as_str(),
            )
                .cmp(&(
                    right.range.start_byte,
                    right.range.end_byte,
                    right.kind.as_str(),
                    right.id.as_str(),
                ))
        });
        self.semantic_facts.sort_by(|left, right| {
            (
                left.evidence.range.start_byte,
                left.evidence.range.end_byte,
                left.kind.as_protocol_str(),
                left.subject.as_str(),
                left.target.as_ref().map(SymbolId::as_str),
            )
                .cmp(&(
                    right.evidence.range.start_byte,
                    right.evidence.range.end_byte,
                    right.kind.as_protocol_str(),
                    right.subject.as_str(),
                    right.target.as_ref().map(SymbolId::as_str),
                ))
        });
        let ir_nodes = ir_nodes_for_units(&self.units).map_err(ParseError::Internal)?;
        let ir_edges = ir_edges_for_units(&self.units).map_err(ParseError::Internal)?;
        Ok(ParseReport {
            units: self.units,
            ir_nodes,
            ir_edges,
            semantic_facts: self.semantic_facts,
            diagnostics: self.diagnostics,
        })
    }
}

struct AnchorSpec {
    unit_kind: CodeUnitKind,
    target: &'static str,
    framework: &'static str,
    record_name_shape: bool,
    fixture_shape: Option<&'static str>,
    suite_shape: Option<&'static str>,
}

enum TestOutcome {
    Anchor(AnchorSpec),
    Blocked {
        reason: UnknownReasonCode,
        kind: &'static str,
        note: &'static str,
    },
}

fn structural_anchor_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    kind: SemanticFactKind,
    target: &str,
    assumptions: Vec<String>,
    note: &str,
) -> Result<SemanticFact, ParseError> {
    Ok(SemanticFact {
        kind,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(target.to_string()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: CPP_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: CPP_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Structural,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            unit.range.clone(),
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions,
    })
}

fn unknown_fact(
    document: &SourceDocument<'_>,
    unit: &CodeUnit,
    reason: UnknownReasonCode,
    affected_claim: &str,
    kind: &str,
    note: &str,
    extra_assumptions: Vec<String>,
) -> Result<SemanticFact, ParseError> {
    let mut assumptions = vec![
        format!("affected_claim={affected_claim}"),
        format!("cpp_unknown_kind={kind}"),
    ];
    assumptions.extend(extra_assumptions);
    Ok(SemanticFact {
        kind: SemanticFactKind::Unknown,
        subject: unit.id.as_str().to_string(),
        target: Some(SymbolId::new(reason.as_protocol_str()).map_err(ParseError::Internal)?),
        origin: FactOrigin {
            engine: CPP_ANCHOR_ENGINE.to_string(),
            engine_version: env!("CARGO_PKG_VERSION").to_string(),
            method: CPP_ANCHOR_METHOD.to_string(),
        },
        certainty: FactCertainty::Unknown,
        evidence: Evidence::new(
            CodeUnitId::new(unit.id.as_str().to_string()).map_err(ParseError::Internal)?,
            unit.range.clone(),
            Provenance::new(
                document.path,
                document.content_hash.clone(),
                document.repository_revision.clone(),
            )
            .map_err(ParseError::Internal)?,
            note,
        )
        .map_err(ParseError::Internal)?,
        assumptions,
    })
}

fn declarator_identifier<'a>(source: &'a str, function_definition: Node<'_>) -> Option<&'a str> {
    let declarator = function_definition.child_by_field_name("declarator")?;
    let declarator = if declarator.kind() == "function_declarator" {
        declarator
    } else {
        first_named_child_of_kind(declarator, "function_declarator")?
    };
    let inner = declarator.child_by_field_name("declarator")?;
    if inner.kind() == "identifier" {
        node_text_checked(source, inner)
    } else {
        None
    }
}

fn base_class_names(source: &str, node: Node<'_>) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() != "base_class_clause" {
            continue;
        }
        let mut base_cursor = child.walk();
        for base in child.named_children(&mut base_cursor) {
            if matches!(base.kind(), "type_identifier" | "qualified_identifier") {
                if let Some(text) = node_text_checked(source, base) {
                    names.push(text.trim().to_string());
                }
            }
        }
    }
    names
}

fn class_body_has_q_object(body: &str) -> bool {
    body.split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(|token| token == "Q_OBJECT")
}

fn first_named_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn first_type_identifier<'a>(source: &'a str, node: Node<'_>) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if child.kind() == "type_identifier" {
            return node_text_checked(source, child);
        }
    }
    None
}

fn node_text_checked<'a>(source: &'a str, node: Node<'_>) -> Option<&'a str> {
    source.get(node.start_byte()..node.end_byte())
}

fn slug(value: &str) -> String {
    let slug = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let compact = slug.trim_matches('_').to_string();
    if compact.is_empty() {
        "unit".to_string()
    } else {
        compact
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ContentHash, RepositoryRevision};

    fn parse_lang(text: &str, language: Language, path: &str) -> ParseReport {
        let document = SourceDocument {
            path,
            language,
            content_hash: ContentHash::new(
                "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            )
            .expect("valid hash"),
            repository_revision: RepositoryRevision::new("UNKNOWN").expect("valid revision"),
            text,
        };
        CppSyntaxParser.parse(document).expect("parse c/c++")
    }

    fn parse(text: &str) -> ParseReport {
        parse_lang(text, Language::Cpp, "tests/catalog_test.cc")
    }

    fn anchor_targets(report: &ParseReport) -> Vec<String> {
        let mut targets = report
            .semantic_facts
            .iter()
            .filter(|fact| {
                fact.kind != SemanticFactKind::Unknown
                    && fact.kind != SemanticFactKind::ProjectConfig
            })
            .map(|fact| fact.target.as_ref().expect("target").as_str().to_string())
            .collect::<Vec<_>>();
        targets.sort();
        targets
    }

    fn unknown_pairs(report: &ParseReport) -> Vec<(String, String)> {
        let mut pairs = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .map(|fact| {
                let reason = fact.target.as_ref().expect("reason").as_str().to_string();
                let claim = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                    .unwrap_or_default()
                    .to_string();
                (reason, claim)
            })
            .collect::<Vec<_>>();
        pairs.sort();
        pairs
    }

    fn unknown_details(report: &ParseReport) -> Vec<(String, String, String)> {
        let mut details = report
            .semantic_facts
            .iter()
            .filter(|fact| fact.kind == SemanticFactKind::Unknown)
            .map(|fact| {
                let reason = fact.target.as_ref().expect("reason").as_str().to_string();
                let claim = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("affected_claim="))
                    .unwrap_or_default()
                    .to_string();
                let kind = fact
                    .assumptions
                    .iter()
                    .find_map(|assumption| assumption.strip_prefix("cpp_unknown_kind="))
                    .unwrap_or_default()
                    .to_string();
                (reason, claim, kind)
            })
            .collect::<Vec<_>>();
        details.sort();
        details
    }

    fn unit_kinds(report: &ParseReport) -> Vec<&'static str> {
        report.units.iter().map(|unit| unit.kind.as_str()).collect()
    }

    #[test]
    fn gtest_function_definition_and_call_shapes_anchor_with_include() {
        let report = parse(
            "#include <gtest/gtest.h>\n\
             TEST(CatalogTest, ReturnsItems) { EXPECT_TRUE(true); }\n\
             TEST(CatalogTest, ReturnsItem);\n",
        );
        let targets = anchor_targets(&report);
        assert_eq!(
            targets
                .iter()
                .filter(|target| **target == "gtest.TEST")
                .count(),
            2
        );
        assert!(unit_kinds(&report).contains(&"gtest_test_case"));
        assert!(unknown_pairs(&report).is_empty());
    }

    #[test]
    fn gtest_without_include_is_blocking_unresolved_import() {
        let report = parse("TEST(CatalogTest, ReturnsItems) { }\n");
        assert!(anchor_targets(&report).is_empty());
        assert_eq!(
            unknown_pairs(&report),
            vec![(
                "UnresolvedImport".to_string(),
                "cpp_test_framework_identity".to_string()
            )]
        );
    }

    #[test]
    fn commented_string_and_conditional_includes_do_not_corroborate_gtest() {
        let report = parse(
            "// #include <gtest/gtest.h>\n\
             const char* fake = \"#include <gtest/gtest.h>\";\n\
             #if ENABLE_GTEST\n\
             #include <gtest/gtest.h>\n\
             #endif\n\
             TEST(CatalogTest, ReturnsItems) { }\n",
        );
        assert!(anchor_targets(&report).is_empty());
        assert_eq!(
            unknown_pairs(&report),
            vec![(
                "UnresolvedImport".to_string(),
                "cpp_test_framework_identity".to_string()
            )]
        );
    }

    #[test]
    fn test_case_catch2_versus_doctest_disambiguation_matrix() {
        let catch2 = parse(
            "#include <catch2/catch_test_macros.hpp>\n\
             TEST_CASE(\"reads items\") { CHECK(true); }\n\
             TEST_CASE(\"reads tagged items\", \"[catalog]\") { CHECK(true); }\n",
        );
        assert_eq!(
            anchor_targets(&catch2),
            vec![
                "catch2.TEST_CASE".to_string(),
                "catch2.TEST_CASE".to_string()
            ]
        );

        let doctest = parse(
            "#include <doctest/doctest.h>\n\
             TEST_CASE(\"reads items\") { CHECK(true); }\n",
        );
        assert_eq!(
            anchor_targets(&doctest),
            vec!["doctest.TEST_CASE".to_string()]
        );

        let both = parse(
            "#include <catch2/catch_test_macros.hpp>\n\
             #include <doctest/doctest.h>\n\
             TEST_CASE(\"reads items\") { CHECK(true); }\n",
        );
        assert!(anchor_targets(&both).is_empty());
        assert_eq!(
            unknown_pairs(&both),
            vec![(
                "ConflictingFacts".to_string(),
                "cpp_test_framework_identity".to_string()
            )]
        );

        let neither = parse("TEST_CASE(\"reads items\") { CHECK(true); }\n");
        assert_eq!(
            unknown_pairs(&neither),
            vec![(
                "UnresolvedImport".to_string(),
                "cpp_test_framework_identity".to_string()
            )]
        );
    }

    #[test]
    fn supported_gtest_macros_require_the_audited_identifier_contract() {
        let report = parse(
            "#include <gtest/gtest.h>\n\
             TEST(CatalogTest, Reads) { }\n\
             TEST_F(CatalogFixture, Reads) { }\n\
             TEST_P(CatalogParameters, Reads) { }\n\
             TYPED_TEST(CatalogTypes, Reads) { }\n",
        );
        assert_eq!(anchor_targets(&report).len(), 4);
        assert!(unknown_pairs(&report).is_empty());
    }

    #[test]
    fn official_gtest_macros_reject_underscores_in_either_name() {
        let sources = [
            "#include <gtest/gtest.h>\nTEST(Catalog_Test, Reads) { }\n",
            "#include <gtest/gtest.h>\nTEST(CatalogTest, Reads_Items) { }\n",
            "#include <gtest/gtest.h>\nTEST_F(Catalog_Fixture, Reads) { }\n",
            "#include <gtest/gtest.h>\nTEST_P(CatalogParameters, Reads_Items) { }\n",
        ];
        for source in sources {
            let report = parse(source);
            assert!(anchor_targets(&report).is_empty(), "source: {source}");
            assert!(unknown_details(&report).contains(&(
                "MacroOrPreprocessor".to_string(),
                "cpp_test_framework_identity".to_string(),
                "unsupported_test_macro_contract".to_string()
            )));
        }
    }

    #[test]
    fn catch2_optional_tags_require_an_explicit_square_bracket_list() {
        let valid = parse(
            "#include <catch2/catch_test_macros.hpp>\n\
             TEST_CASE(\"tagged\", \"[catalog][fast path]\") { }\n\
             SCENARIO(\"hidden\", \"[.][integration]\") { }\n",
        );
        assert_eq!(anchor_targets(&valid).len(), 2);
        assert!(unknown_pairs(&valid).is_empty());

        for tags in ["\"not tags\"", "\"[tag\"", "\"[tag] trailing\""] {
            let source = format!(
                "#include <catch2/catch_test_macros.hpp>\nTEST_CASE(\"name\", {tags}) {{ }}\n"
            );
            let report = parse(&source);
            assert!(anchor_targets(&report).is_empty(), "source: {source}");
            assert!(unknown_details(&report).contains(&(
                "MacroOrPreprocessor".to_string(),
                "cpp_test_framework_identity".to_string(),
                "unsupported_test_macro_contract".to_string()
            )));
        }
    }

    #[test]
    fn unsupported_macro_arity_or_shape_is_a_typed_identity_unknown() {
        let sources = [
            "TEST(CatalogTest) { }\n",
            "#include <gtest/gtest.h>\nTEST(\"CatalogTest\", Reads) { }\n",
            "#include <gtest/gtest.h>\nTEST(Catalog_Test, Reads) { }\n",
            "#include <catch2/catch_test_macros.hpp>\nTEST_CASE(\"name\", \"[tag]\", \"extra\") { }\n",
            "#include <catch2/catch_test_macros.hpp>\nTEST_CASE(\"name\", tags) { }\n",
            "#include <catch2/catch_test_macros.hpp>\nTEST_CASE(\"name\", \"not tags\") { }\n",
            "#include <catch2/catch_test_macros.hpp>\nTEST_CASE(\"name\", \"[tag\") { }\n",
            "#include <catch2/catch_test_macros.hpp>\nTEST_CASE(\"name\", \"[tag] trailing\") { }\n",
            "#include <doctest/doctest.h>\nTEST_CASE(\"name\", \"extra\") { }\n",
            "#include <doctest/doctest.h>\nTEST_CASE(\"name\" * doctest::timeout(1.0)) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, decorator, extra) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * utf::label(\"fast\")) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::label(\"fast\") * boost::unit_test::description(\"catalog\")) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::enable_if()) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::enable_if<true>()) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::label()) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::description()) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::enabled(\"unexpected\")) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::disabled(\"unexpected\")) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::depends_on(other)) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::fixture()) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::fixture(&setup, &teardown, &extra)) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::fixture<Fixture>()) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::precondition()) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_CASE(name, * boost::unit_test::precondition(first, second)) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_FIXTURE_TEST_CASE(name, Fixture, decorator, extra) { }\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_SUITE(name, decorator, extra)\nBOOST_AUTO_TEST_SUITE_END()\n",
            "#include <boost/test/unit_test.hpp>\nBOOST_AUTO_TEST_SUITE_END(unexpected)\n",
        ];
        for source in sources {
            let report = parse(source);
            assert!(anchor_targets(&report).is_empty(), "source: {source}");
            assert!(unknown_details(&report).contains(&(
                "MacroOrPreprocessor".to_string(),
                "cpp_test_framework_identity".to_string(),
                "unsupported_test_macro_contract".to_string()
            )));
        }
    }

    #[test]
    fn boost_case_and_suite_pairing_anchor_with_include() {
        let report = parse(
            "#include <boost/test/unit_test.hpp>\n\
             BOOST_AUTO_TEST_SUITE(catalog)\n\
             BOOST_AUTO_TEST_CASE(reads_items) { }\n\
             BOOST_FIXTURE_TEST_CASE(reads_item, Fixture) { }\n\
             BOOST_AUTO_TEST_SUITE(nested, * boost::unit_test::label(\"nested\"))\n\
             BOOST_AUTO_TEST_CASE(reads_nested, * boost::unit_test::label(\"fast\")) { }\n\
             BOOST_FIXTURE_TEST_CASE(reads_nested_item, Fixture, * boost::unit_test::label(\"fast\")) { }\n\
             BOOST_AUTO_TEST_SUITE_END()\n\
             BOOST_AUTO_TEST_CASE(reads_after_nested) { }\n\
             BOOST_AUTO_TEST_SUITE_END()\n\
             BOOST_AUTO_TEST_CASE(root_case) { }\n",
        );
        let targets = anchor_targets(&report);
        assert_eq!(
            targets
                .iter()
                .filter(|target| **target == "boost_test.BOOST_AUTO_TEST_SUITE")
                .count(),
            2
        );
        assert_eq!(
            targets
                .iter()
                .filter(|target| **target == "boost_test.BOOST_AUTO_TEST_CASE")
                .count(),
            4
        );
        assert_eq!(
            targets
                .iter()
                .filter(|target| **target == "boost_test.BOOST_FIXTURE_TEST_CASE")
                .count(),
            2
        );
        assert!(unknown_pairs(&report).is_empty());
    }

    #[test]
    fn boost_decorators_require_official_plain_call_signatures() {
        let report = parse(
            "#include <boost/test/unit_test.hpp>\n\
             BOOST_AUTO_TEST_CASE(dependency, * boost::unit_test::depends_on(\"prior\")) { }\n\
             BOOST_AUTO_TEST_CASE(described, * boost::unit_test::description(\"catalog\")) { }\n\
             BOOST_AUTO_TEST_CASE(enabled_case, * boost::unit_test::enabled()) { }\n\
             BOOST_AUTO_TEST_CASE(disabled_case, * boost::unit_test::disabled()) { }\n\
             BOOST_AUTO_TEST_CASE(labelled, * boost::unit_test::label(\"fast\")) { }\n\
             BOOST_AUTO_TEST_CASE(with_fixture, * boost::unit_test::fixture(&setup, &teardown)) { }\n\
             BOOST_AUTO_TEST_CASE(conditioned, * boost::unit_test::precondition(is_ready)) { }\n",
        );
        assert_eq!(anchor_targets(&report).len(), 7);
        assert!(unknown_pairs(&report).is_empty());
    }

    #[test]
    fn boost_orphan_end_and_following_case_are_blocking_unknowns() {
        let report = parse(
            "#include <boost/test/unit_test.hpp>\n\
             BOOST_AUTO_TEST_SUITE_END()\n\
             BOOST_AUTO_TEST_CASE(after_orphan_end) { }\n",
        );
        assert!(anchor_targets(&report).is_empty());
        let details = unknown_details(&report);
        assert!(details.contains(&(
            "MacroOrPreprocessor".to_string(),
            "cpp_test_framework_identity".to_string(),
            "boost_suite_end_orphan".to_string()
        )));
        assert!(details.contains(&(
            "MacroOrPreprocessor".to_string(),
            "cpp_test_framework_identity".to_string(),
            "boost_suite_scope_ambiguous".to_string()
        )));
    }

    #[test]
    fn boost_unclosed_suite_blocks_suite_and_case_support() {
        let report = parse(
            "#include <boost/test/unit_test.hpp>\n\
             BOOST_AUTO_TEST_SUITE(catalog)\n\
             BOOST_AUTO_TEST_CASE(reads_items) { }\n",
        );
        assert!(anchor_targets(&report).is_empty());
        let details = unknown_details(&report);
        assert_eq!(
            details
                .iter()
                .filter(|(reason, claim, kind)| {
                    reason == "MacroOrPreprocessor"
                        && claim == "cpp_test_framework_identity"
                        && kind == "boost_suite_scope_unclosed"
                })
                .count(),
            2
        );
    }

    #[test]
    fn gtest_fixture_base_class_anchors() {
        let report = parse(
            "#include <gtest/gtest.h>\n\
             class CatalogFixture : public ::testing::Test {\n\
             protected:\n  void SetUp() override {}\n};\n",
        );
        assert_eq!(
            anchor_targets(&report),
            vec!["gtest.testing.Test".to_string()]
        );
        assert!(unit_kinds(&report).contains(&"gtest_test_fixture"));
    }

    #[test]
    fn gtest_fixture_base_requires_exact_unconditional_include_evidence() {
        for prefix in [
            "",
            "#include <vendor/gtest/gtest.h>\n",
            "#if ENABLE_GTEST\n#include <gtest/gtest.h>\n#endif\n",
        ] {
            let source = format!(
                "{prefix}class CatalogFixture : public ::testing::Test {{\n\
                 protected:\n  void SetUp() override {{}}\n}};\n"
            );
            let report = parse(&source);
            assert!(
                !anchor_targets(&report).contains(&"gtest.testing.Test".to_string()),
                "prefix: {prefix}"
            );
            assert!(unit_kinds(&report).contains(&"class"), "prefix: {prefix}");
            assert!(unknown_details(&report).contains(&(
                "UnresolvedImport".to_string(),
                "cpp_test_framework_identity".to_string(),
                "gtest_fixture_without_include".to_string()
            )));
        }
    }

    #[test]
    fn include_guarded_header_emits_no_build_variant_unknown() {
        let report = parse_lang(
            "#ifndef CATALOG_H\n#define CATALOG_H\n\
             #include <gtest/gtest.h>\n\
             TEST(CatalogTest, ReturnsItems) { EXPECT_TRUE(true); }\n\
             #endif\n",
            Language::Cpp,
            "include/catalog_test.hpp",
        );
        assert_eq!(anchor_targets(&report), vec!["gtest.TEST".to_string()]);
        assert!(!unknown_pairs(&report)
            .iter()
            .any(|(_, claim)| claim == "cpp_build_variant"));
    }

    #[test]
    fn ifdef_region_blocks_anchored_unit_with_build_variant_unknown() {
        let report = parse(
            "#include <gtest/gtest.h>\n\
             #ifdef ENABLE_SLOW_TESTS\n\
             TEST(CatalogTest, ReturnsItems) { EXPECT_TRUE(true); }\n\
             #endif\n",
        );
        assert!(anchor_targets(&report).contains(&"gtest.TEST".to_string()));
        assert!(unknown_pairs(&report).contains(&(
            "BuildVariantAmbiguity".to_string(),
            "cpp_build_variant".to_string()
        )));
    }

    #[test]
    fn ordinary_ifndef_define_is_not_misclassified_as_include_guard() {
        let report = parse(
            "#include <gtest/gtest.h>\n\
             #ifndef ENABLE_SLOW_TESTS\n\
             #define ENABLE_SLOW_TESTS\n\
             TEST(CatalogTest, ReturnsItems) { EXPECT_TRUE(true); }\n\
             #endif\n\
             int outside_guard;\n",
        );
        assert!(anchor_targets(&report).contains(&"gtest.TEST".to_string()));
        assert!(unknown_pairs(&report).contains(&(
            "BuildVariantAmbiguity".to_string(),
            "cpp_build_variant".to_string()
        )));
    }

    #[test]
    fn unclosed_conditional_remains_a_build_variant_unknown() {
        let report = parse(
            "#include <gtest/gtest.h>\n\
             #if defined(ENABLE_SLOW_TESTS) && !defined(SKIP_SLOW_TESTS)\n\
             TEST(CatalogTest, ReturnsItems) { EXPECT_TRUE(true); }\n",
        );
        assert!(anchor_targets(&report).contains(&"gtest.TEST".to_string()));
        assert!(unknown_pairs(&report).contains(&(
            "BuildVariantAmbiguity".to_string(),
            "cpp_build_variant".to_string()
        )));
    }

    #[test]
    fn error_node_region_emits_macro_boundary_and_no_anchor() {
        let report = parse(
            "#include <gtest/gtest.h>\n\
             TEST(CatalogTest, ReturnsItems) {\n\
               connect(a, SIGNAL(x(int)), b, SLOT(y(int)));\n\
             }\n",
        );
        assert!(anchor_targets(&report).is_empty());
        assert!(unknown_pairs(&report).iter().any(|(reason, claim)| {
            reason == "MacroOrPreprocessor" && claim == "cpp_macro_boundary"
        }));
    }

    #[test]
    fn qt_object_class_and_signal_slot_are_non_blocking_context() {
        let report = parse(
            "class Widget : public QObject {\n  Q_OBJECT\npublic:\n  Widget();\n};\n\
             void wire(Widget* a, Widget* b) {\n\
               connect(a, SIGNAL(x(int)), b, SLOT(y(int)));\n\
             }\n",
        );
        assert!(unit_kinds(&report).contains(&"qt_object_class"));
        let pairs = unknown_pairs(&report);
        assert!(pairs.iter().any(
            |(reason, claim)| reason == "MacroOrPreprocessor" && claim == "cpp_generated_code"
        ));
        assert!(pairs
            .iter()
            .any(|(reason, claim)| reason == "FrameworkMagic"
                && claim == "cpp_signal_slot_string_dispatch"));
        assert!(anchor_targets(&report).is_empty());
    }

    #[test]
    fn c_grammar_parses_plain_c_translation_unit() {
        let report = parse_lang(
            "int add(int a, int b) { return a + b; }\n",
            Language::C,
            "src/math.c",
        );
        assert!(unit_kinds(&report).contains(&"module"));
        assert!(anchor_targets(&report).is_empty());
    }
}
