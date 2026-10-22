//! Tree-sitter reader for the ADR-0045 declared Ada subset (ADR-0058).
//!
//! The syntax layer is the pinned `tree-sitter-ada` grammar. The ADR-0045 lexer
//! stays in front of it as the lexical-subset gate, so a file is read by
//! Tree-sitter only after the lexer has proved that every literal, comment and
//! delimiter is inside the declared subset, and after the edition-sensitive
//! words have been refused. The ADR-0045 subset policy is applied over the
//! tree: this module turns the tree into an [`AdmittedUnit`] or a [`Refusal`],
//! and nothing else.
//!
//! The reader has two outcomes and no third: it accepts the whole compilation
//! unit, or it refuses the file. It never recovers, because an anchor recovered
//! from a guess is indistinguishable from an anchor that is there. Any `ERROR`
//! or `MISSING` node refuses the file as [`Refusal::OutsideDeclaredSubset`], so
//! a gap in the grammar costs recall and can never invent a registration.
//!
//! ## The invariance argument
//!
//! RepoGrammar cannot see which Ada edition a GPR project selects, so the
//! declared subset is built so that it does not need to. For every text this
//! reader admits, the token stream and the phrase structure are the same under
//! **every edition of {Ada 95, Ada 2005, Ada 2012, Ada 2022} under which that
//! text is legal**. Ada 83 is outside the set and costs nothing: `'Access` does
//! not exist in Ada 83, so no Ada 83 compilation unit can contain the admitted
//! anchor in the first place.
//!
//! The one construct class that could break this is a word whose classification
//! moves between editions, and [`super::lexer`] gives those five words their
//! own token kind. `overriding` is admitted only immediately before `procedure`
//! or `function`, where the Ada 95 reading is not legal at all, so the two
//! editions cannot disagree about a text that parses. Every other occurrence of
//! an edition-sensitive word is refused before the grammar sees the file.
//!
//! ## Precedence
//!
//! A left-to-right parse reports the refusal it reaches first. This reader
//! reproduces that over the tree: the lexical and edition gates run over the
//! whole token stream first, and among the tree refusals (a syntax error, a
//! construct outside the declared subset, the nesting ceiling, an
//! edition-selecting pragma) the earliest source position wins. At one position
//! a structural refusal precedes the pragma refusal, because a pragma in a
//! position the declared grammar does not reach is never read as a pragma.

use super::lexer::{is_reserved_word, lex, Refusal, Token, TokenKind};
use crate::ports::parser::ParseError;
use tree_sitter::{Node, Parser};

/// Nesting ceiling. Input is untrusted, and deeply nested source is refused
/// rather than analysed. The depth is measured over the tree with the same
/// production weights the declared grammar charged, see [`own_depth`].
pub(crate) const MAX_DEPTH: usize = 96;

/// Byte range of one admitted `Register_Routine` call, from the first character
/// of the callee name through the closing parenthesis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Registration {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct AdmittedUnit {
    /// A context clause in this file names a library unit whose first
    /// identifier is `AUnit`.
    pub aunit_context_clause: bool,
    pub registrations: Vec<Registration>,
}

/// Configuration pragmas that re-select the language, which is precisely what
/// the invariance argument assumes fixed.
///
/// `Ada_95`, `Ada_05`, `Ada_2005`, `Ada_12`, `Ada_2012`, and `Ada_2022` are
/// deliberately absent: they select a member of the declared set, and the
/// admitted parse is identical across the whole set, so they change nothing
/// this frontend claims.
const LANGUAGE_EDITION_PRAGMAS: &[&str] = &["ada_83", "ada_1983", "extensions_allowed"];

const REGISTER_ROUTINE: &str = "register_routine";

/// Constructs ADR-0045 D4a names as outside the declared subset. A file holding
/// one abstains as a whole.
const OUTSIDE_SUBSET_KINDS: &[&str] = &[
    // Generic declarations (instantiations are admitted).
    "generic_package_declaration",
    "generic_subprogram_declaration",
    "generic_renaming_declaration",
    "generic_formal_part",
    // Task and protected types and bodies, entries, and every tasking statement.
    "task_type_declaration",
    "single_task_declaration",
    "task_body",
    "task_body_stub",
    "task_definition",
    "protected_type_declaration",
    "single_protected_declaration",
    "protected_body",
    "protected_body_stub",
    "protected_definition",
    "entry_declaration",
    "entry_body",
    "accept_statement",
    "selective_accept",
    "asynchronous_select",
    "conditional_entry_call",
    "timed_entry_call",
    "requeue_statement",
    "abort_statement",
    "delay_relative_statement",
    "delay_until_statement",
    "terminate_alternative",
    // Interface and synchronized types.
    "interface_type_definition",
    "formal_interface_type_definition",
    // Representation clauses.
    "attribute_definition_clause",
    "enumeration_representation_clause",
    "record_representation_clause",
    "at_clause",
    "mod_clause",
    "component_clause",
    // Subunits, and the package stub. A subprogram stub (`procedure P is
    // separate;`) is an ordinary declaration the declared grammar admits.
    "subunit",
    "package_body_stub",
    // Ada 2022 and later: declare expressions, the `@` target name, raise
    // expressions, delta aggregates, square-bracket aggregates and iterated
    // component associations.
    "declare_expression",
    "target_name",
    "raise_expression",
    "array_delta_aggregate",
    "record_delta_aggregate",
    "value_sequence",
    "iterated_element_association",
    "reduction_attribute_designator",
    // SPARK and Ada 2022 `Global` aspect modes, which the declared aspect
    // grammar (an aspect name and an expression) does not read.
    "global_aspect_definition",
    "global_aspect_element",
    "global_mode",
];

/// Reads one `.adb` text. The outer `Result` carries a grammar-loading failure,
/// which is an internal error and never a refusal.
pub(crate) fn parse_compilation(source: &str) -> Result<Result<AdmittedUnit, Refusal>, ParseError> {
    let tokens = match lex(source) {
        Ok(tokens) => tokens,
        Err(refusal) => return Ok(Err(refusal)),
    };
    if let Err(refusal) = refuse_misplaced_edition_sensitive_words(source, &tokens) {
        return Ok(Err(refusal));
    }
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_ada::LANGUAGE.into())
        .map_err(|error| ParseError::Internal(format!("load Ada grammar: {error}")))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| ParseError::Internal("Tree-sitter Ada parse failed".to_string()))?;
    Ok(read_compilation(source, tree.root_node()))
}

/// An edition-sensitive word is admitted in exactly one position: as the
/// `overriding_indicator` of a subprogram declaration or body, where Ada 95
/// has no legal reading of the same text.
fn refuse_misplaced_edition_sensitive_words(source: &str, tokens: &[Token]) -> Result<(), Refusal> {
    for (index, token) in tokens.iter().enumerate() {
        if token.kind != TokenKind::EditionSensitive {
            continue;
        }
        let is_overriding_indicator = token.text(source).eq_ignore_ascii_case("overriding")
            && tokens.get(index + 1).is_some_and(|next| {
                next.matches(source, TokenKind::Reserved, "procedure")
                    || next.matches(source, TokenKind::Reserved, "function")
            });
        if !is_overriding_indicator {
            return Err(Refusal::EditionSensitiveWord);
        }
    }
    Ok(())
}

/// The earliest tree refusal. A structural refusal outranks a pragma refusal at
/// the same position, and an earlier recorded refusal wins a remaining tie.
#[derive(Default)]
struct EarliestRefusal(Option<(usize, u8, Refusal)>);

impl EarliestRefusal {
    fn note(&mut self, position: usize, rank: u8, refusal: Refusal) {
        if self
            .0
            .is_none_or(|(at, held, _)| (position, rank) < (at, held))
        {
            self.0 = Some((position, rank, refusal));
        }
    }
}

const STRUCTURAL: u8 = 0;
const PRAGMA: u8 = 1;

fn read_compilation(source: &str, root: Node<'_>) -> Result<AdmittedUnit, Refusal> {
    let mut refusals = EarliestRefusal::default();
    let mut admitted = AdmittedUnit::default();
    check_units(source, root, &mut refusals, &mut admitted);
    walk(source, root, &mut refusals, &mut admitted);
    match refusals.0 {
        Some((_, _, refusal)) => Err(refusal),
        None => Ok(admitted),
    }
}

/// `compilation ::= { pragma | with_clause | use_clause } library_item`
///
/// The grammar lets a compilation hold any run of declarations and statements,
/// so the ADR-0045 D1/D2 shape is enforced here: context items, then exactly
/// one package body or library-level subprogram body, then nothing. A subunit
/// or a spec is refused for the reasons ADR-0045 D1 and D2 give.
fn check_units(
    source: &str,
    root: Node<'_>,
    refusals: &mut EarliestRefusal,
    admitted: &mut AdmittedUnit,
) {
    let mut cursor = root.walk();
    let units: Vec<Node<'_>> = root
        .named_children(&mut cursor)
        .filter(|node| node.kind() == "compilation_unit")
        .collect();
    let mut library_item_seen = false;
    for unit in &units {
        if library_item_seen {
            refusals.note(
                unit.start_byte(),
                STRUCTURAL,
                Refusal::OutsideDeclaredSubset,
            );
            return;
        }
        let mut unit_cursor = unit.walk();
        let parts: Vec<Node<'_>> = unit
            .children(&mut unit_cursor)
            .filter(|node| node.kind() != "comment")
            .collect();
        match parts.as_slice() {
            [part] if matches!(part.kind(), "with_clause") => {
                if with_clause_names_aunit(source, *part) {
                    admitted.aunit_context_clause = true;
                }
            }
            [part] if matches!(part.kind(), "use_clause" | "pragma_g") => {}
            [part] if matches!(part.kind(), "package_body" | "subprogram_body") => {
                library_item_seen = true;
            }
            _ => {
                refusals.note(
                    unit.start_byte(),
                    STRUCTURAL,
                    Refusal::OutsideDeclaredSubset,
                );
                return;
            }
        }
    }
    // With a syntax error present, the `ERROR` node is the refusal and sits at
    // the position error recovery chose; a missing library item is then only an
    // artifact of that recovery and must not outrank an earlier pragma.
    if !library_item_seen && !root.has_error() {
        refusals.note(
            units.first().map_or(0, Node::start_byte),
            STRUCTURAL,
            Refusal::OutsideDeclaredSubset,
        );
    }
}

/// True when a withed unit's first identifier is `AUnit`.
fn with_clause_names_aunit(source: &str, clause: Node<'_>) -> bool {
    let mut cursor = clause.walk();
    let names: Vec<Node<'_>> = clause.named_children(&mut cursor).collect();
    names.into_iter().any(|name| {
        let mut leftmost = name;
        while leftmost.kind() == "selected_component" {
            match leftmost.named_child(0) {
                Some(prefix) => leftmost = prefix,
                None => return false,
            }
        }
        leftmost.kind() == "identifier"
            && leftmost
                .utf8_text(source.as_bytes())
                .is_ok_and(|text| text.eq_ignore_ascii_case("aunit"))
    })
}

/// Iterative pre-order walk over every node, so hostile nesting cannot exhaust
/// the stack. It records syntax errors, constructs outside the subset, the
/// nesting ceiling, edition-selecting pragmas, and admitted registrations.
fn walk(source: &str, root: Node<'_>, refusals: &mut EarliestRefusal, admitted: &mut AdmittedUnit) {
    let mut cursor = root.walk();
    // Cumulative depth at each open ancestor; the last entry is the parent of
    // the node about to be visited. The ancestors themselves are kept so no
    // check needs `Node::parent`, which costs a walk from the root.
    let mut depths: Vec<usize> = vec![0];
    let mut ancestors: Vec<Node<'_>> = Vec::new();
    loop {
        let node = cursor.node();
        let parent_depth = depths.last().copied().unwrap_or(0);
        let depth = parent_depth + own_depth(node, &ancestors);
        visit(source, node, &ancestors, depth, refusals, admitted);
        if depth > MAX_DEPTH {
            // Every node still to visit starts at or after this one, so no
            // later refusal can outrank the ceiling.
            return;
        }
        if cursor.goto_first_child() {
            depths.push(depth);
            ancestors.push(node);
            continue;
        }
        loop {
            if cursor.goto_next_sibling() {
                break;
            }
            if !cursor.goto_parent() {
                return;
            }
            depths.pop();
            ancestors.pop();
        }
    }
}

fn visit(
    source: &str,
    node: Node<'_>,
    ancestors: &[Node<'_>],
    depth: usize,
    refusals: &mut EarliestRefusal,
    admitted: &mut AdmittedUnit,
) {
    if node.is_missing() {
        refusals.note(
            node.start_byte(),
            STRUCTURAL,
            Refusal::OutsideDeclaredSubset,
        );
        return;
    }
    if node.is_error() {
        // Error recovery wraps the parsed prefix into the node, so its start can
        // precede the real failure by the whole declaration; its end is where
        // the parser gave up, which is where a left-to-right parse stops.
        refusals.note(node.end_byte(), STRUCTURAL, Refusal::OutsideDeclaredSubset);
        return;
    }
    if depth > MAX_DEPTH {
        refusals.note(node.start_byte(), STRUCTURAL, Refusal::NestingLimit);
    }
    let kind = node.kind();
    if OUTSIDE_SUBSET_KINDS.contains(&kind) {
        refusals.note(
            node.start_byte(),
            STRUCTURAL,
            Refusal::OutsideDeclaredSubset,
        );
        return;
    }
    match kind {
        "identifier" => {
            if reserved_word_used_as_name(source, node, ancestors) {
                refusals.note(
                    node.start_byte(),
                    STRUCTURAL,
                    Refusal::OutsideDeclaredSubset,
                );
            }
        }
        "procedure_call_statement" | "assignment_statement" if !starts_with_identifier(node) => {
            // A statement begins with a name, and a name begins with an
            // identifier; the grammar also lets an operator symbol or a
            // character literal start one.
            refusals.note(
                node.start_byte(),
                STRUCTURAL,
                Refusal::OutsideDeclaredSubset,
            );
        }
        "pragma_g" => {
            let selects_edition = node.named_child(0).is_some_and(|name| {
                name.utf8_text(source.as_bytes()).is_ok_and(|text| {
                    LANGUAGE_EDITION_PRAGMAS
                        .iter()
                        .any(|pragma| text.eq_ignore_ascii_case(pragma))
                })
            });
            if selects_edition {
                refusals.note(node.start_byte(), PRAGMA, Refusal::LanguageEditionPragma);
            }
        }
        _ => {}
    }
    if kind == "procedure_call_statement" && starts_with_identifier(node) {
        if let Some(registration) = registration(source, node) {
            admitted.registrations.push(registration);
        }
    }
}

/// The leftmost token of a statement is an identifier.
fn starts_with_identifier(statement: Node<'_>) -> bool {
    let mut leftmost = statement;
    while let Some(child) = leftmost.child(0) {
        leftmost = child;
    }
    leftmost.kind() == "identifier"
}

/// The grammar accepts some reserved words as names (`end;` and `terminate;`
/// read as procedure calls), which the declared subset does not: a reserved word
/// is never an identifier. Three places spell a reserved word as an identifier
/// node on purpose.
fn reserved_word_used_as_name(source: &str, node: Node<'_>, ancestors: &[Node<'_>]) -> bool {
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return false;
    };
    if !is_reserved_word(text) {
        return false;
    }
    let Some(&parent) = ancestors.last() else {
        return true;
    };
    match parent.kind() {
        // Any word names a pragma.
        "pragma_g" => parent.named_child(0) != Some(node),
        // The grammar reads the `private` of `type T is new B with private;` as
        // an aspect name; that is the private extension.
        "aspect_association" => {
            // The association, its list, the specification, the declaration.
            let private_extension = text.eq_ignore_ascii_case("private")
                && ancestors
                    .len()
                    .checked_sub(4)
                    .and_then(|index| ancestors.get(index))
                    .is_some_and(|declaration| {
                        let mut cursor = declaration.walk();
                        let derived = declaration
                            .named_children(&mut cursor)
                            .any(|part| part.kind() == "derived_type_definition");
                        derived
                    });
            !private_extension
        }
        // `Pointer.all`: the selector the grammar spells as a name, except in
        // the dotted names the declared grammar reads as plain identifiers.
        "selected_component" => {
            !(text.eq_ignore_ascii_case("all")
                && parent.child(parent.child_count().saturating_sub(1)) == Some(node)
                && !in_plain_dotted_name(ancestors))
        }
        _ => true,
    }
}

/// True when this dotted name sits where the declared grammar allows only plain
/// identifiers: a `with` clause, or the name of a package or subprogram.
fn in_plain_dotted_name(ancestors: &[Node<'_>]) -> bool {
    for ancestor in ancestors.iter().rev() {
        match ancestor.kind() {
            "selected_component" => continue,
            "with_clause"
            | "package_body"
            | "procedure_specification"
            | "function_specification" => return true,
            _ => return false,
        }
    }
    false
}

/// How many nesting levels the declared grammar charged for this node. The
/// ceiling bounds untrusted nesting, so the weights follow the productions the
/// frontend recurses through: a declarative part, a statement sequence, a record
/// component list, an expression, a primary, a name, and a parenthesised
/// association list.
fn own_depth(node: Node<'_>, ancestors: &[Node<'_>]) -> usize {
    match node.kind() {
        "non_empty_declarative_part"
        | "handled_sequence_of_statements"
        | "if_statement"
        | "case_statement_alternative"
        | "loop_statement"
        | "component_list"
        | "expression"
        | "function_call"
        | "actual_parameter_part"
        | "procedure_call_statement"
        | "record_aggregate"
        | "extension_aggregate"
        | "positional_array_aggregate"
        | "named_array_aggregate"
        | "enumeration_aggregate"
        | "null_array_aggregate" => 1,
        // A name plus a parenthesised part.
        "qualified_expression" => 2,
        // A name used as a primary is read through the name production.
        "identifier" | "selected_component"
            if ancestors
                .last()
                .is_some_and(|parent| parent.kind() == "term") =>
        {
            1
        }
        "if_expression" | "case_expression" | "quantified_expression" => 1,
        // A primary, plus one more when the primary is a parenthesised expression.
        "term" => {
            let mut cursor = node.walk();
            let parenthesised = node
                .named_children(&mut cursor)
                .any(|child| matches!(child.kind(), "expression"));
            1 + usize::from(parenthesised)
        }
        _ => 0,
    }
}

/// ADR-0045 D2's exact anchor, read off the tree: a procedure call statement
/// naming `Register_Routine`, with exactly three positional actuals, the second
/// a plain dotted name with a final `'Access` and the third one string literal.
fn registration(source: &str, call: Node<'_>) -> Option<Registration> {
    let name = call.child_by_field_name("name")?;
    let mut cursor = call.walk();
    let actuals = call
        .named_children(&mut cursor)
        .find(|child| child.kind() == "actual_parameter_part")?;
    if designator(source, name)?.eq_ignore_ascii_case(REGISTER_ROUTINE) {
        let mut actual_cursor = actuals.walk();
        let associations: Vec<Node<'_>> = actuals
            .named_children(&mut actual_cursor)
            .filter(|child| child.kind() != "comment")
            .collect();
        if associations.len() == 3
            && associations
                .iter()
                .all(|node| node.kind() == "parameter_association")
            && positional_single_term(associations[1])
                .is_some_and(|term| is_access_name(source, term))
            && positional_single_term(associations[2]).is_some_and(is_string_literal)
        {
            return Some(Registration {
                start: name.start_byte(),
                end: actuals.end_byte(),
            });
        }
    }
    None
}

fn is_all(source: &str, node: Node<'_>) -> bool {
    node.utf8_text(source.as_bytes())
        .is_ok_and(|text| text.eq_ignore_ascii_case("all"))
}

/// The text of the identifier that names the called subprogram: the name itself
/// or the last selector of a dotted name.
fn designator<'s>(source: &'s str, name: Node<'_>) -> Option<&'s str> {
    let identifier = match name.kind() {
        "identifier" => name,
        // A call of a call result keeps the designator of the inner name.
        "function_call" => return designator(source, name.named_child(0)?),
        "selected_component" => {
            let mut cursor = name.walk();
            let selector = name
                .named_children(&mut cursor)
                .filter(|child| child.kind() != "comment")
                .last()?;
            if selector.kind() != "identifier" {
                return None;
            }
            selector
        }
        _ => return None,
    };
    identifier.utf8_text(source.as_bytes()).ok()
}

/// The sole `term` of a positional association whose expression is that term
/// alone: no named notation, no `<>`, no operator.
fn positional_single_term(association: Node<'_>) -> Option<Node<'_>> {
    let mut cursor = association.walk();
    let parts: Vec<Node<'_>> = association
        .children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .collect();
    let [expression] = parts.as_slice() else {
        return None;
    };
    if expression.kind() != "expression" {
        return None;
    }
    let mut expression_cursor = expression.walk();
    let parts: Vec<Node<'_>> = expression
        .children(&mut expression_cursor)
        .filter(|node| node.kind() != "comment")
        .collect();
    match parts.as_slice() {
        [term] if term.kind() == "term" => Some(*term),
        _ => None,
    }
}

fn is_string_literal(term: Node<'_>) -> bool {
    let mut cursor = term.walk();
    let parts: Vec<Node<'_>> = term
        .children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .collect();
    matches!(parts.as_slice(), [only] if only.kind() == "string_literal")
}

/// A plain dotted name whose only other suffix is a final `'Access`.
fn is_access_name(source: &str, term: Node<'_>) -> bool {
    let mut cursor = term.walk();
    let parts: Vec<Node<'_>> = term
        .children(&mut cursor)
        .filter(|node| node.kind() != "comment")
        .collect();
    let [prefix, tick, attribute] = parts.as_slice() else {
        return false;
    };
    tick.kind() == "tick"
        && attribute.kind() == "attribute_designator"
        && attribute
            .utf8_text(source.as_bytes())
            .is_ok_and(|text| text.eq_ignore_ascii_case("access"))
        && is_plain_dotted_name(source, *prefix)
}

fn is_plain_dotted_name(source: &str, name: Node<'_>) -> bool {
    let mut current = name;
    loop {
        match current.kind() {
            "identifier" => return true,
            // An operator symbol is a name (`"+"'Access`).
            "string_literal" => return true,
            "selected_component" => {
                let mut cursor = current.walk();
                let children: Vec<Node<'_>> = current
                    .children(&mut cursor)
                    .filter(|node| node.kind() != "comment")
                    .collect();
                match children.as_slice() {
                    [prefix, dot, selector]
                        if dot.kind() == "."
                            && selector.kind() == "identifier"
                            && !is_all(source, *selector) =>
                    {
                        current = *prefix;
                    }
                    _ => return false,
                }
            }
            _ => return false,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
                        procedure Register_Tests (T : in out Test_Case) is\nbegin\n";
    const TAIL: &str = "end Register_Tests;\nend Catalog_Tests;\n";

    fn body(statements: &str) -> String {
        format!("{HEAD}{statements}{TAIL}")
    }

    fn parse(source: &str) -> Result<AdmittedUnit, Refusal> {
        parse_compilation(source).expect("the Ada grammar loads")
    }

    fn registrations(source: &str) -> usize {
        parse(source).expect("parse").registrations.len()
    }

    #[test]
    fn the_admitted_call_anchors_bare_prefixed_and_multi_line() {
        let parsed = parse(&body(
            "Register_Routine (T, Loads'Access, \"loads\");\n\
             Registration.Register_Routine\n  (T, Filters'Access, \"filters\");\n\
             AUnit.Test_Cases.Registration.Register_Routine (T, Sorts'Access, \"sorts\");\n",
        ))
        .expect("parse");
        assert_eq!(parsed.registrations.len(), 3);
        assert!(parsed.aunit_context_clause);
    }

    #[test]
    fn the_registration_span_covers_the_whole_dotted_callee() {
        let source = body("Registration.Register_Routine (T, Loads'Access, \"loads\");\n");
        let parsed = parse(&source).expect("parse");
        let span = parsed.registrations[0];
        assert_eq!(
            &source[span.start..span.end],
            "Registration.Register_Routine (T, Loads'Access, \"loads\")"
        );
    }

    #[test]
    fn only_a_real_context_clause_proves_the_import() {
        // A `with` inside a type extension is not a context clause, and the old
        // text scan could not tell the two apart.
        let parsed = parse(
            "with Ada.Text_IO;\npackage body Catalog_Tests is\n\
               type AUnit_Like is new Base with null record;\n\
               procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
               Register_Routine (T, Loads'Access, \"loads\");\n\
               end Register_Tests;\nend Catalog_Tests;\n",
        )
        .expect("parse");
        assert!(!parsed.aunit_context_clause);
        assert_eq!(parsed.registrations.len(), 1);
    }

    #[test]
    fn a_registration_only_anchors_as_a_procedure_call_statement() {
        // A function result named `Register_Routine` is not AUnit's procedure,
        // and the text scan this parser replaces could not tell them apart.
        assert_eq!(
            registrations(&body(
                "Handle := Register_Routine (T, Loads'Access, \"loads\");\n"
            )),
            0
        );
        assert_eq!(
            registrations(&body(
                "Report (Register_Routine (T, Loads'Access, \"loads\"));\n"
            )),
            0
        );
    }

    #[test]
    fn a_body_at_the_input_ceiling_parses_within_the_bounded_budget() {
        let mut source = String::from("with AUnit.Test_Cases;\npackage body Big is\n");
        while source.len() < 1_000_000 {
            source.push_str(
                "procedure P (A : in out Natural) is\n   L : constant Natural := 16#FF#;\n\
                 begin\n   for I in 1 .. 10 loop\n      A := A + Table (I)'Length * L;\n\
                 end loop;\n   Register_Routine (T, Loads'Access, \"loads\");\nend P;\n",
            );
        }
        source.push_str("end Big;\n");
        let started = std::time::Instant::now();
        let parsed = parse(&source).expect("parse");
        assert!(parsed.registrations.len() > 1_000);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "a bounded parse of the input ceiling took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn the_argument_shape_is_required() {
        for call in [
            "Register_Routine (T, Loads'Access);",
            "Register_Routine (T, Routine_Ptr, \"loads\");",
            "Register_Routine (T, Loads'Access, Description);",
            "Register_Routine (T, Loads'Address, \"loads\");",
            "Register_Routine (T, Loads'Access, \"loads\", Extra);",
            // A concatenation is not a string literal.
            "Register_Routine (T, Loads'Access, \"a\" & \"b\");",
            // A call result is not a plain name, so its `'Access` is not the
            // admitted second actual.
            "Register_Routine (T, Table (1)'Access, \"loads\");",
            "My_Register_Routine (T, Loads'Access, \"loads\");",
            // Named notation for the routine and the description is not the
            // admitted positional shape.
            "Register_Routine (T, Routine => Loads'Access, Name => \"loads\");",
        ] {
            assert_eq!(registrations(&body(&format!("{call}\n"))), 0, "{call}");
        }
    }

    #[test]
    fn a_nested_call_in_the_first_argument_does_not_split_the_arguments() {
        assert_eq!(
            registrations(&body(
                "Register_Routine (Fixture (T, 1), Loads'Access, \"loads\");\n"
            )),
            1
        );
    }

    #[test]
    fn a_registration_in_a_comment_or_a_string_never_anchors() {
        assert_eq!(
            registrations(&body(
                "--  Register_Routine (T, In_A_Comment'Access, \"no\");\n\
                 Note := \"Register_Routine (T, In_A_String'Access, \"\"no\"\")\";\n"
            )),
            0
        );
    }

    #[test]
    fn case_insensitivity_follows_the_language() {
        let parsed = parse(
            "WITH Aunit.Test_Cases;\nPACKAGE BODY Catalog_Tests IS\n\
               PROCEDURE Register_Tests (T : IN OUT Test_Case) IS\nBEGIN\n\
               REGISTER_ROUTINE (T, Loads'ACCESS, \"loads\");\n\
               END Register_Tests;\nEND Catalog_Tests;\n",
        )
        .expect("parse");
        assert!(parsed.aunit_context_clause);
        assert_eq!(parsed.registrations.len(), 1);
    }

    #[test]
    fn an_edition_sensitive_word_outside_the_overriding_indicator_is_refused() {
        for declaration in [
            "Interface : Boolean := False;",
            "Some : Natural := 1;",
            "Parallel : Boolean := False;",
            "Synchronized : Boolean := False;",
        ] {
            let source = format!(
                "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n{declaration}\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n"
            );
            assert_eq!(
                parse(&source),
                Err(Refusal::EditionSensitiveWord),
                "{declaration}"
            );
        }
    }

    #[test]
    fn an_overriding_indicator_is_the_one_admitted_placement() {
        let parsed = parse(
            "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n\
               overriding function Name (T : Test_Case) return Message_String is\n\
               begin\n   return Format (\"catalog\");\nend Name;\n\
               not overriding procedure Set_Up (T : in out Test_Case) is\nbegin\n null;\nend Set_Up;\n\
               procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
               Register_Routine (T, Loads'Access, \"loads\");\n\
               end Register_Tests;\nend Catalog_Tests;\n",
        )
        .expect("parse");
        assert_eq!(parsed.registrations.len(), 1);
    }

    #[test]
    fn an_edition_selecting_pragma_is_refused_and_a_member_of_the_set_is_not() {
        assert_eq!(
            parse(&format!("pragma Ada_83;\n{}", body(""))),
            Err(Refusal::LanguageEditionPragma)
        );
        assert_eq!(
            parse(&format!("pragma Extensions_Allowed (On);\n{}", body(""))),
            Err(Refusal::LanguageEditionPragma)
        );
        for pragma in ["pragma Ada_95;", "pragma Ada_2012;", "pragma Ada_2022;"] {
            let source = format!(
                "{pragma}\n{}",
                body("Register_Routine (T, Loads'Access, \"loads\");\n")
            );
            assert_eq!(registrations(&source), 1, "{pragma}");
        }
    }

    #[test]
    fn constructs_outside_the_declared_subset_abstain_rather_than_recover() {
        for declaration in [
            "task body Worker is begin null; end Worker;",
            "protected body Guard is end Guard;",
            "generic\n type Element is private;\n procedure Swap (A : in out Element);",
            "for Flags'Address use System.Null_Address;",
        ] {
            let source = format!(
                "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n{declaration}\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n"
            );
            assert_eq!(
                parse(&source),
                Err(Refusal::OutsideDeclaredSubset),
                "{declaration}"
            );
        }
        for statement in [
            "accept Ready;",
            "select\n   accept Ready;\nelse\n   null;\nend select;",
            "delay 1.0;",
            "abort Worker;",
        ] {
            assert_eq!(
                parse(&body(&format!("{statement}\n"))),
                Err(Refusal::OutsideDeclaredSubset),
                "{statement}"
            );
        }
    }

    /// Inputs on which the grammar deliberately differs from the retired
    /// hand-written parser (ADR-0058): text that is not valid Ada, or newer
    /// than the pinned grammar, is refused; valid Ada the old parser could not
    /// read is admitted.
    #[test]
    fn the_grammar_refuses_invalid_ada_and_reads_the_constructs_the_old_parser_missed() {
        let in_package = |declaration: &str| {
            format!(
                "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n{declaration}\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\n\
                 end Register_Tests;\nend Catalog_Tests;\n"
            )
        };
        for declaration in [
            "function Q return T is null;",
            "not procedure Q is null;",
            "subtype T is access Integer;",
            "subtype T is A range <>;",
            "subtype P is not null access Integer;",
            "type T is range <>;",
            "type T is record end record;",
            "type T is record null; A : Integer; end record;",
            "X : T := A and B or C;",
            "X : T := A and then B or else C;",
            "X : T := A and B xor C;",
            "X : T := A or B xor C;",
            "X : T := A'Range;",
            "X : T := T'Range (1);",
            "X : T := new access T;",
        ] {
            assert_eq!(
                parse(&in_package(declaration)),
                Err(Refusal::OutsideDeclaredSubset),
                "{declaration}"
            );
        }
        for statement in [
            "case X is end case;",
            "L : if A then null; end if;",
            "L : case X is when others => null; end case;",
            "for I : Integer in 1 .. 3 loop null; end loop;",
            "if A and then B or else C then null; end if;",
            "Register_Routine (T, 1 .. 2, \"loads\");",
            "Register_Routine (T, 1 | 2 => X, \"loads\");",
            "Do_It (null => 1);",
            "null;\nexception when null => null;",
            "X := Y'range;",
            "X := Y'range (1);",
        ] {
            assert_eq!(
                parse(&body(&format!("{statement}\n"))),
                Err(Refusal::OutsideDeclaredSubset),
                "{statement}"
            );
        }
        for declaration in [
            "function Max is new Generic_Max (Integer);",
            "function \"+\" is new R (Integer);",
            "X : Arr := (1 .. 3 => 0);",
            "X : T := (for all I : T in A => B);",
            "type T is new A and B with null record;",
            "type T is new A and B with private;",
            "X renames Y;",
            "type T is tagged;",
            "type T is record A : Integer; end record T;",
            "type T is record pragma Convention (C, T); A : Integer; end record;",
            "type T is record A : Integer; pragma Atomic (A); end record;",
        ] {
            assert_eq!(registrations(&in_package(declaration)), 1, "{declaration}");
        }
        parse(&body("for I in 1 .. 3 when I > 1 loop null; end loop;\n")).expect("iterator filter");
    }

    #[test]
    fn a_subunit_and_a_spec_are_not_admitted_compilation_units() {
        assert_eq!(
            parse(
                "with AUnit.Test_Cases;\nseparate (Catalog_Tests)\n\
                 procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
                 Register_Routine (T, Loads'Access, \"loads\");\nend Register_Tests;\n"
            ),
            Err(Refusal::OutsideDeclaredSubset)
        );
        assert_eq!(
            parse("with AUnit.Test_Cases;\npackage Catalog_Tests is\nend Catalog_Tests;\n"),
            Err(Refusal::OutsideDeclaredSubset)
        );
    }

    #[test]
    fn a_library_level_subprogram_body_is_admitted() {
        assert_eq!(
            registrations(
                "with AUnit.Test_Cases;\nprocedure Register_Tests (T : in out Test_Case) is\n\
                 begin\n   Register_Routine (T, Loads'Access, \"loads\");\nend Register_Tests;\n"
            ),
            1
        );
    }

    #[test]
    fn deep_nesting_hits_the_ceiling_instead_of_the_stack() {
        let source = body(&format!(
            "X := {}1{};\n",
            "(".repeat(MAX_DEPTH * 4),
            ")".repeat(MAX_DEPTH * 4)
        ));
        assert_eq!(parse(&source), Err(Refusal::NestingLimit));
    }

    #[test]
    fn a_reserved_word_the_grammar_reads_as_a_name_is_refused() {
        // The grammar parses `end;` and `terminate;` as procedure calls.
        for statement in ["end;", "terminate;", "Pkg.end;", "X := terminate;"] {
            assert_eq!(
                parse(&body(&format!("{statement}\n"))),
                Err(Refusal::OutsideDeclaredSubset),
                "{statement}"
            );
        }
        // `Pointer.all` is a dereference, and any word may name a pragma.
        assert_eq!(
            registrations(&body("Ptr.all := 1;\npragma Interface_Name (X);\n")),
            0
        );
    }

    #[test]
    fn a_pointer_dereference_is_not_a_plain_name_for_the_access_actual() {
        assert_eq!(
            registrations(&body("Register_Routine (T, Ptr.all'Access, \"loads\");\n")),
            0
        );
        assert_eq!(
            registrations(&body("Register_Routine (T, Pkg.Ptr'Access, \"loads\");\n")),
            1
        );
    }

    #[test]
    fn a_subprogram_stub_is_a_declaration_and_a_package_stub_is_not() {
        assert_eq!(
            registrations(&decl_with("procedure Helper is separate;")),
            1
        );
        assert_eq!(
            parse(&decl_with("package Helper is separate;")),
            Err(Refusal::OutsideDeclaredSubset)
        );
    }

    #[test]
    fn the_earliest_refusal_wins_over_the_tree() {
        // A pragma before a syntax error is the refusal a left-to-right parse
        // reaches first, and so is the syntax error before a pragma.
        assert_eq!(
            parse(&body("pragma Ada_83;\nX := ;\n")),
            Err(Refusal::LanguageEditionPragma)
        );
        assert_eq!(
            parse(&body("X := ;\npragma Ada_83;\nnull;\n")),
            Err(Refusal::OutsideDeclaredSubset)
        );
        // An error-recovered tree never yields a registration.
        assert_eq!(
            parse(&body(
                "Register_Routine (T, Loads'Access, \"loads\");\nX := ;\n"
            )),
            Err(Refusal::OutsideDeclaredSubset)
        );
        // A missing final `end` is found at the end of the file, after the
        // pragma a left-to-right parse has already refused.
        assert_eq!(
            parse("package body Q is\nprocedure R is begin null; pragma Ada_83; end R;\n"),
            Err(Refusal::LanguageEditionPragma)
        );
    }

    #[test]
    fn a_global_aspect_mode_is_outside_the_declared_aspect_grammar() {
        assert_eq!(
            parse(&decl_with("procedure Q with Global => in out State;")),
            Err(Refusal::OutsideDeclaredSubset)
        );
        assert_eq!(registrations(&decl_with("procedure Q with Inline;")), 1);
    }

    fn decl_with(declaration: &str) -> String {
        format!(
            "with AUnit.Test_Cases;\npackage body Catalog_Tests is\n{declaration}\n\
             procedure Register_Tests (T : in out Test_Case) is\nbegin\n\
             Register_Routine (T, Loads'Access, \"loads\");\n\
             end Register_Tests;\nend Catalog_Tests;\n"
        )
    }

    #[test]
    fn hostile_inputs_at_the_byte_ceiling_refuse_within_the_bounded_budget() {
        let started = std::time::Instant::now();
        for hostile in [
            // Very deep nesting, flat noise, and an operator chain.
            format!("X := {}1{};\n", "(".repeat(200_000), ")".repeat(200_000)),
            "F (".repeat(200_000),
            format!("X := {};\n", "1 + ".repeat(200_000)),
            "begin\n".repeat(150_000),
        ] {
            let parsed = parse(&body(&hostile));
            assert!(parsed.is_err(), "hostile input must not be admitted");
        }
        assert!(
            started.elapsed() < std::time::Duration::from_secs(20),
            "hostile inputs took {:?}",
            started.elapsed()
        );
    }

    #[test]
    fn ordinary_ada_around_the_anchor_still_parses() {
        let parsed = parse(
            "with AUnit.Assertions;\nwith AUnit.Test_Cases;\nuse AUnit.Assertions;\n\
             package body Catalog_Tests is\n\
             type Test_Case is new AUnit.Test_Cases.Test_Case with null record;\n\
             subtype Small is Natural range 0 .. 10;\n\
             Table : constant array (1 .. 3) of Natural := (1, 2, 3);\n\
             Mask  : constant Natural := 16#FF#;\n\
             Tick  : constant Character := ''';\n\
             Empty : exception;\n\
             function Score (Value : Natural := 0) return Natural is\n\
             begin\n\
                if Value > 10 then\n   return 10;\n   elsif Value in 1 .. 9 then\n\
                   return Value;\n   else\n      return 0;\n   end if;\n\
             end Score;\n\
             procedure Loads (T : in out AUnit.Test_Cases.Test_Case'Class) is\n\
                Total : Natural := 0;\n\
             begin\n\
                for Index in Table'Range loop\n      Total := Total + Table (Index);\n   end loop;\n\
                case Total is\n      when 0 => null;\n      when others => Assert (True, \"loads\");\n\
                end case;\n\
                declare\n      Local : constant Natural := Total;\n   begin\n\
                   Assert (Local > 0, \"positive\");\n   end;\n\
             exception\n   when Empty => null;\n   when others => raise;\n\
             end Loads;\n\
             procedure Register_Tests (T : in out Test_Case) is\n\
             begin\n   Register_Routine (T, Loads'Access, \"loads the catalog\");\n\
             end Register_Tests;\n\
             end Catalog_Tests;\n",
        )
        .expect("parse ordinary Ada");
        assert_eq!(parsed.registrations.len(), 1);
        assert!(parsed.aunit_context_clause);
    }
}
