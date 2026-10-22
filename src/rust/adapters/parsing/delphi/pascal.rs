//! Bounded Object Pascal lexer and declaration reader.
//!
//! The hand-written lexer is the lexical-subset gate: it classifies comments,
//! strings and compiler directives and tracks conditional-compilation depth.
//! Declarations are read off the pinned `tree-sitter-pascal` tree (ADR-0058),
//! so the extent of a class body comes from the grammar rather than from line
//! position. That is what lets a nested type inside a
//! fixture close its own `end` without ending the fixture -- the false negative
//! ADR-0044 D2 had to record while the frontend was line-oriented.
//!
//! Types are deliberately *not* interpreted. A field's or property's type is
//! consumed as an opaque token run up to its terminator. The declared subset is
//! the declaration structure, because that is all the admitted anchor needs and
//! anything more would be a claim this frontend does not make.
//!
//! It abstains rather than recovers. When a construct inside a class body is not
//! in the subset, the whole class yields no anchor and says so; the parser never
//! resynchronizes into an anchor it did not understand.

/// What the lexer learned about the file as a whole.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LexFlags {
    pub unterminated_block_comment: bool,
    /// Byte offset of the comment opener that never closes. Everything from
    /// here to end of file is comment, so the grammar parses only the prefix.
    pub unterminated_comment_start: Option<usize>,
    /// `{$MODE}` / `{$MODESWITCH}` re-selects the dialect, which is exactly what
    /// this frontend does not evaluate.
    pub dialect_selector: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident,
    Number,
    Str,
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
    /// Conditional-compilation depth at this token. A declaration at depth
    /// greater than zero is selected by a define this frontend cannot evaluate.
    pub conditional_depth: usize,
}

/// The compiler directives that decide what may be claimed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Directive {
    ConditionalOpen,
    ConditionalClose,
    DialectSelector,
    Other,
}

fn classify_directive(body: &str) -> Directive {
    let name = body
        .trim_start()
        .split(|character: char| character.is_whitespace() || character == '}')
        .next()
        .unwrap_or("")
        .trim_end_matches(['+', '-'])
        .to_ascii_uppercase();
    match name.as_str() {
        "IFDEF" | "IFNDEF" | "IF" | "IFOPT" => Directive::ConditionalOpen,
        "ENDIF" | "IFEND" => Directive::ConditionalClose,
        "MODE" | "MODESWITCH" => Directive::DialectSelector,
        _ => Directive::Other,
    }
}

/// Object Pascal separates its delimiters completely -- `//`, `{ }`, `(* *)`
/// for comments and single quotes for strings, escaping by doubling -- so no
/// character serves two purposes and the token boundaries are exact.
pub(crate) fn lex(text: &str) -> (Vec<Token>, LexFlags) {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut flags = LexFlags::default();
    let mut conditional_depth = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        let byte = bytes[index];
        if byte.is_ascii_whitespace() {
            index += 1;
            continue;
        }
        if byte == b'/' && bytes.get(index + 1) == Some(&b'/') {
            while index < bytes.len() && bytes[index] != b'\n' {
                index += 1;
            }
            continue;
        }
        if byte == b'{' {
            let body_start = index + 1;
            let close = text[body_start..]
                .find('}')
                .map(|offset| body_start + offset);
            if bytes.get(body_start) == Some(&b'$') {
                let body_end = close.unwrap_or(bytes.len());
                match classify_directive(&text[body_start + 1..body_end]) {
                    Directive::ConditionalOpen => conditional_depth += 1,
                    Directive::ConditionalClose => {
                        conditional_depth = conditional_depth.saturating_sub(1)
                    }
                    Directive::DialectSelector => flags.dialect_selector = true,
                    Directive::Other => {}
                }
            }
            match close {
                Some(end) => index = end + 1,
                None => {
                    flags.unterminated_block_comment = true;
                    flags.unterminated_comment_start = Some(index);
                    index = bytes.len();
                }
            }
            continue;
        }
        if byte == b'(' && bytes.get(index + 1) == Some(&b'*') {
            match text[index + 2..].find("*)") {
                Some(offset) => index = index + 2 + offset + 2,
                None => {
                    flags.unterminated_block_comment = true;
                    flags.unterminated_comment_start = Some(index);
                    index = bytes.len();
                }
            }
            continue;
        }
        if byte == b'\'' {
            let start = index;
            index += 1;
            loop {
                match bytes.get(index) {
                    Some(b'\'') if bytes.get(index + 1) == Some(&b'\'') => index += 2,
                    Some(b'\'') => {
                        index += 1;
                        break;
                    }
                    Some(_) => index += 1,
                    None => break,
                }
            }
            tokens.push(Token {
                kind: TokenKind::Str,
                start,
                end: index,
                conditional_depth,
            });
            continue;
        }
        if byte.is_ascii_alphabetic() || byte == b'_' {
            let start = index;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Ident,
                start,
                end: index,
                conditional_depth,
            });
            continue;
        }
        if byte.is_ascii_digit() || byte == b'$' {
            let start = index;
            index += 1;
            while index < bytes.len()
                && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'.')
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                start,
                end: index,
                conditional_depth,
            });
            continue;
        }
        tokens.push(Token {
            kind: TokenKind::Punct,
            start: index,
            end: index + 1,
            conditional_depth,
        });
        index += 1;
    }

    (tokens, flags)
}

use crate::ports::parser::ParseError;
use tree_sitter::{Node, Parser};

/// A `[TestFixture]` class declaration and the admitted test procedures the
/// grammar found inside its body.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct FixtureDecl {
    pub start: usize,
    pub end: usize,
    pub tests: Vec<TestDecl>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TestDecl {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct UnitParse {
    pub uses_dunitx: bool,
    pub fixtures: Vec<FixtureDecl>,
    /// A DUnitX attribute spelling appears without the import that binds it.
    pub attributed_without_uses: bool,
    /// An admitted shape sits inside conditional compilation and was not
    /// anchored, because the define that selects it is unevaluated.
    pub skipped_conditional: bool,
    /// A class body contained a construct outside the declared subset, so that
    /// class yielded no anchor.
    pub abstained: bool,
}

const DUNITX_UNIT: &str = "dunitx.testframework";

/// Reads the admitted declarations off the Tree-sitter tree. The lexer's
/// token stream stays the source of the `uses` binding, the attribute
/// spelling scan and the conditional-compilation depth, because those are
/// lexical facts the grammar does not carry.
pub(crate) fn parse_unit(
    text: &str,
    tokens: &[Token],
    flags: &LexFlags,
) -> Result<UnitParse, ParseError> {
    let view = TokenView { text, tokens };
    let mut out = UnitParse {
        uses_dunitx: view.scan_uses_clauses(),
        ..UnitParse::default()
    };
    // An unterminated comment swallows the rest of the file, exactly as the
    // lexer read it, so the grammar sees only the prefix.
    let parsed_text = flags
        .unterminated_comment_start
        .map_or(text, |start| &text[..start]);
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_pascal::LANGUAGE.into())
        .map_err(|error| ParseError::Internal(format!("load Pascal grammar: {error}")))?;
    let tree = parser
        .parse(parsed_text, None)
        .ok_or_else(|| ParseError::Internal("Tree-sitter Pascal parse failed".to_string()))?;
    let root = tree.root_node();

    let section_limit =
        first_of_kind(root, "implementation").map_or(usize::MAX, |node| node.start_byte());
    let mut declarations = Vec::new();
    collect_type_declarations(root, section_limit, &mut declarations);

    // An ERROR or MISSING node inside a class declaration abstains that class,
    // the way the legacy parser abstained on a construct outside the subset.
    // Anywhere else it refuses the whole file (ADR-0058). An error whose span
    // holds a token under conditional compilation is the more specific
    // refusal: the define that selects the branch is unevaluated.
    let truncated = flags.unterminated_comment_start.is_some();
    let mut class_error = vec![None::<ErrorCause>; declarations.len()];
    let mut file_error = None::<ErrorCause>;
    for (start, end) in error_spans(root) {
        let owner = declarations.iter().position(|declaration| {
            class_node(*declaration)
                .is_some_and(|class| class.start_byte() <= start && start < class.end_byte())
        });
        let cause = if view.conditional_between(start, end) {
            ErrorCause::Conditional
        } else {
            ErrorCause::Syntax
        };
        let slot = match owner {
            Some(index) => &mut class_error[index],
            // The prefix of a file cut by an unterminated comment is
            // incomplete by construction, so only class-level errors count.
            None if truncated => continue,
            None => &mut file_error,
        };
        *slot = Some(slot.map_or(cause, |known| known.max(cause)));
    }

    if let Some(cause) = file_error {
        cause.record(&mut out);
    } else {
        for (declaration, cause) in declarations.iter().zip(class_error) {
            read_declaration(&view, *declaration, cause, &mut out);
        }
    }
    if !out.uses_dunitx {
        out.attributed_without_uses = view.any_dunitx_attribute_spelling();
        out.fixtures.clear();
    }
    Ok(out)
}

fn first_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = first_of_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

/// Every `declType` of a type section outside a class body and before the
/// `implementation` section. Nested types are read by their enclosing class.
fn collect_type_declarations<'t>(node: Node<'t>, limit: usize, out: &mut Vec<Node<'t>>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "declClass" => {}
            "declTypes" if child.start_byte() < limit => {
                let mut inner = child.walk();
                out.extend(
                    child
                        .children(&mut inner)
                        .filter(|member| member.kind() == "declType"),
                );
            }
            _ => collect_type_declarations(child, limit, out),
        }
    }
}

fn class_node(declaration: Node<'_>) -> Option<Node<'_>> {
    declaration
        .child_by_field_name("type")
        .filter(|node| node.kind() == "declClass")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum ErrorCause {
    Syntax,
    Conditional,
}

impl ErrorCause {
    fn record(self, out: &mut UnitParse) {
        match self {
            Self::Syntax => out.abstained = true,
            Self::Conditional => out.skipped_conditional = true,
        }
    }
}

/// Byte span of every `ERROR` and `MISSING` node. A `uses` clause is read by
/// the lexer, so the grammar's own gaps there (`uses A in 'a.pas'`) are not
/// errors of the declarations this frontend reads.
fn error_spans(root: Node<'_>) -> Vec<(usize, usize)> {
    fn walk(node: Node<'_>, out: &mut Vec<(usize, usize)>) {
        if node.kind() == "declUses" {
            return;
        }
        if node.is_error() || node.is_missing() {
            out.push((node.start_byte(), node.end_byte()));
        }
        if !node.has_error() {
            return;
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, out);
        }
    }
    let mut out = Vec::new();
    walk(root, &mut out);
    out
}

fn read_declaration(
    view: &TokenView<'_>,
    declaration: Node<'_>,
    class_error: Option<ErrorCause>,
    out: &mut UnitParse,
) {
    let Some(class) = class_node(declaration) else {
        return;
    };
    let mut cursor = class.walk();
    let children: Vec<Node<'_>> = class.children(&mut cursor).collect();
    let Some(keyword) = children.iter().find(|child| child.kind() == "kClass") else {
        return;
    };
    let parent_open = children.iter().find(|child| child.kind() == "(");
    if !children.iter().any(|child| child.kind() == "kEnd") {
        // `class;` is a forward declaration and opens no body, but a forward
        // declaration with an ancestor list is outside the declared subset.
        out.abstained |= parent_open.is_some();
        if let Some(cause) = class_error {
            cause.record(out);
        }
        return;
    }
    if let Some(cause) = class_error {
        cause.record(out);
        return;
    }
    let attributes = view.attributes(declaration);
    if !attributes
        .iter()
        .any(|attribute| attribute.name == "testfixture")
    {
        return;
    }
    // The fixture's header ends at the ancestor list only when the list
    // directly follows `class`.
    let header_end = match (
        parent_open,
        children.iter().find(|child| child.kind() == ")"),
    ) {
        (Some(open), Some(close)) if keyword.next_sibling() == Some(*open) => close.end_byte(),
        _ => keyword.end_byte(),
    };
    if view.depth_at(keyword.start_byte()) > 0 {
        out.skipped_conditional = true;
        return;
    }
    let decl_start = attributes
        .first()
        .map(|attribute| attribute.start)
        .or_else(|| {
            declaration
                .child_by_field_name("name")
                .map(|name| name.start_byte())
        })
        .unwrap_or(declaration.start_byte());
    let mut tests = Vec::new();
    for member in &children {
        match member.kind() {
            "declProc" => read_method(view, *member, out, &mut tests),
            "declSection" => {
                let mut inner = member.walk();
                for entry in member.children(&mut inner) {
                    if entry.kind() == "declProc" {
                        read_method(view, entry, out, &mut tests);
                    }
                }
            }
            _ => {}
        }
    }
    out.fixtures.push(FixtureDecl {
        start: decl_start,
        end: header_end,
        tests,
    });
}

/// An attributed `procedure` member. Functions, constructors and destructors
/// are members but never tests.
fn read_method(
    view: &TokenView<'_>,
    method: Node<'_>,
    out: &mut UnitParse,
    tests: &mut Vec<TestDecl>,
) {
    let mut cursor = method.walk();
    let children: Vec<Node<'_>> = method.children(&mut cursor).collect();
    let Some(keyword) = children.iter().find(|child| child.kind() == "kProcedure") else {
        return;
    };
    let attributes = view.attributes(method);
    if !attributes.iter().any(|attribute| attribute.name == "test") {
        return;
    }
    let Some(semicolon) = children.iter().find(|child| child.kind() == ";") else {
        return;
    };
    if attributes.iter().any(|attribute| attribute.conditional)
        || view.depth_at(keyword.start_byte()) > 0
    {
        out.skipped_conditional = true;
        return;
    }
    tests.push(TestDecl {
        start: attributes
            .first()
            .map_or(keyword.start_byte(), |attribute| attribute.start),
        end: semicolon.end_byte(),
    });
}

struct AttributeRef {
    name: String,
    start: usize,
    conditional: bool,
}

/// Lexical facts the grammar does not carry, read from the token stream.
struct TokenView<'a> {
    text: &'a str,
    tokens: &'a [Token],
}

impl TokenView<'_> {
    /// The `[...]` groups of a declaration's `rttiAttributes`, in source order.
    fn attributes(&self, declaration: Node<'_>) -> Vec<AttributeRef> {
        let mut cursor = declaration.walk();
        let Some(list) = declaration
            .children(&mut cursor)
            .find(|child| child.kind() == "rttiAttributes")
        else {
            return Vec::new();
        };
        let mut inner = list.walk();
        list.children(&mut inner)
            .filter(|child| child.kind() == "[")
            .filter_map(|open| {
                let index = self.token_index(open.start_byte())?;
                Some(AttributeRef {
                    name: self.attribute_name_at(index)?,
                    start: open.start_byte(),
                    conditional: self.tokens[index].conditional_depth > 0,
                })
            })
            .collect()
    }

    fn token_index(&self, start: usize) -> Option<usize> {
        self.tokens
            .binary_search_by_key(&start, |token| token.start)
            .ok()
    }

    /// True when a token inside `start..end`, or just before an empty span,
    /// sits under conditional compilation.
    fn conditional_between(&self, start: usize, end: usize) -> bool {
        let first = self.tokens.partition_point(|token| token.end <= start);
        let inside = self.tokens[first..]
            .iter()
            .take_while(|token| token.start < end.max(start + 1))
            .any(|token| token.conditional_depth > 0);
        let before = start == end
            && first
                .checked_sub(1)
                .is_some_and(|index| self.tokens[index].conditional_depth > 0);
        inside || before
    }

    fn depth_at(&self, start: usize) -> usize {
        self.token_index(start)
            .map_or(0, |index| self.tokens[index].conditional_depth)
    }

    /// `[Name]` or `[Name(...)]`, lowercased. A qualified `[Unit.Name]` keeps
    /// only its last selector, the way the attribute resolves.
    fn attribute_name_at(&self, open: usize) -> Option<String> {
        let mut last = None;
        for index in open + 1..self.tokens.len() {
            if self.tokens[index].kind == TokenKind::Ident {
                last = Some(self.lower(index));
            } else if !self.punct_is(index, b'.') {
                break;
            }
        }
        last
    }

    /// A `uses` clause naming `DUnitX.TestFramework`. The dialect evidence is
    /// this import, never the file suffix, which ADR-0032 forbids as an oracle.
    fn scan_uses_clauses(&self) -> bool {
        let mut index = 0usize;
        while index < self.tokens.len() {
            if !self.ident_is(index, "uses") {
                index += 1;
                continue;
            }
            index += 1;
            let mut qualified = String::new();
            while index < self.tokens.len() {
                match self.tokens[index].kind {
                    TokenKind::Ident => qualified.push_str(&self.lower(index)),
                    TokenKind::Punct if self.punct_is(index, b'.') => qualified.push('.'),
                    TokenKind::Punct if self.punct_is(index, b',') => qualified.clear(),
                    TokenKind::Punct if self.punct_is(index, b';') => {
                        index += 1;
                        break;
                    }
                    _ => qualified.clear(),
                }
                if qualified == DUNITX_UNIT {
                    return true;
                }
                index += 1;
            }
        }
        false
    }

    fn any_dunitx_attribute_spelling(&self) -> bool {
        (0..self.tokens.len()).any(|index| {
            self.punct_is(index, b'[')
                && self
                    .attribute_name_at(index)
                    .is_some_and(|name| name == "testfixture" || name == "test")
        })
    }

    fn ident_is(&self, index: usize, word: &str) -> bool {
        self.tokens[index].kind == TokenKind::Ident && self.lower(index) == word
    }

    fn punct_is(&self, index: usize, byte: u8) -> bool {
        let token = self.tokens[index];
        token.kind == TokenKind::Punct && self.text.as_bytes().get(token.start) == Some(&byte)
    }

    /// Object Pascal is case-insensitive, so every keyword and attribute
    /// comparison is made on a lowercased slice.
    fn lower(&self, index: usize) -> String {
        let token = self.tokens[index];
        self.text[token.start..token.end].to_ascii_lowercase()
    }
}
