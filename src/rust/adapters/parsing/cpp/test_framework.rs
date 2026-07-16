//! Pure structural analysis for supported C++ test-framework macro contracts.
//!
//! This module inspects Tree-sitter shapes only. It never expands a macro,
//! resolves namespace aliases, evaluates decorators, or executes a framework.

use tree_sitter::Node;

#[derive(Debug, Clone)]
pub(super) enum TestMacroKind {
    Gtest {
        target: &'static str,
        fixture: bool,
        forbids_name_underscores: bool,
    },
    TestCase,
    Scenario,
    BoostAutoCase,
    BoostFixtureCase,
    BoostSuite,
    BoostSuiteEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MacroArgumentKind {
    Identifier,
    UnderscoreFreeIdentifier,
    StringLiteral,
    Catch2TagList,
    BoostDecoratorExpression,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MacroShape {
    arguments: Vec<MacroArgumentKind>,
}

impl MacroShape {
    pub(super) fn argument_count(&self) -> usize {
        self.arguments.len()
    }

    pub(super) fn first_is_string_literal(&self) -> bool {
        self.arguments.first().is_some_and(|kind| {
            matches!(
                kind,
                MacroArgumentKind::StringLiteral | MacroArgumentKind::Catch2TagList
            )
        })
    }

    fn prefix_is_identifiers(&self, count: usize) -> bool {
        self.arguments.len() >= count
            && self.arguments[..count].iter().all(|kind| {
                matches!(
                    kind,
                    MacroArgumentKind::Identifier | MacroArgumentKind::UnderscoreFreeIdentifier
                )
            })
    }

    fn prefix_is_underscore_free_identifiers(&self, count: usize) -> bool {
        self.arguments.len() >= count
            && self.arguments[..count]
                .iter()
                .all(|kind| *kind == MacroArgumentKind::UnderscoreFreeIdentifier)
    }

    fn is_catch2_test_contract(&self) -> bool {
        self.first_is_string_literal()
            && (self.argument_count() == 1
                || (self.argument_count() == 2
                    && self.arguments[1] == MacroArgumentKind::Catch2TagList))
    }
}

pub(super) fn classify(name: &str) -> Option<TestMacroKind> {
    match name {
        "TEST" => Some(TestMacroKind::Gtest {
            target: "gtest.TEST",
            fixture: false,
            forbids_name_underscores: true,
        }),
        "TEST_F" => Some(TestMacroKind::Gtest {
            target: "gtest.TEST_F",
            fixture: true,
            forbids_name_underscores: true,
        }),
        "TEST_P" => Some(TestMacroKind::Gtest {
            target: "gtest.TEST_P",
            fixture: true,
            forbids_name_underscores: true,
        }),
        "TYPED_TEST" => Some(TestMacroKind::Gtest {
            target: "gtest.TYPED_TEST",
            fixture: true,
            forbids_name_underscores: false,
        }),
        "TEST_CASE" => Some(TestMacroKind::TestCase),
        "SCENARIO" => Some(TestMacroKind::Scenario),
        "BOOST_AUTO_TEST_CASE" => Some(TestMacroKind::BoostAutoCase),
        "BOOST_FIXTURE_TEST_CASE" => Some(TestMacroKind::BoostFixtureCase),
        "BOOST_AUTO_TEST_SUITE" => Some(TestMacroKind::BoostSuite),
        "BOOST_AUTO_TEST_SUITE_END" => Some(TestMacroKind::BoostSuiteEnd),
        _ => None,
    }
}

pub(super) fn contract_is_supported(
    macro_kind: &TestMacroKind,
    shape: &MacroShape,
    catch2_include: bool,
    doctest_include: bool,
) -> bool {
    if let Some(supported) = boost_contract_is_supported(macro_kind, shape) {
        return supported;
    }
    match macro_kind {
        TestMacroKind::Gtest {
            forbids_name_underscores,
            ..
        } => {
            shape.argument_count() == 2
                && if *forbids_name_underscores {
                    shape.prefix_is_underscore_free_identifiers(2)
                } else {
                    shape.prefix_is_identifiers(2)
                }
        }
        TestMacroKind::TestCase => {
            let catch2_shape = shape.is_catch2_test_contract();
            let doctest_shape = shape.argument_count() == 1 && shape.first_is_string_literal();
            match (catch2_include, doctest_include) {
                (true, false) => catch2_shape,
                (false, true) => doctest_shape,
                (true, true) | (false, false) => catch2_shape || doctest_shape,
            }
        }
        TestMacroKind::Scenario => shape.is_catch2_test_contract(),
        TestMacroKind::BoostAutoCase
        | TestMacroKind::BoostFixtureCase
        | TestMacroKind::BoostSuite
        | TestMacroKind::BoostSuiteEnd => unreachable!("Boost contracts return above"),
    }
}

fn boost_contract_is_supported(macro_kind: &TestMacroKind, shape: &MacroShape) -> Option<bool> {
    match macro_kind {
        TestMacroKind::BoostFixtureCase => Some(
            (shape.argument_count() == 2
                || (shape.argument_count() == 3
                    && shape.arguments[2] == MacroArgumentKind::BoostDecoratorExpression))
                && shape.prefix_is_identifiers(2),
        ),
        TestMacroKind::BoostAutoCase | TestMacroKind::BoostSuite => Some(
            (shape.argument_count() == 1
                || (shape.argument_count() == 2
                    && shape.arguments[1] == MacroArgumentKind::BoostDecoratorExpression))
                && shape.prefix_is_identifiers(1),
        ),
        TestMacroKind::BoostSuiteEnd => Some(shape.argument_count() == 0),
        _ => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SuiteIssue {
    pub(super) kind: &'static str,
    pub(super) note: &'static str,
}

const SUITE_SCOPE_AMBIGUOUS: SuiteIssue = SuiteIssue {
    kind: "boost_suite_scope_ambiguous",
    note: "Boost.Test registration follows an unmatched or invalid suite marker, so its active suite scope is not trusted",
};

const SUITE_END_ORPHAN: SuiteIssue = SuiteIssue {
    kind: "boost_suite_end_orphan",
    note: "BOOST_AUTO_TEST_SUITE_END has no matching source-ordered suite opener",
};

const SUITE_SCOPE_UNCLOSED: SuiteIssue = SuiteIssue {
    kind: "boost_suite_scope_unclosed",
    note: "Boost.Test suite scope remains open at end of file, so registrations in that scope are not anchored",
};

#[derive(Debug, Default)]
pub(super) struct Analysis {
    issues: Vec<(usize, SuiteIssue)>,
    next_issue: usize,
}

impl Analysis {
    pub(super) fn take_issue_at(&mut self, start_byte: usize) -> Option<SuiteIssue> {
        while self
            .issues
            .get(self.next_issue)
            .is_some_and(|(issue_start, _)| *issue_start < start_byte)
        {
            self.next_issue += 1;
        }
        let (_, issue) = self
            .issues
            .get(self.next_issue)
            .filter(|(issue_start, _)| *issue_start == start_byte)?;
        self.next_issue += 1;
        Some(*issue)
    }
}

struct MacroOccurrence<'a> {
    name: &'a str,
    shape: MacroShape,
    start_byte: usize,
}

#[derive(Debug, Clone, Copy)]
struct SuiteFrame {
    occurrence_index: usize,
    valid: bool,
}

/// Validates Boost.Test suite markers in source order with an explicit stack.
/// Root-level cases belong to Boost.Test's implicit master suite and are valid.
pub(super) fn analyze(source: &str, root: Node<'_>) -> Analysis {
    let mut occurrences = Vec::new();
    collect_macro_occurrences(source, root, &mut occurrences);
    let mut issues = vec![None; occurrences.len()];
    let mut stack = Vec::<SuiteFrame>::new();
    let mut invalid_scope_depth = 0usize;
    let mut structure_is_ambiguous = false;

    for (occurrence_index, occurrence) in occurrences.iter().enumerate() {
        let Some(macro_kind) = classify(occurrence.name) else {
            continue;
        };
        match &macro_kind {
            TestMacroKind::BoostSuite => {
                let contract_is_valid =
                    boost_contract_is_supported(&macro_kind, &occurrence.shape).unwrap_or(false);
                let scope_is_valid =
                    contract_is_valid && !structure_is_ambiguous && invalid_scope_depth == 0;
                if contract_is_valid && !scope_is_valid {
                    issues[occurrence_index] = Some(SUITE_SCOPE_AMBIGUOUS);
                }
                stack.push(SuiteFrame {
                    occurrence_index,
                    valid: scope_is_valid,
                });
                invalid_scope_depth += usize::from(!scope_is_valid);
            }
            TestMacroKind::BoostAutoCase | TestMacroKind::BoostFixtureCase => {
                if structure_is_ambiguous || invalid_scope_depth > 0 {
                    issues[occurrence_index] = Some(SUITE_SCOPE_AMBIGUOUS);
                }
            }
            TestMacroKind::BoostSuiteEnd => {
                let contract_is_valid =
                    boost_contract_is_supported(&macro_kind, &occurrence.shape).unwrap_or(false);
                if !contract_is_valid {
                    structure_is_ambiguous = true;
                } else if let Some(frame) = stack.pop() {
                    invalid_scope_depth -= usize::from(!frame.valid);
                    structure_is_ambiguous |= !frame.valid;
                } else {
                    issues[occurrence_index] = Some(SUITE_END_ORPHAN);
                    structure_is_ambiguous = true;
                }
            }
            _ => {}
        }
    }

    if let Some(earliest_unclosed) = stack.iter().map(|frame| frame.occurrence_index).min() {
        for (occurrence_index, occurrence) in occurrences.iter().enumerate().skip(earliest_unclosed)
        {
            if classify(occurrence.name).is_some_and(|kind| {
                matches!(
                    kind,
                    TestMacroKind::BoostAutoCase
                        | TestMacroKind::BoostFixtureCase
                        | TestMacroKind::BoostSuite
                )
            }) {
                issues[occurrence_index] = Some(SUITE_SCOPE_UNCLOSED);
            }
        }
    }

    Analysis {
        issues: occurrences
            .iter()
            .zip(issues)
            .filter_map(|(occurrence, issue)| issue.map(|issue| (occurrence.start_byte, issue)))
            .collect(),
        next_issue: 0,
    }
}

pub(super) fn function_macro_shape<'a>(
    source: &'a str,
    function_definition: Node<'_>,
) -> Option<(&'a str, MacroShape)> {
    let declarator = function_definition.child_by_field_name("declarator")?;
    let function_declarator = if declarator.kind() == "function_declarator" {
        declarator
    } else {
        first_named_child_of_kind(declarator, "function_declarator")?
    };
    let name_node = function_declarator.child_by_field_name("declarator")?;
    if name_node.kind() != "identifier" {
        return None;
    }
    let name = node_text(source, name_node)?;
    let parameters = function_declarator.child_by_field_name("parameters")?;
    let mut cursor = parameters.walk();
    let arguments = parameters
        .named_children(&mut cursor)
        .filter(|parameter| parameter.kind() == "parameter_declaration")
        .map(|parameter| macro_argument_kind(source, parameter))
        .collect();
    Some((name, MacroShape { arguments }))
}

pub(super) fn call_macro_shape(source: &str, call: Node<'_>) -> MacroShape {
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return MacroShape {
            arguments: Vec::new(),
        };
    };
    let mut cursor = arguments.walk();
    let arguments = arguments
        .named_children(&mut cursor)
        .map(|argument| macro_argument_kind(source, argument))
        .collect();
    MacroShape { arguments }
}

fn collect_macro_occurrences<'a>(
    source: &'a str,
    node: Node<'_>,
    occurrences: &mut Vec<MacroOccurrence<'a>>,
) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        match child.kind() {
            "function_definition" if child.child_by_field_name("type").is_none() => {
                if let Some((name, shape)) = function_macro_shape(source, child) {
                    occurrences.push(MacroOccurrence {
                        name,
                        shape,
                        start_byte: child.start_byte(),
                    });
                }
            }
            "expression_statement" => {
                if let Some(call) = first_named_child_of_kind(child, "call_expression") {
                    if let Some(name) = call
                        .child_by_field_name("function")
                        .filter(|function| function.kind() == "identifier")
                        .and_then(|function| node_text(source, function))
                    {
                        occurrences.push(MacroOccurrence {
                            name,
                            shape: call_macro_shape(source, call),
                            start_byte: child.start_byte(),
                        });
                    }
                }
            }
            "namespace_definition"
            | "linkage_specification"
            | "declaration_list"
            | "preproc_ifdef"
            | "preproc_if"
            | "preproc_else"
            | "preproc_elif" => collect_macro_occurrences(source, child, occurrences),
            _ => {}
        }
    }
}

fn macro_argument_kind(source: &str, argument: Node<'_>) -> MacroArgumentKind {
    if argument.kind() == "string_literal" {
        return node_text(source, argument)
            .filter(|text| is_catch2_tag_list_literal(text))
            .map_or(MacroArgumentKind::StringLiteral, |_| {
                MacroArgumentKind::Catch2TagList
            });
    }
    if is_exact_boost_decorator_expression(source, argument) {
        return MacroArgumentKind::BoostDecoratorExpression;
    }
    node_text(source, argument)
        .map(str::trim)
        .filter(|text| is_cpp_identifier(text))
        .map_or(MacroArgumentKind::Other, |text| {
            if text.contains('_') {
                MacroArgumentKind::Identifier
            } else {
                MacroArgumentKind::UnderscoreFreeIdentifier
            }
        })
}

fn is_exact_boost_decorator_expression(source: &str, argument: Node<'_>) -> bool {
    if argument.kind() != "pointer_expression" {
        return false;
    }
    if argument
        .child(0)
        .is_none_or(|operator| operator.kind() != "*")
    {
        return false;
    }
    let Some(call) = argument.child_by_field_name("argument") else {
        return false;
    };
    if call.kind() != "call_expression" {
        return false;
    }
    let Some(function) = call.child_by_field_name("function") else {
        return false;
    };
    let Some(function_name) = node_text(source, function) else {
        return false;
    };
    let Some(arguments) = call.child_by_field_name("arguments") else {
        return false;
    };
    let mut cursor = arguments.walk();
    let mut call_arguments = arguments.named_children(&mut cursor);
    let first = call_arguments.next();
    let second = call_arguments.next();
    let has_more = call_arguments.next().is_some();
    match function_name {
        "boost::unit_test::depends_on"
        | "boost::unit_test::description"
        | "boost::unit_test::label" => {
            first.is_some_and(|argument| argument.kind() == "string_literal") && second.is_none()
        }
        "boost::unit_test::enabled" | "boost::unit_test::disabled" => first.is_none(),
        "boost::unit_test::fixture" => first.is_some() && !has_more,
        "boost::unit_test::precondition" => first.is_some() && second.is_none(),
        _ => false,
    }
}

/// Accepts the conservative source spelling `"[tag][other tag]"` only.
/// Catch2 permits broader ASCII tag contents, but escaped, raw, prefixed, empty,
/// nested, or non-adjacent spellings stay unsupported rather than being decoded.
fn is_catch2_tag_list_literal(text: &str) -> bool {
    let bytes = text.as_bytes();
    if bytes.len() < 4 || bytes.first() != Some(&b'"') || bytes.last() != Some(&b'"') {
        return false;
    }

    let mut index = 1usize;
    let content_end = bytes.len() - 1;
    let mut tag_count = 0usize;
    while index < content_end {
        if bytes[index] != b'[' {
            return false;
        }
        index += 1;
        let tag_start = index;
        while index < content_end && bytes[index] != b']' {
            let byte = bytes[index];
            if !(0x20..=0x7e).contains(&byte) || matches!(byte, b'[' | b'\\' | b'"') {
                return false;
            }
            index += 1;
        }
        if index == tag_start || index >= content_end {
            return false;
        }
        index += 1;
        tag_count += 1;
    }
    tag_count > 0
}

fn is_cpp_identifier(text: &str) -> bool {
    let mut characters = text.chars();
    match characters.next() {
        Some(character) if character.is_ascii_alphabetic() || character == '_' => {}
        _ => return false,
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn first_named_child_of_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node
        .named_children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

fn node_text<'a>(source: &'a str, node: Node<'_>) -> Option<&'a str> {
    source.get(node.start_byte()..node.end_byte())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    #[test]
    fn tree_sitter_macro_shapes_match_the_bounded_contract_model() {
        let source = "TEST(Suite, Name) {}\n\
                      TEST_CASE(\"name\", \"[tag]\") {}\n\
                      TEST_CASE(\"name\" * doctest::timeout(1.0)) {}\n\
                      BOOST_AUTO_TEST_SUITE(outer)\n\
                      BOOST_AUTO_TEST_CASE(case_name, * boost::unit_test::label(\"fast\")) {}\n\
                      BOOST_FIXTURE_TEST_CASE(fixture_case, Fixture, * boost::unit_test::label(\"fast\")) {}\n\
                      BOOST_AUTO_TEST_SUITE_END()\n";
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("load C++ grammar");
        let tree = parser.parse(source, None).expect("parse C++ source");
        let mut occurrences = Vec::new();
        collect_macro_occurrences(source, tree.root_node(), &mut occurrences);
        let shapes = occurrences
            .iter()
            .map(|occurrence| (occurrence.name, occurrence.shape.arguments.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            shapes,
            vec![
                (
                    "TEST",
                    vec![
                        MacroArgumentKind::UnderscoreFreeIdentifier,
                        MacroArgumentKind::UnderscoreFreeIdentifier
                    ]
                ),
                (
                    "TEST_CASE",
                    vec![
                        MacroArgumentKind::StringLiteral,
                        MacroArgumentKind::Catch2TagList
                    ]
                ),
                ("TEST_CASE", vec![MacroArgumentKind::Other]),
                (
                    "BOOST_AUTO_TEST_SUITE",
                    vec![MacroArgumentKind::UnderscoreFreeIdentifier]
                ),
                (
                    "BOOST_AUTO_TEST_CASE",
                    vec![
                        MacroArgumentKind::Identifier,
                        MacroArgumentKind::BoostDecoratorExpression
                    ]
                ),
                (
                    "BOOST_FIXTURE_TEST_CASE",
                    vec![
                        MacroArgumentKind::Identifier,
                        MacroArgumentKind::UnderscoreFreeIdentifier,
                        MacroArgumentKind::BoostDecoratorExpression
                    ]
                ),
                ("BOOST_AUTO_TEST_SUITE_END", Vec::new()),
            ]
        );
    }

    #[test]
    fn catch2_tag_lists_require_adjacent_balanced_nonempty_brackets() {
        for literal in [
            "\"[tag]\"",
            "\"[tag][other tag]\"",
            "\"[.][integration]\"",
            "\"[!throws]\"",
        ] {
            assert!(is_catch2_tag_list_literal(literal), "literal: {literal}");
        }
        for literal in [
            "\"not tags\"",
            "\"[tag\"",
            "\"[tag] trailing\"",
            "\"[tag] [other]\"",
            "\"[]\"",
            "\"[[nested]]\"",
            "R\"([tag])\"",
        ] {
            assert!(!is_catch2_tag_list_literal(literal), "literal: {literal}");
        }
    }
}
