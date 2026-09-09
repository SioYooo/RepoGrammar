//! Bounded Object Pascal lexer and declaration parser.
//!
//! This is a parser, not a scanner: it produces a token stream and consumes
//! grammar productions, so the extent of a class body comes from the grammar
//! rather than from line position. That is what lets a nested type inside a
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

pub(crate) fn parse_unit(text: &str, tokens: &[Token]) -> UnitParse {
    Parser {
        text,
        tokens,
        at: 0,
        out: UnitParse::default(),
    }
    .run()
}

struct Parser<'a> {
    text: &'a str,
    tokens: &'a [Token],
    at: usize,
    out: UnitParse,
}

impl<'a> Parser<'a> {
    fn run(mut self) -> UnitParse {
        self.out.uses_dunitx = self.scan_uses_clauses();
        self.at = 0;
        while self.at < self.tokens.len() {
            if self.ident_is(self.at, "type") {
                self.at += 1;
                self.parse_type_section();
                continue;
            }
            if self.ident_is(self.at, "implementation") {
                break;
            }
            self.at += 1;
        }
        if !self.out.uses_dunitx {
            self.out.attributed_without_uses = self.any_dunitx_attribute_spelling();
            self.out.fixtures.clear();
        }
        self.out
    }

    /// A `uses` clause naming `DUnitX.TestFramework`. The dialect evidence is
    /// this import, never the file suffix, which ADR-0032 forbids as an oracle.
    fn scan_uses_clauses(&mut self) -> bool {
        let mut index = 0usize;
        while index < self.tokens.len() {
            if !self.ident_is(index, "uses") {
                index += 1;
                continue;
            }
            index += 1;
            let mut qualified = String::new();
            while index < self.tokens.len() {
                let token = self.tokens[index];
                match token.kind {
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

    fn parse_type_section(&mut self) {
        loop {
            let attributes = self.collect_attributes();
            let Some(name) = self.peek_ident() else {
                return;
            };
            let _ = name;
            let decl_start = attributes
                .first()
                .map(|attribute| attribute.start)
                .unwrap_or(self.tokens[self.at].start);
            self.at += 1;
            // Generic parameter lists are consumed as opaque punctuation.
            if self.punct_is(self.at, b'<') {
                self.skip_balanced(b'<', b'>');
            }
            if !self.punct_is(self.at, b'=') {
                return;
            }
            self.at += 1;
            let is_fixture = attributes
                .iter()
                .any(|attribute| attribute.name == "testfixture");
            self.parse_type_rhs(decl_start, is_fixture);
            if self.at >= self.tokens.len() || !self.starts_type_decl() {
                return;
            }
        }
    }

    fn starts_type_decl(&self) -> bool {
        let mut index = self.at;
        while self.punct_is(index, b'[') {
            let Some(close) = self.matching(index, b'[', b']') else {
                return false;
            };
            index = close + 1;
        }
        self.tokens
            .get(index)
            .is_some_and(|token| token.kind == TokenKind::Ident)
    }

    fn parse_type_rhs(&mut self, decl_start: usize, is_fixture: bool) {
        if !self.ident_is(self.at, "class") {
            self.skip_to_semicolon();
            return;
        }
        let class_keyword = self.at;
        self.at += 1;
        // `class;` is a forward declaration and `class of X` is a metaclass.
        // Neither opens a body, and neither is the fixture.
        if self.punct_is(self.at, b';') || self.ident_is(self.at, "of") {
            self.skip_to_semicolon();
            return;
        }
        let mut header_end = self.tokens[class_keyword].end;
        if self.punct_is(self.at, b'(') {
            if let Some(close) = self.matching(self.at, b'(', b')') {
                header_end = self.tokens[close].end;
                self.at = close + 1;
            } else {
                self.out.abstained = true;
                self.at = self.tokens.len();
                return;
            }
        }
        let conditional = self.tokens[class_keyword].conditional_depth > 0;
        match self.parse_class_body() {
            Some(tests) => {
                if is_fixture {
                    if conditional {
                        self.out.skipped_conditional = true;
                    } else {
                        self.out.fixtures.push(FixtureDecl {
                            start: decl_start,
                            end: header_end,
                            tests,
                        });
                    }
                }
            }
            None => self.out.abstained = true,
        }
        self.skip_to_semicolon();
    }

    /// Members until the class's own `end`. Returns `None` when the body holds a
    /// construct outside the declared subset, in which case the class yields no
    /// anchor at all rather than a partly-understood one.
    fn parse_class_body(&mut self) -> Option<Vec<TestDecl>> {
        let mut tests = Vec::new();
        loop {
            let attributes = self.collect_attributes();
            if self.at >= self.tokens.len() {
                return None;
            }
            if self.ident_is(self.at, "end") {
                self.at += 1;
                return Some(tests);
            }
            if self.consume_visibility() || self.consume_section_keyword() {
                continue;
            }
            let is_class_member = self.ident_is(self.at, "class");
            if is_class_member {
                self.at += 1;
            }
            if self.at >= self.tokens.len() {
                return None;
            }
            // A nested type declaration recurses, so its own `end` closes it and
            // not the enclosing fixture.
            if self.tokens[self.at].kind == TokenKind::Ident && self.nested_type_ahead() {
                let start = self.tokens[self.at].start;
                self.at += 1;
                if self.punct_is(self.at, b'<') {
                    self.skip_balanced(b'<', b'>');
                }
                self.at += 1;
                self.parse_type_rhs(start, false);
                continue;
            }
            let method = self.method_kind();
            match method {
                Some(MethodKind::Procedure) | Some(MethodKind::Function) => {
                    let decl_start = attributes
                        .first()
                        .map(|attribute| attribute.start)
                        .unwrap_or(self.tokens[self.at].start);
                    let conditional = attributes.iter().any(|attribute| attribute.conditional)
                        || self.tokens[self.at].conditional_depth > 0;
                    self.at += 1;
                    if self.at >= self.tokens.len() || self.tokens[self.at].kind != TokenKind::Ident
                    {
                        return None;
                    }
                    self.at += 1;
                    if self.punct_is(self.at, b'(') {
                        let close = self.matching(self.at, b'(', b')')?;
                        self.at = close + 1;
                    }
                    let decl_end = self.skip_to_semicolon()?;
                    self.consume_method_directives();
                    let is_test = attributes.iter().any(|attribute| attribute.name == "test");
                    if is_test && method == Some(MethodKind::Procedure) {
                        if conditional {
                            self.out.skipped_conditional = true;
                        } else {
                            tests.push(TestDecl {
                                start: decl_start,
                                end: decl_end,
                            });
                        }
                    }
                }
                None => {
                    // Fields and properties: an identifier list, a type run, and
                    // a terminator. The type is never interpreted.
                    if self.tokens[self.at].kind != TokenKind::Ident {
                        return None;
                    }
                    self.skip_to_semicolon()?;
                }
            }
        }
    }

    fn nested_type_ahead(&self) -> bool {
        let mut index = self.at + 1;
        if self.punct_is(index, b'<') {
            let Some(close) = self.matching(index, b'<', b'>') else {
                return false;
            };
            index = close + 1;
        }
        self.punct_is(index, b'=')
    }

    fn method_kind(&self) -> Option<MethodKind> {
        if self.ident_is(self.at, "procedure") {
            Some(MethodKind::Procedure)
        } else if self.ident_is(self.at, "function")
            || self.ident_is(self.at, "constructor")
            || self.ident_is(self.at, "destructor")
        {
            Some(MethodKind::Function)
        } else {
            None
        }
    }

    fn consume_method_directives(&mut self) {
        while self.at < self.tokens.len() && self.tokens[self.at].kind == TokenKind::Ident {
            const DIRECTIVES: &[&str] = &[
                "overload",
                "virtual",
                "override",
                "abstract",
                "reintroduce",
                "static",
                "inline",
                "stdcall",
                "cdecl",
                "register",
                "safecall",
                "pascal",
                "varargs",
                "deprecated",
                "experimental",
                "platform",
                "final",
                "dynamic",
                "message",
            ];
            let name = self.lower(self.at);
            if !DIRECTIVES.contains(&name.as_str()) {
                return;
            }
            self.at += 1;
            while self.at < self.tokens.len() && !self.punct_is(self.at, b';') {
                self.at += 1;
            }
            self.at += 1;
        }
    }

    fn consume_visibility(&mut self) -> bool {
        if self.ident_is(self.at, "strict") {
            self.at += 1;
        } else if !(self.ident_is(self.at, "private")
            || self.ident_is(self.at, "protected")
            || self.ident_is(self.at, "public")
            || self.ident_is(self.at, "published")
            || self.ident_is(self.at, "automated"))
        {
            return false;
        }
        if self.ident_is(self.at, "private")
            || self.ident_is(self.at, "protected")
            || self.ident_is(self.at, "public")
            || self.ident_is(self.at, "published")
            || self.ident_is(self.at, "automated")
        {
            self.at += 1;
        }
        true
    }

    fn consume_section_keyword(&mut self) -> bool {
        if self.ident_is(self.at, "var")
            || self.ident_is(self.at, "const")
            || self.ident_is(self.at, "threadvar")
        {
            self.at += 1;
            return true;
        }
        if self.ident_is(self.at, "type") && !self.nested_type_after_keyword() {
            self.at += 1;
            return true;
        }
        if self.ident_is(self.at, "property") {
            self.skip_to_semicolon();
            return true;
        }
        false
    }

    fn nested_type_after_keyword(&self) -> bool {
        false
    }

    fn collect_attributes(&mut self) -> Vec<AttributeRef> {
        let mut attributes = Vec::new();
        while self.punct_is(self.at, b'[') {
            let Some(close) = self.matching(self.at, b'[', b']') else {
                return attributes;
            };
            if let Some(name) = self.attribute_name_at(self.at) {
                attributes.push(AttributeRef {
                    name,
                    start: self.tokens[self.at].start,
                    conditional: self.tokens[self.at].conditional_depth > 0,
                });
            }
            self.at = close + 1;
        }
        attributes
    }

    /// `[Name]` or `[Name(...)]`, lowercased. A qualified `[Unit.Name]` keeps
    /// only its last selector, the way the attribute resolves.
    fn attribute_name_at(&self, open: usize) -> Option<String> {
        let mut index = open + 1;
        let mut last = None;
        while index < self.tokens.len() {
            let token = self.tokens[index];
            if token.kind == TokenKind::Ident {
                last = Some(self.lower(index));
                index += 1;
                continue;
            }
            if self.punct_is(index, b'.') {
                index += 1;
                continue;
            }
            break;
        }
        last
    }

    fn skip_to_semicolon(&mut self) -> Option<usize> {
        let mut depth = 0usize;
        while self.at < self.tokens.len() {
            if self.punct_is(self.at, b'(') || self.punct_is(self.at, b'[') {
                depth += 1;
            } else if self.punct_is(self.at, b')') || self.punct_is(self.at, b']') {
                depth = depth.saturating_sub(1);
            } else if depth == 0 && self.punct_is(self.at, b';') {
                let end = self.tokens[self.at].end;
                self.at += 1;
                return Some(end);
            } else if depth == 0 && self.ident_is(self.at, "end") {
                return None;
            }
            self.at += 1;
        }
        None
    }

    fn skip_balanced(&mut self, open: u8, close: u8) {
        if let Some(end) = self.matching(self.at, open, close) {
            self.at = end + 1;
        } else {
            self.at = self.tokens.len();
        }
    }

    fn matching(&self, from: usize, open: u8, close: u8) -> Option<usize> {
        let mut depth = 0usize;
        let mut index = from;
        while index < self.tokens.len() {
            if self.punct_is(index, open) {
                depth += 1;
            } else if self.punct_is(index, close) {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            index += 1;
        }
        None
    }

    fn peek_ident(&self) -> Option<&Token> {
        self.tokens
            .get(self.at)
            .filter(|token| token.kind == TokenKind::Ident)
    }

    fn ident_is(&self, index: usize, word: &str) -> bool {
        self.tokens
            .get(index)
            .is_some_and(|token| token.kind == TokenKind::Ident)
            && self.lower(index) == word
    }

    fn punct_is(&self, index: usize, byte: u8) -> bool {
        self.tokens.get(index).is_some_and(|token| {
            token.kind == TokenKind::Punct && self.text.as_bytes().get(token.start) == Some(&byte)
        })
    }

    /// Object Pascal is case-insensitive, so every keyword and attribute
    /// comparison is made on a lowercased slice.
    fn lower(&self, index: usize) -> String {
        let token = self.tokens[index];
        self.text[token.start..token.end].to_ascii_lowercase()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MethodKind {
    Procedure,
    Function,
}

struct AttributeRef {
    name: String,
    start: usize,
    conditional: bool,
}
