//! Bounded recursive-descent parser for the ADR-0047 declared PHP subset.
//!
//! This is the second half of the frontend. It descends
//! `prologue` -> top-level declaration -> class -> member -> method header,
//! and it accepts or refuses each construct against the declared grammar.
//! Method bodies are skipped by brace balancing over the token stream, which
//! is exact because the lexer has already consumed strings, comments,
//! heredocs, and attributes as single opaque tokens — no brace inside any of
//! those can masquerade as structure.
//!
//! Outside the declared subset the parser abstains for the whole file: it
//! never guesses, never recovers, and never resynchronizes to a later
//! declaration, so no anchor after a refusal can be invented. That is the
//! whole-file abstain-or-admit semantics ADR-0047 D4 requires, and it is what
//! makes a bounded subset acceptable — a gap in the grammar costs recall and
//! can never invent a test method.

use super::lexer::{Refusal, Token, TokenKind};

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

pub(crate) fn parse<'a>(text: &'a str, tokens: &[Token]) -> FileParse<'a> {
    let mut parser = Parser {
        text,
        tokens,
        index: 0,
        depth: 0,
        classes: Vec::new(),
        trait_used_in_class: false,
    };
    let abstention = parser.parse_program();
    FileParse {
        classes: parser.classes,
        trait_used_in_class: parser.trait_used_in_class,
        abstention,
    }
}

type Step = Result<(), Abstention>;

struct Parser<'a, 'b> {
    text: &'a str,
    tokens: &'b [Token],
    index: usize,
    depth: usize,
    classes: Vec<DeclaredClass<'a>>,
    trait_used_in_class: bool,
}

/// The member prefix: attributes and comments before the declaration keyword.
struct Prefix<'a> {
    attributes: Vec<(&'a str, bool)>,
    comments: Vec<Token>,
}

impl<'a, 'b> Parser<'a, 'b> {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.index)
    }

    fn peek_at(&self, offset: usize) -> Option<&Token> {
        self.tokens.get(self.index + offset)
    }

    fn text(&self, token: &Token) -> &'a str {
        token.text(self.text)
    }

    /// True when the token is the given keyword, case-insensitively. PHP
    /// keywords are case-insensitive, and reading them that way changes no
    /// token boundary.
    fn is_word(&self, token: &Token, word: &str) -> bool {
        token.kind == TokenKind::Identifier && self.text(token).eq_ignore_ascii_case(word)
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.peek().is_some_and(|token| token.kind == kind) {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, Abstention> {
        match self.tokens.get(self.index) {
            Some(token) if token.kind == kind => {
                self.index += 1;
                Ok(*token)
            }
            _ => Err(Abstention::OutsideDeclaredSubset),
        }
    }

    fn skip_separators(&mut self) {
        while self
            .peek()
            .is_some_and(|token| matches!(token.kind, TokenKind::Comment { .. }))
        {
            self.index += 1;
        }
    }

    fn enter(&mut self) -> Result<(), Abstention> {
        self.depth += 1;
        if self.depth > MAX_BLOCK_DEPTH {
            return Err(Abstention::DepthLimit);
        }
        Ok(())
    }

    fn leave(&mut self) {
        self.depth -= 1;
    }

    fn parse_program(&mut self) -> Option<Abstention> {
        let result = self.top_level();
        match result {
            Ok(()) if self.index == self.tokens.len() => None,
            Ok(()) => Some(Abstention::OutsideDeclaredSubset),
            Err(abstention) => Some(abstention),
        }
    }

    fn top_level(&mut self) -> Step {
        loop {
            self.skip_separators();
            if self.eat(TokenKind::Semicolon) {
                continue;
            }
            let Some(token) = self.peek().copied() else {
                return Ok(());
            };
            if token.kind == TokenKind::Attribute {
                // A top-level attribute prefixes a class or function
                // declaration; parse_member_prefix consumes it.
                self.parse_member_prefix()?;
                continue;
            }
            if token.kind != TokenKind::Identifier {
                return Err(Abstention::OutsideDeclaredSubset);
            }
            if self.is_word(&token, "declare") {
                self.parse_directive()?;
                continue;
            }
            if self.is_word(&token, "namespace") {
                self.parse_namespace()?;
                continue;
            }
            if self.is_word(&token, "use") {
                self.parse_import()?;
                continue;
            }
            if self.is_word(&token, "abstract")
                || self.is_word(&token, "final")
                || self.is_word(&token, "class")
            {
                self.parse_class()?;
                continue;
            }
            if self.is_word(&token, "function") {
                // A free function is admitted for its extent only; it is not a
                // class member and can never anchor.
                self.parse_function(&Prefix {
                    attributes: Vec::new(),
                    comments: Vec::new(),
                })?;
                continue;
            }
            if self.is_word(&token, "const") {
                self.skip_to_semicolon()?;
                continue;
            }
            return Err(Abstention::OutsideDeclaredSubset);
        }
    }

    /// `declare ( … ) ;` — the block form is outside the subset.
    fn parse_directive(&mut self) -> Step {
        self.index += 1;
        self.expect(TokenKind::LParen)?;
        self.skip_group(TokenKind::LParen, TokenKind::RParen)?;
        self.expect(TokenKind::Semicolon)?;
        Ok(())
    }

    /// `namespace Name ;` — the braced form is outside the subset.
    fn parse_namespace(&mut self) -> Step {
        self.index += 1;
        self.parse_qualified_name()?;
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::LBrace)
        {
            return Err(Abstention::OutsideDeclaredSubset);
        }
        self.expect(TokenKind::Semicolon)?;
        Ok(())
    }

    /// `use [function|const] Name [as alias] ;` — group use is outside the
    /// subset.
    fn parse_import(&mut self) -> Step {
        self.index += 1;
        if self
            .peek()
            .is_some_and(|token| self.is_word(token, "function") || self.is_word(token, "const"))
            && self
                .peek_at(1)
                .is_some_and(|token| token.kind == TokenKind::Identifier)
        {
            self.index += 1;
        }
        self.parse_qualified_name()?;
        if self.peek().is_some_and(|token| self.is_word(token, "as")) {
            self.index += 1;
            self.expect(TokenKind::Identifier)?;
        }
        if self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::LBrace)
        {
            return Err(Abstention::OutsideDeclaredSubset);
        }
        self.expect(TokenKind::Semicolon)?;
        Ok(())
    }

    /// `[ #[…] … ]* [abstract|final]* class Name [extends Name]
    /// [implements Name, …] { member* }`
    fn parse_class(&mut self) -> Step {
        self.enter()?;
        let parsed = self.parse_class_inner();
        self.leave();
        parsed
    }

    fn parse_class_inner(&mut self) -> Step {
        self.parse_member_prefix()?;
        let mut is_abstract = false;
        loop {
            let Some(token) = self.peek().copied() else {
                return Err(Abstention::OutsideDeclaredSubset);
            };
            if self.is_word(&token, "abstract") {
                is_abstract = true;
                self.index += 1;
                continue;
            }
            if self.is_word(&token, "final") {
                self.index += 1;
                continue;
            }
            if !self.is_word(&token, "class") {
                return Err(Abstention::OutsideDeclaredSubset);
            }
            break;
        }
        let class_keyword = *self.peek().expect("class keyword checked above");
        self.index += 1;
        let name = self.expect(TokenKind::Identifier)?;
        let mut extends_last_segment = None;
        if self
            .peek()
            .is_some_and(|token| self.is_word(token, "extends"))
        {
            self.index += 1;
            extends_last_segment = Some(self.parse_qualified_name()?);
        }
        if self
            .peek()
            .is_some_and(|token| self.is_word(token, "implements"))
        {
            self.index += 1;
            self.parse_qualified_name()?;
            while self.eat(TokenKind::Comma) {
                self.parse_qualified_name()?;
            }
        }
        self.expect(TokenKind::LBrace)?;
        let mut methods = Vec::new();
        loop {
            // The prefix — attributes and comments — belongs to the member
            // that follows it, so it is collected here rather than skipped:
            // the nearest doc comment is what carries the @test marker.
            let prefix = self.parse_member_prefix()?;
            if self.eat(TokenKind::Semicolon) {
                continue;
            }
            if self.eat(TokenKind::RBrace) {
                break;
            }
            if let Some(method) = self.parse_class_member(prefix)? {
                methods.push(method);
            }
        }
        self.classes.push(DeclaredClass {
            name: self.text(&name),
            is_abstract,
            extends_last_segment,
            line: line_bounds(self.text, class_keyword.start),
            methods,
        });
        Ok(())
    }

    /// One class member. Returns the parsed method when the member is a
    /// function declaration; traits, constants, and properties return `None`
    /// after their extent is consumed. The prefix of attributes and comments
    /// was already collected by the caller.
    fn parse_class_member(
        &mut self,
        prefix: Prefix<'a>,
    ) -> Result<Option<DeclaredMethod<'a>>, Abstention> {
        if self.peek().is_some_and(|token| self.is_word(token, "use")) {
            // A trait import. Trait-supplied methods are invisible here, so
            // this can only understate support.
            self.index += 1;
            self.trait_used_in_class = true;
            self.parse_qualified_name()?;
            while self.eat(TokenKind::Comma) {
                self.parse_qualified_name()?;
            }
            self.expect(TokenKind::Semicolon)?;
            return Ok(None);
        }
        let mut explicit_public = false;
        let mut is_static = false;
        let mut is_abstract = false;
        loop {
            let Some(token) = self.peek().copied() else {
                return Err(Abstention::OutsideDeclaredSubset);
            };
            if token.kind != TokenKind::Identifier {
                break;
            }
            if self.is_word(&token, "public") {
                explicit_public = true;
                self.index += 1;
            } else if self.is_word(&token, "protected") || self.is_word(&token, "private") {
                self.index += 1;
            } else if self.is_word(&token, "static") {
                is_static = true;
                self.index += 1;
            } else if self.is_word(&token, "abstract") {
                is_abstract = true;
                self.index += 1;
            } else if self.is_word(&token, "final") {
                self.index += 1;
            } else {
                break;
            }
        }
        let Some(token) = self.peek().copied() else {
            return Err(Abstention::OutsideDeclaredSubset);
        };
        if self.is_word(&token, "function") {
            let method =
                self.parse_function_shape(&prefix, explicit_public, is_static, is_abstract)?;
            return Ok(Some(method));
        }
        if self.is_word(&token, "const") {
            self.index += 1;
            self.skip_to_semicolon()?;
            return Ok(None);
        }
        // A property declaration, with or without a type and modifiers; its
        // default value is consumed by the semicolon scan.
        self.skip_to_semicolon()?;
        Ok(None)
    }

    /// Consume the run of attributes and comments that prefix a declaration.
    fn parse_member_prefix(&mut self) -> Result<Prefix<'a>, Abstention> {
        let mut prefix = Prefix {
            attributes: Vec::new(),
            comments: Vec::new(),
        };
        loop {
            let Some(token) = self.peek().copied() else {
                return Ok(prefix);
            };
            match token.kind {
                TokenKind::Comment { .. } => {
                    prefix.comments.push(token);
                    self.index += 1;
                }
                TokenKind::Attribute => {
                    let (name, has_arguments) = attribute_name(self.text(&token))
                        .ok_or(Abstention::OutsideDeclaredSubset)?;
                    prefix.attributes.push((name, has_arguments));
                    self.index += 1;
                }
                _ => return Ok(prefix),
            }
        }
    }

    /// `function [&]? name ( params ) [: bounded-type] ( ; | { body } )`
    fn parse_function(&mut self, prefix: &Prefix<'a>) -> Step {
        self.parse_function_shape(prefix, false, false, false)
            .map(|_| ())
    }

    fn parse_function_shape(
        &mut self,
        prefix: &Prefix<'a>,
        explicit_public: bool,
        is_static: bool,
        is_abstract: bool,
    ) -> Result<DeclaredMethod<'a>, Abstention> {
        self.enter()?;
        let parsed = self.parse_function_inner(prefix, explicit_public, is_static, is_abstract);
        self.leave();
        parsed
    }

    fn parse_function_inner(
        &mut self,
        prefix: &Prefix<'a>,
        explicit_public: bool,
        is_static: bool,
        is_abstract: bool,
    ) -> Result<DeclaredMethod<'a>, Abstention> {
        let function_keyword = self.expect(TokenKind::Identifier)?;
        if !self.is_word(&function_keyword, "function") {
            return Err(Abstention::OutsideDeclaredSubset);
        }
        // `function&name()` reference return.
        self.eat(TokenKind::Operator);
        let name = self.expect(TokenKind::Identifier)?;
        self.expect(TokenKind::LParen)?;
        let mut zero_parameters = true;
        let mut depth = 0usize;
        loop {
            let Some(token) = self.peek().copied() else {
                return Err(Abstention::OutsideDeclaredSubset);
            };
            self.index += 1;
            match token.kind {
                TokenKind::LParen => depth += 1,
                TokenKind::RParen => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                TokenKind::Comment { .. } => {}
                _ => zero_parameters = false,
            }
        }
        if self.eat(TokenKind::Colon) {
            self.parse_bounded_type()?;
        }
        if self.eat(TokenKind::Semicolon) {
            // An abstract or interface-shaped declaration has no body.
        } else {
            self.parse_body()?;
        }
        let nearest_doc = prefix
            .comments
            .iter()
            .rev()
            .find_map(|token| match token.kind {
                TokenKind::Comment { doc: true } => Some(token.text(self.text)),
                _ => None,
            });
        Ok(DeclaredMethod {
            name: self.text(&name),
            explicit_public,
            is_static,
            is_abstract,
            zero_parameters,
            attributes: prefix.attributes.clone(),
            nearest_doc,
            line: line_bounds(self.text, function_keyword.start),
        })
    }

    /// The bounded return-type vocabulary: an optional `?` and one qualified
    /// name. Union, intersection, and DNF types are outside the subset.
    fn parse_bounded_type(&mut self) -> Step {
        if self.eat(TokenKind::Operator) {
            // The `?` of a nullable type; any other operator here leaves the
            // subset and fails the name expectation below.
        }
        self.parse_qualified_name()?;
        Ok(())
    }

    /// A `\`-separated name; returns its last segment.
    fn parse_qualified_name(&mut self) -> Result<&'a str, Abstention> {
        self.eat(TokenKind::Backslash);
        let mut last = self.expect(TokenKind::Identifier)?;
        while self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Backslash)
        {
            self.index += 1;
            last = self.expect(TokenKind::Identifier)?;
        }
        Ok(self.text(&last))
    }

    /// Skip a balanced token group whose opener has already been consumed.
    fn skip_group(&mut self, opener: TokenKind, closer: TokenKind) -> Step {
        let mut depth = 1usize;
        loop {
            let Some(token) = self.peek().copied() else {
                return Err(Abstention::OutsideDeclaredSubset);
            };
            self.index += 1;
            if token.kind == opener {
                depth += 1;
            } else if token.kind == closer {
                depth -= 1;
                if depth == 0 {
                    return Ok(());
                }
            }
        }
    }

    /// Skip to the next `;` at grouping depth zero. A `{` before it cannot be
    /// part of a property or const declaration, so the file leaves the subset.
    fn skip_to_semicolon(&mut self) -> Step {
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;
        loop {
            let Some(token) = self.peek().copied() else {
                return Err(Abstention::OutsideDeclaredSubset);
            };
            self.index += 1;
            match token.kind {
                TokenKind::LParen => paren_depth += 1,
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::LBracket => bracket_depth += 1,
                TokenKind::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::LBrace if paren_depth == 0 && bracket_depth == 0 => {
                    return Err(Abstention::OutsideDeclaredSubset);
                }
                TokenKind::Semicolon if paren_depth == 0 && bracket_depth == 0 => return Ok(()),
                _ => {}
            }
        }
    }

    /// Skip a `{ … }` body by brace balance. Exact because the lexer has
    /// already consumed every brace-bearing literal as one token.
    fn parse_body(&mut self) -> Step {
        self.enter()?;
        let parsed = self.parse_body_inner();
        self.leave();
        parsed
    }

    fn parse_body_inner(&mut self) -> Step {
        self.expect(TokenKind::LBrace)?;
        let mut depth = 1usize;
        loop {
            let Some(token) = self.peek().copied() else {
                return Err(Abstention::OutsideDeclaredSubset);
            };
            self.index += 1;
            match token.kind {
                TokenKind::LBrace => {
                    depth += 1;
                    if depth > MAX_BLOCK_DEPTH {
                        // Untrusted input: a body of nothing but nested
                        // brackets must abstain rather than run unbounded.
                        return Err(Abstention::DepthLimit);
                    }
                }
                TokenKind::RBrace => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(());
                    }
                }
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
        parse(text, &tokens)
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
                let parse = parse(text, &tokens);
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
