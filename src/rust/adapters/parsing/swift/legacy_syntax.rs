//! Legacy hand-written Swift parser, kept only to run the ADR-0058 differential
//! against the Tree-sitter reading. It is deleted once the fixture-wide capture
//! confirms the migration; nothing outside tests reaches it.

use super::{
    is_test_method_name, lex, ClassDecl, ImportClause, MethodDecl, Refusal, SwiftFile, Token,
    TokenKind, MAX_HEADER_TOKENS, MAX_MODIFIERS, MAX_NAME_BYTES, MAX_NESTING, MEMBER_MODIFIERS,
    OPERATOR_NAME_CHARS, TYPE_MODIFIERS,
};

/// Parse one Swift source file with the legacy parser, or refuse it whole.
pub(super) fn parse_file(text: &str) -> Result<SwiftFile, Refusal> {
    parse_file_with(text, false)
}

/// The legacy parser with its `init`/`deinit` head defect repaired, used only
/// to look past that defect when comparing against the Tree-sitter reading.
pub(super) fn parse_file_patched(text: &str) -> Result<SwiftFile, Refusal> {
    parse_file_with(text, true)
}

fn parse_file_with(text: &str, admit_initializers: bool) -> Result<SwiftFile, Refusal> {
    let tokens = lex(text)?;
    let mut parser = Parser {
        admit_initializers,
        text,
        tokens: &tokens,
        index: 0,
        depth: 0,
        imports: Vec::new(),
        classes: Vec::new(),
        free_test_function: false,
    };
    parser.parse_file_body()?;
    Ok(SwiftFile {
        imports: parser.imports,
        classes: parser.classes,
        free_test_function: parser.free_test_function,
    })
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// The `import kind` words: `import class XCTest.XCTestCase` still binds the
/// module named by the first path component.
const IMPORT_KINDS: &[&str] = &[
    "class",
    "struct",
    "enum",
    "protocol",
    "func",
    "var",
    "let",
    "typealias",
];

/// File-level type declarations whose bodies are consumed for extent only.
/// No anchor is claimed inside them: an `extension` cannot re-open a class
/// this frontend admitted, and a test class nested in a value type is a
/// shape ADR-0048 deliberately does not descend into.
const OPAQUE_TYPE_KEYWORDS: &[&str] = &["struct", "enum", "actor", "protocol", "extension"];

/// The words that may follow a `class` modifier, as opposed to a nested
/// `class Name` declaration whose name can be any identifier.
const MEMBER_DECLARATION_KEYWORDS: &[&str] = &["func", "init", "deinit", "var", "let", "typealias"];

struct Parser<'a> {
    /// Read `init` and `deinit` as their own name instead of expecting a
    /// function name after the keyword.
    admit_initializers: bool,
    text: &'a str,
    tokens: &'a [Token],
    index: usize,
    /// Class-body recursion depth, bounded because repository contents are
    /// untrusted.
    depth: usize,
    imports: Vec<ImportClause>,
    classes: Vec<ClassDecl>,
    free_test_function: bool,
}

/// One declaration head, reduced to what the anchor rule asks of it.
struct FuncHead {
    start: usize,
    is_test_prefixed: bool,
    empty_params: bool,
    void_return: bool,
    rethrows: bool,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token> {
        self.tokens.get(self.index).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<Token> {
        self.tokens.get(self.index + offset).copied()
    }

    fn bump(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.index).copied();
        self.index += 1;
        token
    }

    fn word(&self, token: Token) -> &'a str {
        let text = self.text;
        &text[token.start..token.end]
    }

    fn peek_word(&self) -> Option<&'a str> {
        let token = self.peek()?;
        if token.kind != TokenKind::Word {
            return None;
        }
        let text = self.text;
        Some(&text[token.start..token.end])
    }

    fn skip_separators(&mut self) {
        while self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Newline || token.is_punct(';'))
        {
            self.index += 1;
        }
    }

    fn skip_newlines(&mut self) {
        while self
            .peek()
            .is_some_and(|token| token.kind == TokenKind::Newline)
        {
            self.index += 1;
        }
    }

    fn parse_file_body(&mut self) -> Result<(), Refusal> {
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                return Ok(());
            };
            if token.is_punct('}') {
                return Err(Refusal::UnbalancedBlockStructure);
            }
            self.parse_file_member()?;
        }
    }

    fn parse_file_member(&mut self) -> Result<(), Refusal> {
        let start = self.peek().expect("caller checked a token exists").start;
        let attributes = self.parse_attribute_lists()?;
        let modifiers = self.parse_modifiers(TYPE_MODIFIERS)?;
        self.skip_newlines();
        let Some(token) = self.peek() else {
            return Err(Refusal::UnbalancedBlockStructure);
        };
        if token.kind == TokenKind::HashWord {
            return self.skip_hash_construct();
        }
        let keyword = match token.kind {
            TokenKind::Word => self.word(token),
            _ => return Err(Refusal::UnadmittedFileConstruct),
        };
        match keyword {
            "import"
                if attributes.iter().all(|name| *name == "testable") && modifiers.is_empty() =>
            {
                self.parse_import()
            }
            "class" => {
                let class = self.parse_class(start)?;
                self.classes.push(class);
                Ok(())
            }
            "func" => {
                let head = self.parse_func_head(start)?;
                self.consume_body()?;
                if head.is_test_prefixed && head.empty_params && head.void_return {
                    self.free_test_function = true;
                }
                Ok(())
            }
            "var" | "let" | "typealias" => self.parse_simple_head(),
            keyword if OPAQUE_TYPE_KEYWORDS.contains(&keyword) => self.skip_opaque_type_body(),
            _ => Err(Refusal::UnadmittedFileConstruct),
        }
    }

    /// `import [kind] dotted.path` — single line. The module is the first
    /// path component, which is what an `XCTestCase` binding question needs.
    fn parse_import(&mut self) -> Result<(), Refusal> {
        self.bump();
        if let Some(kind) = self.peek_word() {
            if IMPORT_KINDS.contains(&kind) {
                self.bump();
            }
        }
        let mut module = String::new();
        let mut components = 0usize;
        loop {
            let token = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
            match token.kind {
                TokenKind::Word => {
                    components += 1;
                    if components == 1 {
                        module = self.word(token).to_string();
                        if module.len() > MAX_NAME_BYTES {
                            return Err(Refusal::ResourceLimit);
                        }
                    }
                    self.bump();
                }
                TokenKind::Punct('.') if components > 0 => {
                    self.bump();
                    if self.peek().is_none_or(|next| next.kind != TokenKind::Word) {
                        return Err(Refusal::UnbalancedBlockStructure);
                    }
                }
                _ => break,
            }
        }
        if components == 0 {
            return Err(Refusal::UnbalancedBlockStructure);
        }
        self.imports.push(ImportClause { module });
        self.expect_statement_end()
    }

    /// `class Name: Super, Proto1, Proto2 {` — the header from the `class`
    /// keyword to the `{` is required to sit on one physical line, which is
    /// the bounded class-header shape ADR-0048 admits.
    fn parse_class(&mut self, start: usize) -> Result<ClassDecl, Refusal> {
        self.bump();
        let name = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
        if name.kind != TokenKind::Word && name.kind != TokenKind::Backticked {
            return Err(Refusal::UnbalancedBlockStructure);
        }
        self.bump();
        if name.kind == TokenKind::Word && self.peek().is_some_and(|token| token.is_punct('<')) {
            return Err(Refusal::GenericDeclaration);
        }
        let mut derives_xctestcase = false;
        if self.peek().is_some_and(|token| token.is_punct(':')) {
            self.bump();
            derives_xctestcase = self.parse_inheritance_list()?;
        }
        self.expect_body_open()?;
        let (methods, nested, end) = self.parse_class_body()?;
        Ok(ClassDecl {
            start,
            end,
            derives_xctestcase,
            methods,
            nested,
        })
    }

    /// The `: Super, Proto` list. A top-level entry whose token run is
    /// exactly one bare `XCTestCase` word marks the class; generic arguments
    /// and qualified names in the list are consumed but never match, because
    /// the anchor claims the bare source-visible spelling.
    fn parse_inheritance_list(&mut self) -> Result<bool, Refusal> {
        let mut derives = false;
        let mut angle_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut entry_tokens = 0usize;
        let mut entry_is_bare_test_case = true;
        loop {
            let token = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
            if angle_depth == 0 && paren_depth == 0 {
                match token.kind {
                    TokenKind::Punct('{') => break,
                    TokenKind::Newline => return Err(Refusal::UnadmittedDeclarationHeader),
                    _ => {}
                }
            }
            match token.kind {
                TokenKind::Punct('<') => {
                    angle_depth += 1;
                    entry_is_bare_test_case = false;
                }
                TokenKind::Punct('(') => {
                    paren_depth += 1;
                    entry_is_bare_test_case = false;
                }
                TokenKind::Punct(')') => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::Punct('>') => angle_depth = angle_depth.saturating_sub(1),
                TokenKind::Punct(',') if angle_depth == 0 && paren_depth == 0 => {
                    if entry_is_bare_test_case && entry_tokens == 1 {
                        derives = true;
                    }
                    entry_tokens = 0;
                    entry_is_bare_test_case = true;
                    self.bump();
                    continue;
                }
                TokenKind::Word => {
                    entry_tokens += 1;
                    if !(entry_tokens == 1 && self.word(token) == "XCTestCase") {
                        entry_is_bare_test_case = false;
                    }
                }
                TokenKind::Punct('.') => {}
                _ => {
                    entry_tokens += 1;
                    entry_is_bare_test_case = false;
                }
            }
            if entry_tokens > MAX_HEADER_TOKENS {
                return Err(Refusal::ResourceLimit);
            }
            self.bump();
        }
        if entry_is_bare_test_case && entry_tokens == 1 {
            derives = true;
        }
        Ok(derives)
    }

    /// Members until the class's closing `}`.
    fn parse_class_body(&mut self) -> Result<(Vec<MethodDecl>, Vec<ClassDecl>, usize), Refusal> {
        if self.depth >= MAX_NESTING {
            return Err(Refusal::ResourceLimit);
        }
        self.depth += 1;
        let mut methods = Vec::new();
        let mut nested = Vec::new();
        let end;
        loop {
            self.skip_separators();
            let Some(token) = self.peek() else {
                self.depth -= 1;
                return Err(Refusal::UnbalancedBlockStructure);
            };
            if token.is_punct('}') {
                end = token.end;
                self.bump();
                break;
            }
            let start = token.start;
            self.parse_attribute_lists()?;
            let modifiers = self.parse_modifiers(MEMBER_MODIFIERS)?;
            self.skip_newlines();
            let Some(keyword) = self.peek() else {
                self.depth -= 1;
                return Err(Refusal::UnbalancedBlockStructure);
            };
            if keyword.kind != TokenKind::Word {
                self.depth -= 1;
                return Err(Refusal::UnadmittedMemberShape);
            }
            match self.word(keyword) {
                "func" => {
                    let head = self.parse_func_head(start)?;
                    self.consume_body()?;
                    let end = self
                        .tokens
                        .get(self.index - 1)
                        .map(|token| token.end)
                        .unwrap_or(head.start);
                    methods.push(MethodDecl {
                        start: head.start,
                        end,
                        is_static: modifiers
                            .iter()
                            .any(|modifier| *modifier == "static" || *modifier == "class"),
                        is_test_prefixed: head.is_test_prefixed,
                        empty_params: head.empty_params,
                        void_return: head.void_return,
                        rethrows: head.rethrows,
                    });
                }
                "init" | "deinit" => {
                    self.parse_func_head(start)?;
                    self.consume_body()?;
                }
                "var" | "let" | "typealias" => {
                    self.parse_simple_head()?;
                }
                "class" => {
                    let class = self.parse_class(start)?;
                    nested.push(class);
                }
                _ => {
                    self.depth -= 1;
                    return Err(Refusal::UnadmittedMemberShape);
                }
            }
        }
        self.depth -= 1;
        Ok((methods, nested, end))
    }

    /// Zero or more attribute lists standing before a declaration.
    ///
    /// The attribute binds by grammar, not by line position, so an attribute
    /// list may end its own line before the declaration continues. A bare
    /// attribute or one with an exactly empty `()` argument list is admitted
    /// and ignored; a non-empty argument list is outside the declared subset
    /// and refuses the file, because its contents can change what the
    /// declaration means.
    fn parse_attribute_lists(&mut self) -> Result<Vec<&'a str>, Refusal> {
        let mut attributes = Vec::new();
        while self.peek().is_some_and(|token| token.is_punct('@')) {
            self.bump();
            let name = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
            if name.kind != TokenKind::Word {
                return Err(Refusal::UnbalancedBlockStructure);
            }
            if name.end - name.start > MAX_NAME_BYTES {
                return Err(Refusal::ResourceLimit);
            }
            let name = self.word(name);
            self.bump();
            if self.peek().is_some_and(|token| token.is_punct('(')) {
                self.bump();
                if self.peek().is_some_and(|token| !token.is_punct(')')) {
                    return Err(Refusal::AttributeArgument);
                }
                self.skip_balanced('(', ')')?;
            }
            attributes.push(name);
            if attributes.len() > MAX_MODIFIERS {
                return Err(Refusal::ResourceLimit);
            }
            self.skip_newlines();
        }
        Ok(attributes)
    }

    fn parse_modifiers(&mut self, table: &'a [&'a str]) -> Result<Vec<&'a str>, Refusal> {
        let mut modifiers = Vec::new();
        while let Some(word) = self.peek_word() {
            let Some(matched) = table.iter().find(|candidate| **candidate == word) else {
                break;
            };
            // `class` is both a member modifier (`class func`) and the
            // nested-class keyword. It is read as a modifier only when a
            // declaration keyword or another modifier follows; otherwise a
            // name follows and this is the declaration itself.
            if *matched == "class" && !self.class_introduces_member() {
                break;
            }
            modifiers.push(*matched);
            self.bump();
            if modifiers.len() > MAX_MODIFIERS {
                return Err(Refusal::ResourceLimit);
            }
        }
        Ok(modifiers)
    }

    /// True when the word after this `class` shows it is the modifier: a
    /// member declaration keyword or another modifier. A bare identifier
    /// names a nested class instead.
    fn class_introduces_member(&self) -> bool {
        let Some(next) = self.peek_at(1) else {
            return false;
        };
        if next.kind != TokenKind::Word {
            return false;
        }
        let text = self.text;
        let word = &text[next.start..next.end];
        MEMBER_DECLARATION_KEYWORDS.contains(&word) || MEMBER_MODIFIERS.contains(&word)
    }

    /// The head of a `func` (or an `init`/`deinit`) declaration, up to the
    /// `{` that opens its body.
    ///
    /// Newlines are admitted inside the parameter parentheses — the wrapped
    /// parameter-list case — and refused everywhere else before the `{`,
    /// which keeps the head bounded while accepting the one continuation the
    /// language itself encourages.
    fn parse_func_head(&mut self, start: usize) -> Result<FuncHead, Refusal> {
        let keyword_is_name = self.admit_initializers
            && self
                .peek_word()
                .is_some_and(|word| word == "init" || word == "deinit");
        if !keyword_is_name {
            self.bump();
        }
        let name = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
        let mut is_test_prefixed = false;
        match name.kind {
            TokenKind::Word => {
                is_test_prefixed = is_test_method_name(self.word(name));
                self.bump();
                if self.peek().is_some_and(|token| token.is_punct('<')) {
                    return Err(Refusal::GenericDeclaration);
                }
            }
            TokenKind::Backticked => {
                self.bump();
                if name.end - name.start > MAX_NAME_BYTES {
                    return Err(Refusal::ResourceLimit);
                }
            }
            TokenKind::Punct(character) if OPERATOR_NAME_CHARS.contains(&character) => {
                // An operator function: `func ==`, `func <`, custom operators.
                // It is an admitted declaration that can never carry the
                // anchor, because an operator has no bare identifier name.
                let mut end = name.end;
                while let Some(token) = self.peek() {
                    match token.kind {
                        TokenKind::Punct(character) if OPERATOR_NAME_CHARS.contains(&character) => {
                            end = token.end;
                            self.bump();
                        }
                        _ => break,
                    }
                }
                if end - name.start > MAX_NAME_BYTES {
                    return Err(Refusal::ResourceLimit);
                }
            }
            _ => return Err(Refusal::UnbalancedBlockStructure),
        }
        // A failable initializer: `init?` / `init!`.
        if self
            .peek()
            .is_some_and(|token| token.is_punct('?') || token.is_punct('!'))
        {
            self.bump();
        }
        let empty_params = if self.peek().is_some_and(|token| token.is_punct('(')) {
            self.bump();
            let empty = self.peek().is_some_and(|token| token.is_punct(')'));
            self.skip_balanced('(', ')')?;
            empty
        } else {
            // `deinit` carries no parameter list.
            true
        };
        let mut rethrows = false;
        let mut async_seen = false;
        loop {
            match self.peek_word() {
                Some("async") if !async_seen => {
                    async_seen = true;
                    self.bump();
                }
                Some(effect @ ("throws" | "rethrows")) => {
                    rethrows |= effect == "rethrows";
                    self.bump();
                }
                _ => break,
            }
        }
        let mut void_return = true;
        if self.peek().is_some_and(|token| token.is_punct('-'))
            && self.peek_at(1).is_some_and(|token| token.is_punct('>'))
        {
            self.bump();
            self.bump();
            void_return = self.parse_return_type()?;
        }
        self.expect_body_open()?;
        Ok(FuncHead {
            start,
            is_test_prefixed,
            empty_params,
            void_return,
            rethrows,
        })
    }

    /// The return type, up to the `{`. `Void`, `Swift.Void`, and `()` are the
    /// Void spellings; anything else parses but is not a Void return.
    fn parse_return_type(&mut self) -> Result<bool, Refusal> {
        let mut tokens = 0usize;
        let mut words: Vec<&'a str> = Vec::new();
        let mut first_punct: Option<char> = None;
        let mut second_punct: Option<char> = None;
        loop {
            let token = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
            match token.kind {
                TokenKind::Punct('{') => break,
                TokenKind::Newline => return Err(Refusal::UnadmittedDeclarationHeader),
                TokenKind::Word => words.push(self.word(token)),
                TokenKind::Punct(character) => {
                    if tokens == 0 {
                        first_punct = Some(character);
                    } else if tokens == 1 {
                        second_punct = Some(character);
                    }
                }
                _ => {}
            }
            tokens += 1;
            if tokens > MAX_HEADER_TOKENS {
                return Err(Refusal::ResourceLimit);
            }
            self.bump();
        }
        Ok(match (words.as_slice(), tokens) {
            ([word], 1) => *word == "Void",
            ([first, last], 3) => *first == "Swift" && *last == "Void",
            ([], 2) => first_punct == Some('(') && second_punct == Some(')'),
            _ => false,
        })
    }

    /// A `var`, `let`, or `typealias` head: bounded to one line at bracket
    /// depth zero. Reaching a `{` at depth zero is a computed property, an
    /// accessor block, or a stored closure, and refuses the file.
    fn parse_simple_head(&mut self) -> Result<(), Refusal> {
        self.bump();
        let mut paren_depth = 0usize;
        let mut tokens = 0usize;
        loop {
            let token = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
            match token.kind {
                TokenKind::Punct('(') => paren_depth += 1,
                TokenKind::Punct(')') => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::Punct('{') if paren_depth == 0 => {
                    return Err(Refusal::UnadmittedAccessor);
                }
                TokenKind::Punct(';') | TokenKind::Newline if paren_depth == 0 => {
                    self.bump();
                    return Ok(());
                }
                _ => {}
            }
            tokens += 1;
            if tokens > MAX_HEADER_TOKENS {
                return Err(Refusal::ResourceLimit);
            }
            self.bump();
        }
    }

    /// A `#name …` construct outside a body: `#selector(…)`,
    /// `#warning("…")`, and their peers are consumed opaquely with their
    /// balanced argument group. `#if` and its family never reach the parser.
    fn skip_hash_construct(&mut self) -> Result<(), Refusal> {
        self.bump();
        if self.peek().is_some_and(|token| token.is_punct('(')) {
            self.bump();
            self.skip_balanced('(', ')')?;
        }
        self.skip_to_statement_end()
    }

    fn skip_to_statement_end(&mut self) -> Result<(), Refusal> {
        while let Some(token) = self.peek() {
            if token.kind == TokenKind::Newline || token.is_punct(';') {
                self.bump();
                return Ok(());
            }
            self.bump();
        }
        Ok(())
    }

    /// The `{` that opens a declaration body. A newline first means the head
    /// continued past the single line this frontend admits.
    fn expect_body_open(&mut self) -> Result<(), Refusal> {
        match self.peek() {
            Some(token) if token.is_punct('{') => {
                self.bump();
                Ok(())
            }
            Some(token) if token.kind == TokenKind::Newline => {
                Err(Refusal::UnadmittedDeclarationHeader)
            }
            _ => Err(Refusal::UnbalancedBlockStructure),
        }
    }

    fn expect_statement_end(&mut self) -> Result<(), Refusal> {
        match self.peek() {
            Some(token) if token.kind == TokenKind::Newline || token.is_punct(';') => {
                self.bump();
                Ok(())
            }
            None => Ok(()),
            _ => Err(Refusal::UnbalancedBlockStructure),
        }
    }

    /// A file-level type whose body is consumed for extent only. Its head may
    /// wrap across lines; no anchor inside it is claimed or read.
    fn skip_opaque_type_body(&mut self) -> Result<(), Refusal> {
        let mut tokens = 0usize;
        loop {
            let token = self.peek().ok_or(Refusal::UnbalancedBlockStructure)?;
            if token.is_punct('{') {
                self.bump();
                return self.consume_body();
            }
            tokens += 1;
            if tokens > MAX_HEADER_TOKENS {
                return Err(Refusal::ResourceLimit);
            }
            self.bump();
        }
    }

    /// One body whose opening `{` the caller already consumed. Statements
    /// are never parsed: no anchor rests on what a statement means, only on
    /// where the enclosing braces close, and strings and comments are already
    /// single tokens.
    fn consume_body(&mut self) -> Result<(), Refusal> {
        let mut depth = 1usize;
        while let Some(token) = self.peek() {
            if token.is_punct('{') {
                depth += 1;
            } else if token.is_punct('}') {
                depth -= 1;
                if depth == 0 {
                    self.bump();
                    return Ok(());
                }
            }
            self.bump();
        }
        Err(Refusal::UnbalancedBlockStructure)
    }

    /// Consume through the closing bracket of a group whose opening bracket
    /// the caller already consumed. Nested groups of the same pair are
    /// honoured; a stray closer or end of input is a structure failure.
    fn skip_balanced(&mut self, open: char, close: char) -> Result<(), Refusal> {
        let mut depth = 1usize;
        while let Some(token) = self.peek() {
            if token.is_punct(open) {
                depth += 1;
            } else if token.is_punct(close) {
                depth -= 1;
                if depth == 0 {
                    self.bump();
                    return Ok(());
                }
            }
            self.bump();
        }
        Err(Refusal::UnbalancedBlockStructure)
    }
}
