//! The ADR-0047 declared PHP subset, read off a Tree-sitter PHP tree.
//!
//! This is the second half of the frontend. Declarations come from the pinned
//! Tree-sitter PHP grammar (ADR-0058), with the ADR-0047 declared subset
//! applied over the tree: the lexer in [`super::lexer`] stays the lexical gate
//! and supplies the token stream, and every construct the subset does not
//! admit is refused from the tree or the tokens. Method bodies, parameter
//! lists, property defaults, and constant values are admitted for their
//! extent only, and a syntax error inside them refuses the file (ADR-0058
//! declared divergence `syntax_error_in_skipped_region`).
//!
//! Outside the declared subset the reader abstains for the whole file: it
//! never guesses, never recovers, and never resynchronizes to a later
//! declaration, so no anchor after a refusal can be invented. That is the
//! whole-file abstain-or-admit semantics ADR-0047 D4 requires, and it is what
//! makes a bounded subset acceptable — a gap in the grammar costs recall and
//! can never invent a test method.

use super::lexer::{Refusal, Token, TokenKind};
use crate::ports::parser::ParseError;
use tree_sitter::{Node, Parser};

/// Why the parse stopped. Every variant maps to exactly one registered typed
/// `UNKNOWN` kind; none carries source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Abstention {
    /// The lexer refused the file's bytes.
    Lex(Refusal),
    /// A construct the declared grammar does not admit, including an unclosed
    /// body.
    OutsideDeclaredSubset,
    /// The bounded nesting ceiling was reached.
    DepthLimit,
}

impl Abstention {
    pub(crate) fn unknown_kind(self) -> &'static str {
        match self {
            Self::Lex(Refusal::UnterminatedLiteral) => "unterminated_literal",
            Self::Lex(Refusal::AttributeSpanningLines) => "attribute_spanning_lines",
            Self::Lex(Refusal::UnsupportedByte) => "unadmitted_php_construct",
            Self::OutsideDeclaredSubset => "unadmitted_php_construct",
            Self::DepthLimit => "parser_depth_limit",
        }
    }

    pub(crate) fn diagnostic(self) -> &'static str {
        match self {
            Self::Lex(Refusal::UnterminatedLiteral) => {
                "a PHP string, comment, or heredoc is left open, so every later token boundary is unreliable"
            }
            Self::Lex(Refusal::AttributeSpanningLines) => {
                "a PHP attribute runs past its opening line, where the PHP 7.4 and 8.x readings disagree about token boundaries"
            }
            Self::Lex(Refusal::UnsupportedByte) => {
                "PHP source contains a byte outside the declared subset, so no later declaration in this file was read"
            }
            Self::OutsideDeclaredSubset => {
                "a PHP construct outside the declared subset ends the parse, so the declarations after it are unread"
            }
            Self::DepthLimit => {
                "PHP nesting exceeded the bounded parse depth, so the declarations after it are unread"
            }
        }
    }
}

/// Descent bound. Repository contents are untrusted, and a file of nothing
/// but nested brackets must abstain rather than exhaust the stack.
const MAX_BLOCK_DEPTH: usize = 256;

/// One directly declared method header in a class.
pub(crate) struct DeclaredMethod<'a> {
    pub name: &'a str,
    pub explicit_public: bool,
    pub is_static: bool,
    pub is_abstract: bool,
    pub zero_parameters: bool,
    /// Attribute names on the member, each with whether it carried arguments.
    pub attributes: Vec<(&'a str, bool)>,
    /// The doc comment immediately preceding the declaration, if any.
    pub nearest_doc: Option<&'a str>,
    /// Line bounds of the `function` keyword.
    pub line: (usize, usize),
}

/// One named class declaration in the file.
pub(crate) struct DeclaredClass<'a> {
    pub name: &'a str,
    pub is_abstract: bool,
    /// Last `\`-separated segment of the written `extends` name.
    pub extends_last_segment: Option<&'a str>,
    /// Line bounds of the `class` keyword.
    pub line: (usize, usize),
    pub methods: Vec<DeclaredMethod<'a>>,
}

pub(crate) struct FileParse<'a> {
    pub classes: Vec<DeclaredClass<'a>>,
    /// Any class declared a `use … ;` trait import.
    pub trait_used_in_class: bool,
    pub abstention: Option<Abstention>,
}
/// Read the ADR-0047 declared subset off a Tree-sitter PHP tree (ADR-0058).
///
/// The ADR-0047 lexer has already run and refused every lexical-subset
/// violation, so `tokens` is the opaque-literal token stream of the same
/// bytes. The grammar supplies the declaration structure; the token stream
/// supplies the facts the grammar does not carry (block-comment placement,
/// same-line attributes, the doc comment run before a member, brace depth).
/// As in a left-to-right parse the earliest refusal in source order wins. Any
/// `ERROR` or `MISSING` node refuses the file, but only as the generic
/// unadmitted class and only when no more specific refusal applies, because
/// error recovery decides where an `ERROR` node starts while the declared
/// subset decides which refusal a file is owed.
pub(crate) fn parse<'a>(text: &'a str, tokens: &[Token]) -> Result<FileParse<'a>, ParseError> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .map_err(|error| ParseError::Internal(format!("load PHP grammar: {error}")))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| ParseError::Internal("Tree-sitter PHP parse failed".to_string()))?;
    let root = tree.root_node();
    let mut reader = Reader {
        text,
        tokens,
        refusals: Vec::new(),
        classes: Vec::new(),
        trait_used_in_class: false,
    };
    // A root that is itself an `ERROR` node holds the recovered tokens
    // flat, so none of them is a top-level declaration to judge.
    if root.kind() != "ERROR" {
        reader.program(root);
    }
    if root.has_error() {
        reader.recovered_depth();
    }
    // Stable: at one position the earlier-recorded refusal wins.
    reader.refusals.sort_by_key(|(position, _)| *position);
    let abstention = reader
        .refusals
        .first()
        .map(|(_, abstention)| *abstention)
        .or_else(|| {
            root.has_error()
                .then_some(Abstention::OutsideDeclaredSubset)
        });
    Ok(FileParse {
        classes: reader.classes,
        trait_used_in_class: reader.trait_used_in_class,
        abstention,
    })
}

/// The member prefix: attributes and the doc comment before the declaration.
struct Prefix<'a> {
    attributes: Vec<(&'a str, bool)>,
    nearest_doc: Option<&'a str>,
}

struct Reader<'a, 't> {
    text: &'a str,
    tokens: &'t [Token],
    refusals: Vec<(usize, Abstention)>,
    classes: Vec<DeclaredClass<'a>>,
    trait_used_in_class: bool,
}

fn children<'tree>(node: Node<'tree>) -> Vec<Node<'tree>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).collect()
}

/// A declaration's own start: the first child that is not an attribute list
/// or a comment, which is where the member prefix ends.
fn core_start(node: Node<'_>) -> usize {
    children(node)
        .into_iter()
        .find(|child| !matches!(child.kind(), "attribute_list" | "comment"))
        .map_or(node.start_byte(), |child| child.start_byte())
}

fn ends_with_semicolon(node: Node<'_>) -> bool {
    node.child(node.child_count().saturating_sub(1))
        .is_some_and(|last| last.kind() == ";" && !last.is_missing())
}

impl<'a> Reader<'a, '_> {
    fn refuse(&mut self, position: usize, abstention: Abstention) {
        self.refusals.push((position, abstention));
    }

    /// A node the subset does not admit. An `ERROR` node is the generic
    /// recovery class instead: it carries no position of its own and refuses
    /// the file only when nothing more specific applies.
    fn refuse_unadmitted(&mut self, node: Node<'_>) {
        if node.kind() != "ERROR" {
            self.refuse(node.start_byte(), Abstention::OutsideDeclaredSubset);
        }
    }

    /// Brace depth over a recovered tree. Error recovery can swallow a whole
    /// file into one `ERROR` node, hiding the body whose nesting a
    /// left-to-right parse would have bounded, so the bound is read off the
    /// token stream instead. ponytail: declarations nest at most a class brace
    /// above a body, so a global depth past the bound plus one is a body past
    /// it; a free function at depth zero is off by one, which only changes the
    /// refusal class of an already-refused file.
    fn recovered_depth(&mut self) {
        let mut depth = 0usize;
        for token in self.tokens {
            match token.kind {
                TokenKind::LBrace => {
                    depth += 1;
                    if depth > MAX_BLOCK_DEPTH + 1 {
                        self.refuse(token.start, Abstention::DepthLimit);
                        return;
                    }
                }
                TokenKind::RBrace => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }

    fn program(&mut self, root: Node<'_>) {
        for node in children(root) {
            match node.kind() {
                "php_tag" | "comment" | "empty_statement" => {}
                "declare_statement" => {
                    self.prefix(node);
                    self.directive(node);
                }
                "namespace_definition" => {
                    self.prefix(node);
                    self.namespace(node);
                }
                "namespace_use_declaration" => {
                    self.prefix(node);
                    self.import(node);
                }
                "class_declaration" => {
                    self.prefix(node);
                    self.class(node);
                }
                "function_definition" => {
                    // A free function is admitted for its extent only; it is
                    // not a class member and can never anchor.
                    self.prefix(node);
                    self.function_like(node);
                }
                "const_declaration" => {
                    self.prefix(node);
                }
                _ => {
                    self.prefix(node);
                    self.refuse_unadmitted(node);
                }
            }
        }
    }

    /// The run of block comments and attributes immediately before a
    /// declaration. Every attribute in it must be the bounded uppercase-first
    /// spelling.
    fn prefix(&mut self, node: Node<'_>) -> Prefix<'a> {
        let text = self.text;
        let end = self
            .tokens
            .partition_point(|token| token.start < core_start(node));
        let mut start = end;
        while start > 0
            && matches!(
                self.tokens[start - 1].kind,
                TokenKind::Comment { .. } | TokenKind::Attribute
            )
        {
            start -= 1;
        }
        let mut prefix = Prefix {
            attributes: Vec::new(),
            nearest_doc: None,
        };
        for token in &self.tokens[start..end] {
            match token.kind {
                TokenKind::Comment { doc: true } => prefix.nearest_doc = Some(token.text(text)),
                TokenKind::Attribute => match attribute_name(token.text(text)) {
                    Some(attribute) => prefix.attributes.push(attribute),
                    None => self.refuse(token.start, Abstention::OutsideDeclaredSubset),
                },
                _ => {}
            }
        }
        prefix
    }

    /// Block comments are admitted in prefixes, parameter lists, bodies, and
    /// between declarations only; one inside a declaration header leaves the
    /// subset. `excluded` is a byte range, such as a parameter list, where
    /// they stay admitted.
    fn refuse_header_comments(
        &mut self,
        start: usize,
        end: usize,
        excluded: Option<(usize, usize)>,
    ) {
        let first = self.tokens.partition_point(|token| token.start < start);
        let comment = self.tokens[first..]
            .iter()
            .take_while(|token| token.start < end)
            .find(|token| {
                matches!(token.kind, TokenKind::Comment { .. })
                    && !excluded.is_some_and(|(from, to)| token.start >= from && token.end <= to)
            });
        if let Some(comment) = comment {
            self.refuse(comment.start, Abstention::OutsideDeclaredSubset);
        }
    }

    /// `declare ( … ) ;` — the block and alternative forms are outside the
    /// subset.
    fn directive(&mut self, node: Node<'_>) {
        let parts = children(node);
        let parentheses = parts
            .iter()
            .find(|part| part.kind() == "(")
            .zip(parts.iter().find(|part| part.kind() == ")"))
            .map(|(open, close)| (open.start_byte(), close.end_byte()));
        let admitted = ends_with_semicolon(node)
            && parts.iter().all(|part| {
                matches!(
                    part.kind(),
                    "declare" | "(" | ")" | ";" | "declare_directive" | "comment" | "ERROR"
                )
            });
        if !admitted {
            self.refuse(node.start_byte(), Abstention::OutsideDeclaredSubset);
        }
        self.refuse_header_comments(core_start(node), node.end_byte(), parentheses);
    }

    /// `namespace Name ;` — the braced and unnamed forms are outside the
    /// subset.
    fn namespace(&mut self, node: Node<'_>) {
        let admitted = ends_with_semicolon(node)
            && node.child_by_field_name("name").is_some()
            && node.child_by_field_name("body").is_none();
        if !admitted {
            self.refuse(node.start_byte(), Abstention::OutsideDeclaredSubset);
        }
        self.refuse_header_comments(core_start(node), node.end_byte(), None);
    }

    /// `use [function|const] Name [as alias] ;` — group and comma-separated
    /// imports are outside the subset.
    fn import(&mut self, node: Node<'_>) {
        let clauses = children(node)
            .iter()
            .filter(|part| part.kind() == "namespace_use_clause")
            .count();
        let admitted =
            ends_with_semicolon(node) && clauses == 1 && node.child_by_field_name("body").is_none();
        if !admitted {
            self.refuse(node.start_byte(), Abstention::OutsideDeclaredSubset);
        }
        self.refuse_header_comments(core_start(node), node.end_byte(), None);
    }

    /// `[abstract|final]* class Name [extends Name] [implements Name, …]
    /// { member* }`
    fn class(&mut self, node: Node<'_>) {
        let text = self.text;
        let mut is_abstract = false;
        let mut class_keyword = None;
        let mut extends_last_segment = None;
        for part in children(node) {
            match part.kind() {
                "abstract_modifier" => is_abstract = true,
                "final_modifier"
                | "attribute_list"
                | "comment"
                | "name"
                | "class_interface_clause"
                | "declaration_list" => {}
                "class" if !part.is_named() => class_keyword = Some(part),
                "base_clause" => {
                    let names: Vec<_> = children(part)
                        .into_iter()
                        .filter(|name| name.is_named() && name.kind() != "comment")
                        .collect();
                    // A class extends one name; a list is the interface form.
                    if names.len() > 1 {
                        self.refuse(part.start_byte(), Abstention::OutsideDeclaredSubset);
                    }
                    extends_last_segment = names.last().map(|name| {
                        let written = &text[name.byte_range()];
                        written.rsplit('\\').next().unwrap_or(written).trim()
                    });
                }
                // Readonly, static, var, and visibility modifiers, and any
                // recovered node, are outside the subset.
                _ => self.refuse_unadmitted(part),
            }
        }
        let (Some(keyword), Some(name), Some(body)) = (
            class_keyword,
            node.child_by_field_name("name"),
            node.child_by_field_name("body"),
        ) else {
            self.refuse(node.start_byte(), Abstention::OutsideDeclaredSubset);
            return;
        };
        self.refuse_header_comments(core_start(node), body.start_byte(), None);
        let mut methods = Vec::new();
        for member in children(body) {
            match member.kind() {
                "{" | "}" | "comment" => {}
                "use_declaration" => {
                    // A trait import. Trait-supplied methods are invisible
                    // here, so this can only understate support.
                    self.prefix(member);
                    self.trait_used_in_class = true;
                    if children(member)
                        .iter()
                        .any(|part| part.kind() == "use_list")
                    {
                        self.refuse(member.start_byte(), Abstention::OutsideDeclaredSubset);
                    }
                    self.refuse_header_comments(core_start(member), member.end_byte(), None);
                }
                "const_declaration" => {
                    self.prefix(member);
                }
                "property_declaration" => {
                    self.prefix(member);
                    // A hook list is a brace group in a property, which the
                    // subset's property extent does not admit.
                    if let Some(hooks) = children(member)
                        .into_iter()
                        .find(|part| part.kind() == "property_hook_list")
                    {
                        self.refuse(hooks.start_byte(), Abstention::OutsideDeclaredSubset);
                    }
                }
                "method_declaration" => {
                    if let Some(method) = self.function_like(member) {
                        methods.push(method);
                    }
                }
                _ => self.refuse_unadmitted(member),
            }
        }
        self.classes.push(DeclaredClass {
            name: &text[name.byte_range()],
            is_abstract,
            extends_last_segment,
            line: line_bounds(text, keyword.start_byte()),
            methods,
        });
    }

    /// `[modifier]* function [&]? name ( params ) [: bounded-type]
    /// ( ; | { body } )`, for a class method or a free function.
    fn function_like(&mut self, node: Node<'_>) -> Option<DeclaredMethod<'a>> {
        let text = self.text;
        let prefix = self.prefix(node);
        let mut explicit_public = false;
        let mut is_static = false;
        let mut is_abstract = false;
        let mut unread_modifier = false;
        let mut function_keyword = None;
        for part in children(node) {
            match part.kind() {
                "visibility_modifier" => {
                    explicit_public |= text[part.byte_range()].eq_ignore_ascii_case("public");
                }
                "static_modifier" => is_static = true,
                "abstract_modifier" => is_abstract = true,
                "readonly_modifier" | "var_modifier" => unread_modifier = true,
                "function" if !part.is_named() => function_keyword = Some(part),
                _ => {}
            }
        }
        let (Some(keyword), Some(name), Some(parameters)) = (
            function_keyword,
            node.child_by_field_name("name"),
            node.child_by_field_name("parameters"),
        ) else {
            self.refuse(node.start_byte(), Abstention::OutsideDeclaredSubset);
            return None;
        };
        let body = node.child_by_field_name("body");
        // The member reader treats a member whose modifier run is broken by a
        // block comment or an unread modifier as a property extent, which
        // ends at the first `;` and cannot contain a body brace.
        let modifier_start = self
            .tokens
            .partition_point(|token| token.start < core_start(node));
        let broken_modifier_run = unread_modifier
            || self.tokens[modifier_start..]
                .iter()
                .take_while(|token| token.end <= keyword.start_byte())
                .any(|token| matches!(token.kind, TokenKind::Comment { .. }));
        if broken_modifier_run {
            if let Some(body) = body {
                self.refuse(body.start_byte(), Abstention::OutsideDeclaredSubset);
            }
            return None;
        }
        let header_end = body.map_or(node.end_byte(), |body| body.start_byte());
        self.refuse_header_comments(
            keyword.start_byte(),
            header_end,
            Some((parameters.start_byte(), parameters.end_byte())),
        );
        if let Some(return_type) = node.child_by_field_name("return_type") {
            // The bounded return-type vocabulary: an optional `?` and one
            // name. Union, intersection, and DNF types are outside the subset.
            if !matches!(
                return_type.kind(),
                "named_type" | "primitive_type" | "optional_type" | "bottom_type"
            ) {
                self.refuse(return_type.start_byte(), Abstention::OutsideDeclaredSubset);
            }
        }
        // An abstract or interface-shaped declaration has no body. A
        // declaration with neither a body nor `;` is an error-recovery
        // artifact, which the generic recovery refusal covers.
        if let Some(body) = body {
            self.body_depth(body);
        }
        let first_inner = self
            .tokens
            .partition_point(|token| token.start <= parameters.start_byte());
        let zero_parameters = self.tokens[first_inner..]
            .iter()
            .take_while(|token| token.end < parameters.end_byte())
            .all(|token| matches!(token.kind, TokenKind::Comment { .. }));
        Some(DeclaredMethod {
            name: &text[name.byte_range()],
            explicit_public,
            is_static,
            is_abstract,
            zero_parameters,
            attributes: prefix.attributes,
            nearest_doc: prefix.nearest_doc,
            line: line_bounds(text, keyword.start_byte()),
        })
    }

    /// Brace nesting inside a body, counted over the token stream where every
    /// brace-bearing literal is already one token. Repository contents are
    /// untrusted, so a body past the bound abstains.
    fn body_depth(&mut self, body: Node<'_>) {
        let first = self
            .tokens
            .partition_point(|token| token.start < body.start_byte());
        let mut depth = 0usize;
        for token in self.tokens[first..]
            .iter()
            .take_while(|token| token.start < body.end_byte())
        {
            match token.kind {
                TokenKind::LBrace => {
                    depth += 1;
                    if depth > MAX_BLOCK_DEPTH {
                        self.refuse(token.start, Abstention::DepthLimit);
                        return;
                    }
                }
                TokenKind::RBrace => depth = depth.saturating_sub(1),
                _ => {}
            }
        }
    }
}

/// The name and argument shape of a same-line attribute token. `None` when
/// the content is not the bounded uppercase-first attribute spelling.
fn attribute_name(text: &str) -> Option<(&str, bool)> {
    let inner = text
        .strip_prefix("#[")
        .and_then(|rest| rest.strip_suffix(']'))?;
    let body = inner.trim();
    let name_end = body
        .find(|byte: char| !(byte.is_ascii_alphanumeric() || byte == '_'))
        .unwrap_or(body.len());
    let (name, rest) = body.split_at(name_end);
    let first = name.as_bytes().first()?;
    if !first.is_ascii_uppercase() {
        return None;
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    let rest = rest.trim();
    let has_arguments = match rest {
        "" => false,
        _ => rest.starts_with('(') && rest.ends_with(')'),
    };
    if !has_arguments && !rest.is_empty() {
        return None;
    }
    Some((name, has_arguments))
}

/// The anchor range is the physical line the declaration keyword sits on.
fn line_bounds(text: &str, offset: usize) -> (usize, usize) {
    let bytes = text.as_bytes();
    let start = bytes[..offset]
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |position| position + 1);
    let end = bytes[offset..]
        .iter()
        .position(|byte| *byte == b'\n')
        .map_or(text.len(), |position| offset + position + 1);
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::parsing::php::lexer;

    fn parsed(text: &str) -> FileParse<'_> {
        let start = lexer::prologue_code_start(text).expect("admitted prologue");
        let mut tokens = lexer::lex(&text[start..]).expect("lex");
        for token in &mut tokens {
            token.start += start;
            token.end += start;
        }
        parse(text, &tokens).expect("tree-sitter parse")
    }

    fn admitted(text: &str) -> FileParse<'_> {
        let parse = parsed(text);
        assert!(parse.abstention.is_none(), "{:?}", parse.abstention);
        parse
    }

    fn refused(text: &str) -> Abstention {
        let start = lexer::prologue_code_start(text).expect("admitted prologue");
        let lexed = match lexer::lex(&text[start..]) {
            Ok(mut tokens) => {
                for token in &mut tokens {
                    token.start += start;
                    token.end += start;
                }
                let parse = parse(text, &tokens).expect("tree-sitter parse");
                return parse.abstention.expect("must refuse");
            }
            Err(refusal) => Abstention::Lex(refusal),
        };
        lexed
    }

    #[test]
    fn a_declared_class_with_methods_parses_with_shapes() {
        let parse = admitted(
            "<?php\n\ndeclare(strict_types=1);\n\nnamespace App\\Tests;\n\nuse PHPUnit\\Framework\\TestCase;\n\nfinal class CatalogTest extends TestCase\n{\n    /** @test */\n    public function loadsTheCatalog(): void\n    {\n        self::assertTrue(true);\n    }\n\n    #[Test]\n    public function sortsEntries(): void\n    {\n    }\n}\n",
        );
        assert_eq!(parse.classes.len(), 1);
        let class = &parse.classes[0];
        assert_eq!(class.name, "CatalogTest");
        assert!(!class.is_abstract);
        assert_eq!(class.extends_last_segment, Some("TestCase"));
        assert_eq!(class.methods.len(), 2);
        assert!(class.methods[0].explicit_public);
        assert!(class.methods[0].zero_parameters);
        assert!(class.methods[0]
            .nearest_doc
            .is_some_and(|doc| doc.contains("@test")));
        assert_eq!(class.methods[1].attributes, vec![("Test", false)]);
    }

    #[test]
    fn an_in_file_base_class_chain_is_visible() {
        let parse = admitted(
            "<?php\n\nclass InvoiceTestCase extends TestCase\n{\n    protected function makeInvoice(): Invoice\n    {\n    }\n}\n\nfinal class InvoiceAttributeTest extends InvoiceTestCase\n{\n    public function totalsNetAmount(): void\n    {\n    }\n}\n",
        );
        assert_eq!(parse.classes.len(), 2);
        assert_eq!(parse.classes[0].extends_last_segment, Some("TestCase"));
        assert_eq!(
            parse.classes[1].extends_last_segment,
            Some("InvoiceTestCase")
        );
        // The protected helper is parsed but carries no marker.
        assert!(!parse.classes[0].methods[0].explicit_public);
    }

    #[test]
    fn constructs_outside_the_declared_subset_refuse_whole_file() {
        for source in [
            // Group use.
            "<?php\nuse PHPUnit\\Framework\\{TestCase, Attributes\\Test};\nclass A extends TestCase {}\n",
            // Braced namespace.
            "<?php\nnamespace App {\n}\n",
            // Union return type.
            "<?php\nclass A extends TestCase\n{\n    public function testUnion(): int|string\n    {\n    }\n}\n",
            // Enum declaration.
            "<?php\nenum Suit {\n    case Hearts;\n}\n",
            // Interface declaration.
            "<?php\ninterface Runnable {}\n",
            // Top-level conditional code.
            "<?php\nif (PHP_VERSION_ID >= 80100) {\n    class A extends TestCase {}\n}\n",
            // Lowercase attribute name.
            "<?php\n#[test]\nclass A extends TestCase {}\n",
            // Unclosed body.
            "<?php\nclass A extends TestCase\n{\n    public function testOpen(): void\n    {\n",
        ] {
            assert_eq!(
                refused(source),
                Abstention::OutsideDeclaredSubset,
                "must refuse: {source}"
            );
        }
    }

    #[test]
    fn a_close_tag_is_refused_by_the_lexer_and_never_reaches_the_grammar() {
        assert_eq!(
            refused("<?php\nclass A extends TestCase {}\n?>\n<p>html</p>\n"),
            Abstention::Lex(lexer::Refusal::UnsupportedByte)
        );
    }

    #[test]
    fn bodies_are_skipped_exactly_across_literals_and_heredocs() {
        let parse = admitted(
            "<?php\nclass WeirdTest extends TestCase\n{\n    public function testWeird(): void\n    {\n        $a = 'not } a brace';\n        $b = \"interp {$c->m()} end\";\n        $d = <<<EOT\nheredoc } body #[Test]\nEOT;\n        if ($a === $b) {\n            echo \"} } {\";\n        }\n    }\n}\n",
        );
        assert_eq!(parse.classes[0].methods.len(), 1);
    }

    #[test]
    fn depth_nesting_past_the_bound_abstains() {
        let body = format!(
            "<?php\nclass A extends TestCase\n{{\n    public function testDeep(): void\n    {{\n        {}\n    }}\n}}\n",
            "{".repeat(512)
        );
        assert_eq!(refused(&body), Abstention::DepthLimit);
    }
}
