//! Bounded C/C++ preprocessor structure analysis.
//!
//! Tree-sitter identifies real directives and their source ranges. This module
//! never evaluates conditions or expands macros: it records non-guard
//! conditional regions, accepts framework includes only when they are
//! unconditional (or protected solely by a verified whole-file include guard),
//! and exposes parse-degradation helpers to the structural scanner.

use tree_sitter::Node;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct IncludeEvidence {
    pub(super) gtest: bool,
    pub(super) catch2: bool,
    pub(super) doctest: bool,
    pub(super) boost_test: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ConditionalRegion {
    start_byte: usize,
    end_byte: usize,
    shape: &'static str,
}

impl ConditionalRegion {
    fn overlaps(&self, start_byte: usize, end_byte: usize) -> bool {
        start_byte < self.end_byte && self.start_byte < end_byte
    }

    pub(super) fn shape(&self) -> &'static str {
        self.shape
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Analysis {
    pub(super) includes: IncludeEvidence,
    pub(super) conditional_regions: Vec<ConditionalRegion>,
}

/// Analyzes Tree-sitter preprocessor nodes in one bounded, non-executing pass.
pub(super) fn analyze(source: &str, root: Node<'_>) -> Analysis {
    let guard_range = whole_file_include_guard(source, root);
    let mut includes = IncludeEvidence::default();
    let mut conditional_regions = Vec::new();
    let mut stack = vec![(root, 0usize)];

    while let Some((node, inherited_conditional_depth)) = stack.pop() {
        let is_conditional = is_conditional_node(node);
        let is_guard = guard_range
            .is_some_and(|(start, end)| node.start_byte() == start && node.end_byte() == end);
        let conditional_depth =
            inherited_conditional_depth + usize::from(is_conditional && !is_guard);

        if is_conditional && !is_guard && inherited_conditional_depth == 0 {
            conditional_regions.push(ConditionalRegion {
                start_byte: node.start_byte(),
                end_byte: node.end_byte(),
                shape: conditional_shape(source, node),
            });
        } else if node.kind() == "preproc_include" && conditional_depth == 0 {
            if let Some(path) = include_path(source, node) {
                record_framework_include(&mut includes, path);
            }
        }

        let mut cursor = node.walk();
        let children = node.named_children(&mut cursor).collect::<Vec<_>>();
        stack.extend(
            children
                .into_iter()
                .rev()
                .map(|child| (child, conditional_depth)),
        );
    }

    Analysis {
        includes,
        conditional_regions,
    }
}

/// Finds an overlap in the source-ordered, non-overlapping maximal conditional
/// regions without rescanning every condition for every framework unit.
pub(super) fn first_overlapping_region(
    regions: &[ConditionalRegion],
    start_byte: usize,
    end_byte: usize,
) -> Option<&ConditionalRegion> {
    let index = regions.partition_point(|region| region.end_byte <= start_byte);
    regions
        .get(index)
        .filter(|region| region.overlaps(start_byte, end_byte))
}

/// Reports whether a region contains a Tree-sitter `ERROR` node without
/// recursively consuming the Rust call stack on deeply nested input.
pub(super) fn contains_error_node(node: Node<'_>) -> bool {
    let mut stack = vec![node];
    while let Some(current) = stack.pop() {
        if current.is_error() {
            return true;
        }
        let mut cursor = current.walk();
        stack.extend(current.children(&mut cursor));
    }
    false
}

/// Recognizes the bounded identifier shape used to surface unexpanded user
/// macro invocations as macro-boundary context.
pub(super) fn is_all_caps_macro(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|character| {
            character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
        })
        && name.chars().any(|character| character.is_ascii_uppercase())
}

fn whole_file_include_guard(source: &str, root: Node<'_>) -> Option<(usize, usize)> {
    let mut candidate = None;
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() == "comment" {
            continue;
        }
        if candidate.replace(child).is_some() {
            return None;
        }
    }
    let candidate = candidate?;
    if !is_conditional_node(candidate)
        || candidate.has_error()
        || candidate.child_by_field_name("alternative").is_some()
    {
        return None;
    }

    let guard_identifier = guard_identifier(source, candidate)?;
    let definition = first_guard_body_node(candidate)?;
    if definition.kind() != "preproc_def"
        || empty_define_identifier(source, definition) != Some(guard_identifier)
        || guard_identifier_is_redefined_or_undefined(
            source,
            candidate,
            definition,
            guard_identifier,
        )
    {
        return None;
    }

    Some((candidate.start_byte(), candidate.end_byte()))
}

fn guard_identifier_is_redefined_or_undefined(
    source: &str,
    guard: Node<'_>,
    initial_definition: Node<'_>,
    identifier: &str,
) -> bool {
    let mut stack = vec![guard];
    while let Some(node) = stack.pop() {
        if node.kind() == "preproc_def"
            && node.start_byte() != initial_definition.start_byte()
            && directive_for_node(source, node).is_some_and(|directive| {
                directive.name == "define"
                    && first_identifier_token(directive.argument) == Some(identifier)
            })
        {
            return true;
        }
        if node.kind() == "preproc_call"
            && directive_for_node(source, node).is_some_and(|directive| {
                directive.name == "undef"
                    && first_identifier_token(directive.argument) == Some(identifier)
            })
        {
            return true;
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    false
}

fn first_guard_body_node(node: Node<'_>) -> Option<Node<'_>> {
    let opener_row = node.start_position().row;
    let mut cursor = node.walk();
    let first = node
        .named_children(&mut cursor)
        .find(|child| child.start_position().row > opener_row && child.kind() != "comment");
    first
}

fn guard_identifier<'a>(source: &'a str, node: Node<'_>) -> Option<&'a str> {
    let directive = directive_for_node(source, node)?;
    match directive.name {
        "ifndef" if is_identifier(directive.argument) => Some(directive.argument),
        "if" => not_defined_identifier(directive.argument),
        _ => None,
    }
}

fn empty_define_identifier<'a>(source: &'a str, node: Node<'_>) -> Option<&'a str> {
    let directive = directive_for_node(source, node)?;
    (directive.name == "define" && is_identifier(directive.argument)).then_some(directive.argument)
}

fn not_defined_identifier(expression: &str) -> Option<&str> {
    let expression = expression.strip_prefix('!')?.trim_start();
    let after_defined = expression.strip_prefix("defined")?;
    if after_defined
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return None;
    }
    let operand = after_defined.trim();
    let identifier = if let Some(parenthesized) = operand.strip_prefix('(') {
        parenthesized.strip_suffix(')')?.trim()
    } else {
        operand
    };
    is_identifier(identifier).then_some(identifier)
}

fn is_conditional_node(node: Node<'_>) -> bool {
    matches!(node.kind(), "preproc_if" | "preproc_ifdef")
}

fn conditional_shape(source: &str, node: Node<'_>) -> &'static str {
    match directive_for_node(source, node).map(|directive| directive.name) {
        Some("ifdef") => "ifdef",
        Some("ifndef") => "ifndef",
        _ => "if_expr",
    }
}

fn include_path<'a>(source: &'a str, include: Node<'_>) -> Option<&'a str> {
    let path = include.child_by_field_name("path")?;
    let text = source.get(path.start_byte()..path.end_byte())?;
    let unquoted = match path.kind() {
        "system_lib_string" => text.strip_prefix('<')?.strip_suffix('>')?,
        "string_literal" => text.strip_prefix('"')?.strip_suffix('"')?,
        _ => return None,
    };
    is_normalized_include_path(unquoted).then_some(unquoted)
}

fn is_normalized_include_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains('\\')
        && !path.chars().any(char::is_control)
        && path.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '/' | '.' | '_' | '-' | '+')
        })
        && !path
            .split('/')
            .any(|component| component.is_empty() || component == "." || component == "..")
}

fn record_framework_include(evidence: &mut IncludeEvidence, path: &str) {
    match path {
        "gtest/gtest.h" | "gmock/gmock.h" => evidence.gtest = true,
        "catch.hpp" => evidence.catch2 = true,
        "doctest/doctest.h" | "doctest.h" => evidence.doctest = true,
        _ if path
            .strip_prefix("catch2/")
            .is_some_and(|rest| !rest.is_empty()) =>
        {
            evidence.catch2 = true;
        }
        _ if path
            .strip_prefix("boost/test/")
            .is_some_and(|rest| !rest.is_empty()) =>
        {
            evidence.boost_test = true;
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Directive<'a> {
    name: &'a str,
    argument: &'a str,
}

fn directive_for_node<'a>(source: &'a str, node: Node<'_>) -> Option<Directive<'a>> {
    let tail = source.get(node.start_byte()..node.end_byte())?;
    parse_directive_line(tail.lines().next()?)
}

fn parse_directive_line(line: &str) -> Option<Directive<'_>> {
    let line = line.trim_start_matches([' ', '\t']);
    let after_hash = line.strip_prefix('#')?.trim_start_matches([' ', '\t']);
    let name_end = after_hash
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(after_hash.len());
    if name_end == 0 {
        return None;
    }
    let name = &after_hash[..name_end];
    let argument = after_hash[name_end..].trim();
    Some(Directive { name, argument })
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(character) if character.is_ascii_alphabetic() || character == '_' => {}
        _ => return false,
    }
    chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn first_identifier_token(text: &str) -> Option<&str> {
    let end = text
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(text.len());
    let identifier = &text[..end];
    is_identifier(identifier).then_some(identifier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tree_sitter::Parser;

    fn analyze_cpp(source: &str) -> Analysis {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .expect("load C++ grammar");
        let tree = parser.parse(source, None).expect("parse C++ source");
        analyze(source, tree.root_node())
    }

    #[test]
    fn exact_unconditional_framework_includes_are_recorded() {
        let analysis = analyze_cpp(
            "#include <gtest/gtest.h>\n\
             #include \"catch2/catch_test_macros.hpp\"\n\
             #include <doctest/doctest.h>\n\
             #include <boost/test/unit_test.hpp>\n",
        );
        assert_eq!(
            analysis.includes,
            IncludeEvidence {
                gtest: true,
                catch2: true,
                doctest: true,
                boost_test: true,
            }
        );
        assert!(analysis.conditional_regions.is_empty());
    }

    #[test]
    fn commented_pseudo_and_non_exact_includes_are_ignored() {
        let analysis = analyze_cpp(
            "// #include <gtest/gtest.h>\n\
             /* #include <catch2/catch_test_macros.hpp> */\n\
             const char* text = \"#include <doctest/doctest.h>\";\n\
             #define PSEUDO_INCLUDE \"#include <boost/test/unit_test.hpp>\"\n\
             #include <vendor/gtest/gtest.h>\n",
        );
        assert_eq!(analysis.includes, IncludeEvidence::default());
    }

    #[test]
    fn commented_and_string_conditional_pseudodirectives_are_ignored() {
        let analysis = analyze_cpp(
            "// #if ENABLE_GTEST\n\
             /* #ifdef ENABLE_CATCH2\n#endif */\n\
             const char* text = \"#ifndef ENABLE_DOCTEST\";\n\
             #include <gtest/gtest.h>\n",
        );
        assert!(analysis.includes.gtest);
        assert!(analysis.conditional_regions.is_empty());
    }

    #[test]
    fn conditional_and_nested_conditional_includes_are_not_unconditional() {
        let analysis = analyze_cpp(
            "#if USE_GTEST\n\
             #include <gtest/gtest.h>\n\
             #if USE_CATCH2\n\
             #include <catch2/catch_test_macros.hpp>\n\
             #endif\n\
             #endif\n",
        );
        assert_eq!(analysis.includes, IncludeEvidence::default());
        assert_eq!(analysis.conditional_regions.len(), 1);
        assert!(analysis
            .conditional_regions
            .iter()
            .all(|region| region.shape() == "if_expr"));
    }

    #[test]
    fn verified_ifndef_guard_allows_framework_include_without_variant() {
        let analysis = analyze_cpp(
            "// license\n\
             #ifndef CATALOG_TEST_HPP\n\
             #define CATALOG_TEST_HPP\n\
             #include <gtest/gtest.h>\n\
             TEST(CatalogTest, Reads) {}\n\
             #endif\n\
             // trailing comment\n",
        );
        assert!(analysis.includes.gtest);
        assert!(analysis.conditional_regions.is_empty());
    }

    #[test]
    fn verified_if_not_defined_guard_is_exempt() {
        let analysis = analyze_cpp(
            "#if !defined(CATALOG_TEST_HPP)\n\
             #define CATALOG_TEST_HPP\n\
             #include <gtest/gtest.h>\n\
             #endif\n",
        );
        assert!(analysis.includes.gtest);
        assert!(analysis.conditional_regions.is_empty());
    }

    #[test]
    fn partial_or_value_defining_ifndef_is_not_an_include_guard() {
        let partial = analyze_cpp(
            "#ifndef ENABLE_GTEST\n\
             #define ENABLE_GTEST\n\
             #include <gtest/gtest.h>\n\
             #endif\n\
             int outside_guard;\n",
        );
        assert!(!partial.includes.gtest);
        assert_eq!(partial.conditional_regions.len(), 1);
        assert_eq!(partial.conditional_regions[0].shape(), "ifndef");

        let value_defining = analyze_cpp(
            "#ifndef ENABLE_GTEST\n\
             #define ENABLE_GTEST 1\n\
             #include <gtest/gtest.h>\n\
             #endif\n",
        );
        assert!(!value_defining.includes.gtest);
        assert_eq!(value_defining.conditional_regions.len(), 1);

        let undefined = analyze_cpp(
            "#ifndef ENABLE_GTEST\n\
             #define ENABLE_GTEST\n\
             #undef ENABLE_GTEST\n\
             #include <gtest/gtest.h>\n\
             #endif\n",
        );
        assert!(!undefined.includes.gtest);
        assert_eq!(undefined.conditional_regions.len(), 1);
    }

    #[test]
    fn commented_undef_prevents_guard_exemption() {
        for suffix in [" // trailing comment", " /* trailing comment */"] {
            let source = format!(
                "#ifndef CATALOG_TEST_HPP\n\
                 #define CATALOG_TEST_HPP\n\
                 #undef CATALOG_TEST_HPP{suffix}\n\
                 #include <gtest/gtest.h>\n\
                 #endif\n"
            );
            let analysis = analyze_cpp(&source);
            assert!(!analysis.includes.gtest, "suffix: {suffix}");
            assert_eq!(analysis.conditional_regions.len(), 1, "suffix: {suffix}");
        }
    }

    #[test]
    fn guard_with_alternative_branch_is_not_exempt() {
        for alternative in ["#else\nint fallback;", "#elif USE_FALLBACK\nint fallback;"] {
            let source = format!(
                "#ifndef CATALOG_TEST_HPP\n\
                 #define CATALOG_TEST_HPP\n\
                 #include <gtest/gtest.h>\n\
                 {alternative}\n\
                 #endif\n"
            );
            let analysis = analyze_cpp(&source);
            assert!(!analysis.includes.gtest, "alternative: {alternative}");
            assert_eq!(
                analysis.conditional_regions.len(),
                1,
                "alternative: {alternative}"
            );
        }
    }

    #[test]
    fn nested_condition_inside_guard_remains_a_variant_and_cannot_add_evidence() {
        let analysis = analyze_cpp(
            "#ifndef CATALOG_TEST_HPP\n\
             #define CATALOG_TEST_HPP\n\
             #if ENABLE_GTEST\n\
             #include <gtest/gtest.h>\n\
             #endif\n\
             #endif\n",
        );
        assert!(!analysis.includes.gtest);
        assert_eq!(analysis.conditional_regions.len(), 1);
        assert_eq!(analysis.conditional_regions[0].shape(), "if_expr");
    }

    #[test]
    fn complex_and_unclosed_conditions_remain_conservative_variants() {
        let complex = analyze_cpp(
            "#if defined(ENABLE_GTEST) && (!defined(DISABLE_GTEST) || FORCE_GTEST)\n\
             #include <gtest/gtest.h>\n\
             #endif\n",
        );
        assert!(!complex.includes.gtest);
        assert_eq!(complex.conditional_regions.len(), 1);
        assert_eq!(complex.conditional_regions[0].shape(), "if_expr");

        let unclosed = analyze_cpp(
            "#ifdef ENABLE_GTEST\n\
             #include <gtest/gtest.h>\n",
        );
        assert!(!unclosed.includes.gtest);
        assert_eq!(unclosed.conditional_regions.len(), 1);
        assert_eq!(unclosed.conditional_regions[0].shape(), "ifdef");
    }

    #[test]
    fn macro_identifier_shape_is_strict_and_bounded() {
        assert!(is_all_caps_macro("REGISTER_CASE_2"));
        assert!(!is_all_caps_macro("RegisterCase"));
        assert!(!is_all_caps_macro("123_"));
        assert!(!is_all_caps_macro("REGISTER-CASE"));
    }
}
