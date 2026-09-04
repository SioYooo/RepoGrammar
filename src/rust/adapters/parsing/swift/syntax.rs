//! Bounded Swift lexer and recursive-descent parser for ADR-0048.
//!
//! This module owns syntax only. It decides what the token stream *is*; it
//! decides nothing about XCTest and names no framework. `xctest.rs` walks the
//! declarations this produces and applies the anchor rule. The separation is
//! deliberate: a widened claim and a widened grammar are different decisions
//! and must not be made in the same place.
//!
//! The declared invariance set is **Swift 5.x source-mode syntax**. Every
//! construct admitted below lexes and nests identically across the Swift 5
//! release line, so the frontend never selects a compiler version, a language
//! mode, or a toolchain and never needs one. A construct any member of the set
//! reads differently — the regex literal is the one that matters, because a
//! `/` starts a literal only from Swift 5.7 and lexes as an operator before
//! that — is refused rather than read under an assumed version. A keyword a
//! later Swift release adds is equally refused: the parser does not guess at
//! it, so it cannot silently mis-nest.
//!
//! The parser abstains; it never recovers. Outside the admitted grammar it
//! returns a typed [`Refusal`] for the whole file and yields no declaration,
//! so a partial or recovered tree can never become an anchor. This discharges
//! ADR-0025 D7's no-text-matching requirement the way ADR-0042 through
//! ADR-0046 did for their lanes: with a real parse, not a scanner.
//!
//! Nothing here invokes `swift`, `swiftc`, `swift-frontend`, `sourcekit-lsp`,
//! SwiftPM, Xcode, `xcodebuild`, a macro, a plugin, a package script, a child
//! process, or the network.

/// The bounded ceilings this parser works under. Repository input is
/// untrusted, so the token stream, the interpolation nesting inside string
/// literals, the class recursion, and every declaration header are capped.
const MAX_TOKENS: usize = 400_000;
const MAX_NESTING: usize = 64;
const MAX_HEADER_TOKENS: usize = 512;
const MAX_INTERPOLATION_DEPTH: usize = 64;
const MAX_MODIFIERS: usize = 8;
const MAX_NAME_BYTES: usize = 256;

/// Why the parser refused the file.
///
/// Every variant is a whole-file abstention: the frontend keeps its module
/// unit, records this reason, and emits no anchor. There is deliberately no
/// partial outcome, because a recovered tree cannot prove a declaration —
/// once the token stream diverges, every later boundary in the file is
/// unproven.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A `"…"` literal left open at end of line, or a `"""…"""` literal left
    /// open at end of file. Both are decidable well-formedness violations.
    UnterminatedString,
    /// A `/* … */` comment left open. Swift nests block comments, so an open
    /// one makes every later boundary unreliable.
    UnterminatedBlockComment,
    /// A backtick-quoted identifier left open at end of line.
    UnterminatedBacktickName,
    /// `#"…"`, `##"…"##`, and the rest of the raw-string family. Swift 5.0
    /// lexes them, but this frontend does not carry a second delimiter
    /// grammar for them, so the file abstains rather than mis-reading one.
    RawStringLiteral,
    /// A non-ASCII byte in code position. Swift classifies identifier
    /// characters with the Unicode standard, and this byte lexer does not, so
    /// it refuses the construct rather than splitting an identifier wrongly.
    /// Comments and string literals are unaffected: their bytes are never
    /// classified.
    NonAsciiCode,
    /// A `/` in expression-start position. From Swift 5.7 such bytes can open
    /// a regex literal; before 5.7 they lex as operators. The two readings
    /// disagree about where a string starts, so this is a genuine
    /// declared-set divergence and the file abstains under
    /// `swift_dialect_invariance`.
    RegexLiteral,
    /// `#if`, `#else`, `#elseif`, or `#endif`. Conditional compilation
    /// selects a branch from a build configuration this frontend does not
    /// evaluate, and an inactive branch is not fully parsed even by the real
    /// compiler, so no declaration in the file may anchor.
    ConditionalCompilation,
    /// An attribute carrying a non-empty argument list at a declaration head,
    /// such as `@available(iOS 15, *)`. The arguments can change what the
    /// declaration means and are outside the declared subset.
    AttributeArgument,
    /// A generic parameter list on a declaration this frontend parses:
    /// `func testFoo<T>(…)`, `class Suite<T: …>`. A generic subclass or
    /// method is outside the anchor ADR-0048 admits.
    GenericDeclaration,
    /// A `var`/`let` whose initializer head reaches a `{`: a computed
    /// property, a property with accessors, or a stored closure. The brace
    /// the head reaches is not a body this frontend knows how to bound.
    UnadmittedAccessor,
    /// A class body member that is not one of the admitted member kinds. The
    /// extents of the members around it are then unproven.
    UnadmittedMemberShape,
    /// A file-level construct outside the admitted set.
    UnadmittedFileConstruct,
    /// A declaration header this frontend requires to be single-line was
    /// continued onto the next line.
    UnadmittedDeclarationHeader,
    /// The token stream left the admitted grammar: an unclosed block, a stray
    /// closer, or a head that never opened a body.
    UnbalancedBlockStructure,
    /// A `#` followed by something this frontend does not read.
    UnadmittedHashConstruct,
    /// A bounded ceiling was reached.
    ResourceLimit,
}

impl Refusal {
    /// The bounded claim the refusal records.
    pub(crate) fn claim(self) -> &'static str {
        match self {
            Self::RegexLiteral => "swift_dialect_invariance",
            Self::ConditionalCompilation => "swift_conditional_compilation",
            _ => "swift_syntax_admission",
        }
    }

    /// The bounded `swift_unknown_kind` token for this refusal.
    ///
    /// Fixed vocabulary: no source text, identifier, or path ever reaches it.
    pub(crate) fn unknown_kind(self) -> &'static str {
        match self {
            Self::UnterminatedString => "unterminated_string_literal",
            Self::UnterminatedBlockComment => "unterminated_block_comment",
            Self::UnterminatedBacktickName => "unterminated_backtick_name",
            Self::RawStringLiteral => "unadmitted_raw_string",
            Self::NonAsciiCode => "unadmitted_non_ascii_code",
            Self::RegexLiteral => "unadmitted_regex_literal",
            Self::ConditionalCompilation => "conditional_compilation_region",
            Self::AttributeArgument => "unadmitted_attribute_argument",
            Self::GenericDeclaration => "generic_declaration",
            Self::UnadmittedAccessor => "unadmitted_accessor",
            Self::UnadmittedMemberShape => "unadmitted_member_shape",
            Self::UnadmittedFileConstruct => "unadmitted_file_construct",
            Self::UnadmittedDeclarationHeader => "unadmitted_declaration_header",
            Self::UnbalancedBlockStructure => "unbalanced_block_structure",
            Self::UnadmittedHashConstruct => "unadmitted_hash_construct",
            Self::ResourceLimit => "parser_resource_limit",
        }
    }

    /// The operator-facing reason for the degraded-parse diagnostic.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::UnterminatedString => {
                "a Swift string literal is left open, so the following token boundaries are unreliable"
            }
            Self::UnterminatedBlockComment => {
                "a Swift block comment is left open at end of file, so any declaration after it was read as comment"
            }
            Self::UnterminatedBacktickName => {
                "a backtick-quoted Swift identifier is left open at end of line"
            }
            Self::RawStringLiteral => {
                "a raw string literal is outside the string forms this frontend admits"
            }
            Self::NonAsciiCode => {
                "a non-ASCII byte appears in code position, and this frontend does not classify Unicode identifier bytes"
            }
            Self::RegexLiteral => {
                "a `/` in expression-start position may open a regex literal or lex as an operator depending on the Swift 5.x version, so the token stream is not decidable"
            }
            Self::ConditionalCompilation => {
                "conditional compilation selects a branch this frontend does not evaluate, so no declaration in the file is proven"
            }
            Self::AttributeArgument => {
                "an attribute carries a non-empty argument list, which is outside the declared subset"
            }
            Self::GenericDeclaration => {
                "a generic declaration is outside the anchor this frontend admits"
            }
            Self::UnadmittedAccessor => {
                "a property head reaches a brace, so its extent is a computed property or closure this frontend does not bound"
            }
            Self::UnadmittedMemberShape => {
                "a class body holds a construct outside the admitted member kinds, so the extents of the members around it are unproven"
            }
            Self::UnadmittedFileConstruct => {
                "the source left the declaration grammar this frontend admits, so no declaration was proven"
            }
            Self::UnadmittedDeclarationHeader => {
                "a declaration header this frontend requires to be single-line was continued onto the next line"
            }
            Self::UnbalancedBlockStructure => {
                "Swift block structure does not close, so the declarations after the failure point were never parsed"
            }
            Self::UnadmittedHashConstruct => "a `#` construct is not one this frontend reads",
            Self::ResourceLimit => "Swift source exceeded a bounded parser ceiling",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    /// An identifier or keyword. Swift is case-sensitive, so matching is too.
    Word,
    /// A backtick-quoted identifier. It denotes the same symbol as the bare
    /// spelling but is not the source-visible bare name this frontend's
    /// anchor claims, so it never matches a keyword or a test prefix.
    Backticked,
    /// A numeric literal, consumed opaquely.
    Number,
    /// A `"…"` or `"""…"""` literal, consumed opaquely including its
    /// interpolation holes. Its bytes never reach a claim.
    String,
    /// One punctuation character. Multi-character operators are not
    /// combined: no admitted rule depends on operator spelling.
    Punct(char),
    /// End of a physical line.
    Newline,
    /// `#name` — a compiler construct such as `#selector` or `#warning`.
    /// `#if` and its family are refused by the lexer before a token exists.
    HashWord,
}

#[derive(Debug, Clone, Copy)]
struct Token {
    kind: TokenKind,
    start: usize,
    end: usize,
}

impl Token {
    fn is_punct(self, character: char) -> bool {
        self.kind == TokenKind::Punct(character)
    }
}

/// One `import` declaration, reduced to the module it binds.
#[derive(Debug)]
pub(crate) struct ImportClause {
    /// The first component of the imported path: the module name.
    pub(crate) module: String,
}

/// A method-like declaration inside a class body.
#[derive(Debug)]
pub(crate) struct MethodDecl {
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// `static` or `class` was among the modifiers. XCTest discovers only
    /// instance methods, so this one never anchors.
    pub(crate) is_static: bool,
    /// The bare name starts with the exact lowercase prefix `test` and has at
    /// least one further identifier character. A backticked name or an
    /// operator name is never a bare-prefix match.
    pub(crate) is_test_prefixed: bool,
    /// The parameter list was exactly `()`.
    pub(crate) empty_params: bool,
    /// The return is absent, `Void`, `Swift.Void`, or `()`.
    pub(crate) void_return: bool,
    /// `rethrows` was among the effects.
    pub(crate) rethrows: bool,
}

/// A class declaration and its direct members.
#[derive(Debug)]
pub(crate) struct ClassDecl {
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// One inheritance-list entry was exactly the bare identifier
    /// `XCTestCase`. Qualified and generic spellings do not count; the
    /// import binding is checked separately, by `xctest.rs`.
    pub(crate) derives_xctestcase: bool,
    pub(crate) methods: Vec<MethodDecl>,
    pub(crate) nested: Vec<ClassDecl>,
}

/// Everything the frontend needs from one admitted file.
#[derive(Debug)]
pub(crate) struct SwiftFile {
    pub(crate) imports: Vec<ImportClause>,
    pub(crate) classes: Vec<ClassDecl>,
    /// A free function at file level spelled like a test method. It is legal
    /// Swift and never an XCTest test, and is reported rather than silently
    /// dropped.
    pub(crate) free_test_function: bool,
}

/// Parse one Swift source file, or refuse it whole.
pub(crate) fn parse_file(text: &str) -> Result<SwiftFile, Refusal> {
    let tokens = lex(text)?;
    let mut parser = Parser {
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
// Lexer
// ---------------------------------------------------------------------------

fn lex(text: &str) -> Result<Vec<Token>, Refusal> {
    let bytes = text.as_bytes();
    let mut tokens: Vec<Token> = Vec::new();
    let mut index = 0usize;
    // The last significant token decides whether a `/` can start a regex
    // literal: a regex may begin exactly where a prefix expression can.
    let mut previous: Option<TokenKind> = None;
    while index < bytes.len() {
        if tokens.len() > MAX_TOKENS {
            return Err(Refusal::ResourceLimit);
        }
        let byte = bytes[index];
        match byte {
            b'\n' => {
                let repeated = tokens
                    .last()
                    .is_some_and(|token| token.kind == TokenKind::Newline);
                if !repeated {
                    tokens.push(Token {
                        kind: TokenKind::Newline,
                        start: index,
                        end: index + 1,
                    });
                    previous = Some(TokenKind::Newline);
                }
                index += 1;
                continue;
            }
            b'\r' => {
                index += 1;
                continue;
            }
            _ if byte.is_ascii_whitespace() => {
                index += 1;
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index += 2;
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            // Swift block comments nest, so an open one swallows to its own
            // matching close and never to the first `*/`.
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                index = skip_block_comment(bytes, index)?;
                continue;
            }
            b'/' => {
                if regex_can_start(previous) {
                    return Err(Refusal::RegexLiteral);
                }
                push_punct(&mut tokens, &mut previous, byte, index);
                index += 1;
                continue;
            }
            b'"' => {
                let end = scan_string(bytes, index)?;
                tokens.push(Token {
                    kind: TokenKind::String,
                    start: index,
                    end,
                });
                previous = Some(TokenKind::String);
                index = end;
                continue;
            }
            b'`' => {
                let end = scan_backtick_name(bytes, index)?;
                tokens.push(Token {
                    kind: TokenKind::Backticked,
                    start: index,
                    end,
                });
                previous = Some(TokenKind::Backticked);
                index = end;
                continue;
            }
            b'#' => match bytes.get(index + 1) {
                Some(&b'"') => return Err(Refusal::RawStringLiteral),
                Some(next) if next.is_ascii_alphabetic() => {
                    let mut end = index + 2;
                    while end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
                        end += 1;
                    }
                    // Conditional compilation is refused wherever it appears:
                    // the selected branch depends on a build configuration
                    // nothing in the file fixes.
                    if matches!(&text[index + 1..end], "if" | "else" | "elseif" | "endif") {
                        return Err(Refusal::ConditionalCompilation);
                    }
                    tokens.push(Token {
                        kind: TokenKind::HashWord,
                        start: index,
                        end,
                    });
                    previous = Some(TokenKind::HashWord);
                    index = end;
                }
                _ => return Err(Refusal::UnadmittedHashConstruct),
            },
            _ if byte.is_ascii_alphabetic() || byte == b'_' => {
                let mut end = index + 1;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
                {
                    end += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Word,
                    start: index,
                    end,
                });
                previous = Some(TokenKind::Word);
                index = end;
                continue;
            }
            _ if byte.is_ascii_digit() => {
                let mut end = index + 1;
                while end < bytes.len()
                    && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'.')
                {
                    end += 1;
                }
                tokens.push(Token {
                    kind: TokenKind::Number,
                    start: index,
                    end,
                });
                previous = Some(TokenKind::Number);
                index = end;
                continue;
            }
            // A non-ASCII byte in code position. Swift identifiers may carry
            // Unicode, and this byte lexer would split such an identifier at
            // the first multi-byte character, so the file abstains instead of
            // guessing a boundary.
            _ if byte >= 0x80 => return Err(Refusal::NonAsciiCode),
            _ => {
                push_punct(&mut tokens, &mut previous, byte, index);
                index += 1;
            }
        }
    }
    Ok(tokens)
}

fn push_punct(tokens: &mut Vec<Token>, previous: &mut Option<TokenKind>, byte: u8, index: usize) {
    tokens.push(Token {
        kind: TokenKind::Punct(byte as char),
        start: index,
        end: index + 1,
    });
    *previous = Some(TokenKind::Punct(byte as char));
}

/// True when a `/` at this position could open a regex literal: exactly where
/// a prefix expression can start, which is at the beginning of input, after
/// an operator, or after an opening bracket — and never after an identifier,
/// a literal, or a closing bracket, where `/` is division. This is the Swift
/// lexer's own contextual rule, used here only to *refuse* the ambiguous
/// bytes rather than to read them.
fn regex_can_start(previous: Option<TokenKind>) -> bool {
    match previous {
        None | Some(TokenKind::HashWord) | Some(TokenKind::Newline) => true,
        Some(TokenKind::Word)
        | Some(TokenKind::Backticked)
        | Some(TokenKind::Number)
        | Some(TokenKind::String) => false,
        Some(TokenKind::Punct(character)) => !matches!(character, ')' | ']' | '}'),
    }
}

/// Consume a `/* … */` comment, honouring Swift's nesting.
fn skip_block_comment(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 2;
    let mut depth = 1usize;
    while index < bytes.len() {
        if bytes[index] == b'/' && bytes.get(index + 1) == Some(&b'*') {
            depth += 1;
            index += 2;
            continue;
        }
        if bytes[index] == b'*' && bytes.get(index + 1) == Some(&b'/') {
            depth -= 1;
            index += 2;
            if depth == 0 {
                return Ok(index);
            }
            continue;
        }
        index += 1;
    }
    Err(Refusal::UnterminatedBlockComment)
}

/// The mode stack for string scanning. Swift interpolation holes nest
/// arbitrary expressions, including their own string literals, so the scanner
/// carries an explicit stack instead of recursing: untrusted input must not
/// reach the call stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StringMode {
    /// Inside a string body. `multiline` decides whether a newline is content
    /// and whether a triple quote closes it.
    Str { multiline: bool },
    /// Inside a `\(` hole. `depth` counts the parentheses of the hole
    /// expression.
    Hole { depth: usize },
}

/// Consume a string literal starting at `start`, including its interpolation
/// holes and any literals nested inside them. Returns the offset just past
/// the closing quote.
fn scan_string(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let multiline = bytes.get(start + 1) == Some(&b'"') && bytes.get(start + 2) == Some(&b'"');
    let mut index = if multiline { start + 3 } else { start + 1 };
    let mut stack = vec![StringMode::Str { multiline }];
    while index < bytes.len() {
        let byte = bytes[index];
        match stack.last().copied().expect("stack is never empty") {
            StringMode::Str { multiline } => match byte {
                b'\\' => {
                    if bytes.get(index + 1) == Some(&b'(') {
                        if stack.len() > MAX_INTERPOLATION_DEPTH {
                            return Err(Refusal::ResourceLimit);
                        }
                        stack.push(StringMode::Hole { depth: 1 });
                        index += 2;
                        continue;
                    }
                    // Any other escape — `\"`, `\\`, `\u{…}` — is skipped
                    // atomically so its bytes cannot close the string or
                    // contribute a brace.
                    index += 2;
                }
                b'"' if multiline => {
                    if bytes.get(index + 1) == Some(&b'"') && bytes.get(index + 2) == Some(&b'"') {
                        stack.pop();
                        if stack.is_empty() {
                            return Ok(index + 3);
                        }
                        index += 3;
                    } else {
                        // A single `"` or `""` inside a multiline literal is
                        // content, not a delimiter.
                        index += 1;
                    }
                }
                b'"' => {
                    stack.pop();
                    if stack.is_empty() {
                        return Ok(index + 1);
                    }
                    index += 1;
                }
                b'\n' if !multiline => return Err(Refusal::UnterminatedString),
                _ => index += 1,
            },
            StringMode::Hole { depth } => match byte {
                b'(' => {
                    if let Some(StringMode::Hole { depth }) = stack.last_mut() {
                        *depth += 1;
                    }
                    index += 1;
                }
                b')' => {
                    if depth == 1 {
                        stack.pop();
                    } else if let Some(StringMode::Hole { depth }) = stack.last_mut() {
                        *depth -= 1;
                    }
                    index += 1;
                }
                b'"' => {
                    if stack.len() > MAX_INTERPOLATION_DEPTH {
                        return Err(Refusal::ResourceLimit);
                    }
                    let nested =
                        bytes.get(index + 1) == Some(&b'"') && bytes.get(index + 2) == Some(&b'"');
                    stack.push(StringMode::Str { multiline: nested });
                    index += if nested { 3 } else { 1 };
                }
                // A backslash inside a hole is a keypath or an escape in a
                // construct this scanner treats opaquely; skipping it cannot
                // move a paren or a quote boundary.
                b'\\' => index += 2,
                _ => index += 1,
            },
        }
    }
    Err(Refusal::UnterminatedString)
}

/// Consume a backtick-quoted identifier. Such a name never spans lines.
fn scan_backtick_name(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'`' => return Ok(index + 1),
            b'\n' => return Err(Refusal::UnterminatedBacktickName),
            _ => index += 1,
        }
    }
    Err(Refusal::UnterminatedBacktickName)
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Declaration modifiers this frontend knows. Anything else in modifier
/// position is an unadmitted construct, so a keyword a later Swift release
/// adds abstains rather than mis-nesting — the same release-difference answer
/// ADR-0046 gives for MATLAB class bodies.
const TYPE_MODIFIERS: &[&str] = &[
    "public",
    "open",
    "internal",
    "private",
    "fileprivate",
    "final",
];

const MEMBER_MODIFIERS: &[&str] = &[
    "public",
    "open",
    "internal",
    "private",
    "fileprivate",
    "static",
    "class",
    "final",
    "override",
    "mutating",
    "nonmutating",
    "weak",
    "unowned",
    "lazy",
    "required",
    "convenience",
    "dynamic",
    "nonisolated",
];

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

/// The characters an operator function name may be built from. `func ==`,
/// `func <`, and custom operators are admitted declarations that never
/// anchor; refusing them would abstain on every Equatable helper.
const OPERATOR_NAME_CHARS: &[char] = &[
    '/', '=', '-', '+', '!', '*', '%', '<', '>', '&', '|', '^', '~', '?',
];

struct Parser<'a> {
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
        self.bump();
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

/// The exact name rule from the XCTest contract, narrowed the way ADR-0025
/// D6 and ADR-0048 state it: the bare source-visible name starts with the
/// lowercase ASCII prefix `test` and carries at least one further identifier
/// character.
fn is_test_method_name(word: &str) -> bool {
    word.len() > 4 && word.starts_with("test")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<SwiftFile, Refusal> {
        parse_file(text)
    }

    fn test_class(source: &str) -> String {
        format!("import XCTest\nfinal class CatalogTests: XCTestCase {{\n{source}}}\n")
    }

    // -- lexer: strings and comments never move a boundary -------------------

    #[test]
    fn interpolation_holes_containing_braces_and_quotes_do_not_disturb_nesting() {
        let parsed = parse(&test_class(
            "    func testBuildsMessage() {\n\
             \x20       let message = \"value \\(dictionary[\"key\"] ?? \"fallback {\") end\"\n\
             \x20       let label = \"count \\(items.filter { $0 > 1 }.count)\"\n\
             \x20   }\n",
        ))
        .expect("parse");
        assert_eq!(parsed.classes[0].methods.len(), 1);
    }

    #[test]
    fn multiline_strings_swallow_braces_and_their_own_quotes() {
        let parsed = parse(&test_class(
            "    func testRendersTemplate() {\n\
             \x20       let template = \"\"\"\n\
             \x20       func fake() { }
             \x20       \"quoted \\(1 + 1) text\"
             \x20       \"\"\"\n\
             \x20   }\n",
        ))
        .expect("parse");
        assert!(parsed.classes[0].methods[0].is_test_prefixed);
    }

    #[test]
    fn nested_block_comments_and_line_comments_never_declare_anything() {
        let mut parsed = parse(&test_class(
            "    /* outer /* inner */ func testHidden() { } */\n\
             \x20   // func testAlsoHidden() { }\n\
             \x20   func testVisible() { }\n",
        ))
        .expect("parse");
        let class = parsed.classes.swap_remove(0);
        assert_eq!(class.methods.len(), 1);
        assert!(class.methods[0].is_test_prefixed);
    }

    #[test]
    fn an_unterminated_string_comment_or_backtick_name_refuses_the_file() {
        assert_eq!(
            parse(&test_class("    let s = \"never closed\n")).unwrap_err(),
            Refusal::UnterminatedString
        );
        assert_eq!(
            parse(&test_class("    /* never closed\n")).unwrap_err(),
            Refusal::UnterminatedBlockComment
        );
        assert_eq!(
            parse(&test_class("    let x = `name\n")).unwrap_err(),
            Refusal::UnterminatedBacktickName
        );
    }

    #[test]
    fn raw_strings_non_ascii_code_and_unexpected_hash_forms_refuse() {
        assert_eq!(
            parse(&test_class("    let s = #\"raw\"#\n")).unwrap_err(),
            Refusal::RawStringLiteral
        );
        assert_eq!(
            parse(&test_class("    let π = 3.14159\n")).unwrap_err(),
            Refusal::NonAsciiCode
        );
        assert_eq!(
            parse("import XCTest\nlet x = #1\n").unwrap_err(),
            Refusal::UnadmittedHashConstruct
        );
    }

    /// A `/` where an expression can start may open a regex literal from
    /// Swift 5.7 and lexes as an operator before that, so the declared 5.x
    /// set does not agree on the token boundary. Division after an operand
    /// is not expression-start and still parses.
    #[test]
    fn a_regex_literal_position_refuses_but_division_parses() {
        assert_eq!(
            parse(&test_class("    let pattern = /a{2,}/\n")).unwrap_err(),
            Refusal::RegexLiteral
        );
        assert!(parse(&test_class(
            "    func testSplits() {\n        let half = total / 2\n    }\n"
        ))
        .is_ok());
    }

    #[test]
    fn unicode_inside_strings_and_comments_is_content_not_code() {
        assert!(parse(&test_class(
            "    func testLocalized() {\n\
             \x20       let greeting = \"目录カタログ\"\n\
             \x20       // コメント with ünïcöde\n\
             \x20   }\n"
        ))
        .is_ok());
    }

    // -- grammar: the admitted declaration shapes -----------------------------

    #[test]
    fn a_class_header_must_stay_on_one_line() {
        assert_eq!(
            parse("import XCTest\nclass CatalogTests:\n    XCTestCase {\n}\n").unwrap_err(),
            Refusal::UnadmittedDeclarationHeader
        );
        assert!(parse("import XCTest\nclass CatalogTests: XCTestCase {\n}\n").is_ok());
    }

    #[test]
    fn a_wrapped_parameter_list_is_one_head_but_a_wrapped_return_is_not() {
        assert!(parse(&test_class(
            "    private func helper(\n        _ value: Int,\n        _ scale: Int) {\n    }\n"
        ))
        .is_ok());
        assert_eq!(
            parse(&test_class(
                "    func testLoads()\n        -> Void {\n    }\n"
            ))
            .unwrap_err(),
            Refusal::UnadmittedDeclarationHeader
        );
    }

    #[test]
    fn inheritance_entries_are_read_for_the_bare_testcase_spelling_only() {
        let single = parse("import XCTest\nclass A: XCTestCase {\n}\n").expect("parse");
        assert!(single.classes[0].derives_xctestcase);
        let listed =
            parse("import XCTest\nclass B: ObservableObject, XCTestCase {\n}\n").expect("parse");
        assert!(listed.classes[0].derives_xctestcase);
        let qualified =
            parse("import XCTest\nclass D: Other.XCTestCase, XCTestCaseSubclass {\n}\n")
                .expect("parse");
        assert!(!qualified.classes[0].derives_xctestcase);
        let none = parse("import XCTest\nclass E: ObservableObject {\n}\n").expect("parse");
        assert!(!none.classes[0].derives_xctestcase);
    }

    #[test]
    fn generics_and_attribute_arguments_on_declarations_refuse() {
        assert_eq!(
            parse(&test_class("    func testGeneric<T>() {\n    }\n")).unwrap_err(),
            Refusal::GenericDeclaration
        );
        assert_eq!(
            parse("import XCTest\nclass S<T: XCTestCase> {\n}\n").unwrap_err(),
            Refusal::GenericDeclaration
        );
        assert_eq!(
            parse(&test_class(
                "    @available(iOS 15, *)\n    func testNew() {\n    }\n"
            ))
            .unwrap_err(),
            Refusal::AttributeArgument
        );
        // A bare attribute or an exactly empty argument list is admitted.
        assert!(parse(&test_class(
            "    @discardableResult\n    @objc()\n    func testPlain() {\n    }\n"
        ))
        .is_ok());
    }

    #[test]
    fn accessor_and_closure_initializers_refuse_but_plain_properties_parse() {
        assert_eq!(
            parse(&test_class(
                "    var count: Int {\n        get { 1 }\n    }\n"
            ))
            .unwrap_err(),
            Refusal::UnadmittedAccessor
        );
        assert_eq!(
            parse(&test_class("    let onClick = { print(1) }\n")).unwrap_err(),
            Refusal::UnadmittedAccessor
        );
        assert!(parse(&test_class(
            "    static let items: [Int] = []\n    private var total = 0\n"
        ))
        .is_ok());
    }

    #[test]
    fn unadmitted_class_members_and_file_constructs_refuse() {
        assert_eq!(
            parse(&test_class(
                "    subscript(index: Int) -> Int {\n        1\n    }\n"
            ))
            .unwrap_err(),
            Refusal::UnadmittedMemberShape
        );
        assert_eq!(
            parse("import XCTest\nprecedencegroup TestPrecedence {\n}\n").unwrap_err(),
            Refusal::UnadmittedFileConstruct
        );
    }

    #[test]
    fn opaque_file_level_types_are_consumed_without_claims() {
        let parsed = parse(
            "import XCTest\n\
             private struct Stub: Identifiable {\n\
             \x20   var value = 1\n\
             }\n\
             extension Stub {\n\
             \x20   func method() { }\n\
             }\n\
             final class CatalogTests: XCTestCase {\n\
             \x20   func testLoadsCatalog() { }\n\
             }\n",
        )
        .expect("parse");
        assert_eq!(parsed.classes.len(), 1);
        assert!(parsed.classes[0].derives_xctestcase);
    }

    #[test]
    fn conditional_compilation_refuses_wherever_it_appears() {
        assert_eq!(
            parse("#if DEBUG\nimport XCTest\n#endif\n").unwrap_err(),
            Refusal::ConditionalCompilation
        );
        assert_eq!(
            parse(&test_class(
                "    #if os(Linux)\n        skip()\n    #endif\n"
            ))
            .unwrap_err(),
            Refusal::ConditionalCompilation
        );
    }

    #[test]
    fn unbalanced_structure_and_resource_ceilings_refuse() {
        assert_eq!(
            parse("import XCTest\nclass A: XCTestCase {\n").unwrap_err(),
            Refusal::UnbalancedBlockStructure
        );
        assert_eq!(
            parse("import XCTest\n}\n").unwrap_err(),
            Refusal::UnbalancedBlockStructure
        );
        let deep = format!(
            "import XCTest\nclass A: XCTestCase {{\n{}{}\n",
            "    class B {\n".repeat(200),
            "    }\n".repeat(200)
        );
        assert_eq!(parse(&deep).unwrap_err(), Refusal::ResourceLimit);
    }

    #[test]
    fn modifier_and_method_shapes_are_tracked() {
        let mut parsed = parse(&test_class(
            "    override func setUp() { super.setUp() }\n\
             \x20   static func testStatic() { }\n\
             \x20   class func testClassLevel() { }\n\
             \x20   func testWithParameter(x: Int) -> Int { x }\n\
             \x20   private func testPrivate() { }\n\
             \x20   func testThrows() throws { }\n\
             \x20   func testAsync() async throws { }\n\
             \x20   func helper() { }\n",
        ))
        .expect("parse");
        let class = parsed.classes.swap_remove(0);
        let shapes = class
            .methods
            .iter()
            .map(|method| {
                (
                    method.is_test_prefixed,
                    method.is_static,
                    method.empty_params,
                    method.void_return,
                    method.rethrows,
                )
            })
            .collect::<Vec<_>>();
        assert!(shapes.contains(&(true, true, true, true, false)), "static");
        assert!(
            shapes.contains(&(true, false, false, false, false)),
            "parameterized with a non-Void return"
        );
        assert!(
            shapes.contains(&(true, false, true, true, false)),
            "private"
        );
        assert!(
            shapes.contains(&(false, false, true, true, false)),
            "helper"
        );
    }

    #[test]
    fn operator_and_backticked_names_never_match_the_bare_prefix() {
        let mut parsed = parse(&test_class(
            "    static func == (lhs: Self, rhs: Self) -> Bool { true }\n\
             \x20   func < (other: Self) -> Bool { false }\n\
             \x20   func `testBackticked`() { }\n",
        ))
        .expect("parse");
        let class = parsed.classes.swap_remove(0);
        assert_eq!(class.methods.len(), 3);
        assert!(
            class.methods.iter().all(|method| !method.is_test_prefixed),
            "operator and backticked names are not bare test names"
        );
    }

    #[test]
    fn a_nested_class_is_a_declaration_not_a_modifier() {
        let mut parsed = parse(&test_class(
            "    class Inner: XCTestCase {\n\
             \x20       func testInner() { }\n\
             \x20   }\n\
             \x20   class func testClassLevel() { }\n",
        ))
        .expect("parse");
        let class = parsed.classes.swap_remove(0);
        assert_eq!(class.nested.len(), 1);
        assert!(class.nested[0].derives_xctestcase);
        assert!(class
            .methods
            .iter()
            .any(|method| method.is_test_prefixed && method.is_static));
    }

    #[test]
    fn a_free_test_function_is_recorded_as_a_lookalike() {
        let parsed =
            parse("import XCTest\nfunc testFreeStanding() {\n    print(1)\n}\n").expect("parse");
        assert!(parsed.free_test_function);
        let plain = parse("import XCTest\nfunc helper() {\n}\n").expect("parse");
        assert!(!plain.free_test_function);
    }

    #[test]
    fn void_return_spellings_are_the_admitted_three_plus_absent() {
        let mut parsed = parse(&test_class(
            "    func testA() { }\n\
             \x20   func testB() -> Void { }\n\
             \x20   func testC() -> Swift.Void { }\n\
             \x20   func testD() -> () { }\n\
             \x20   func testE() async -> Void { }\n\
             \x20   func testF() throws -> Int { 1 }\n",
        ))
        .expect("parse");
        let class = parsed.classes.swap_remove(0);
        let voids = class
            .methods
            .iter()
            .map(|method| method.void_return)
            .collect::<Vec<_>>();
        assert_eq!(voids, vec![true, true, true, true, true, false]);
    }

    #[test]
    fn imports_record_the_bound_module_including_kind_and_testable_forms() {
        let parsed =
            parse("import XCTest\n@testable import CatalogKit\nimport class XCTest.XCTestCase\n")
                .expect("parse");
        let modules = parsed
            .imports
            .iter()
            .map(|import| import.module.as_str())
            .collect::<Vec<_>>();
        assert_eq!(modules, vec!["XCTest", "CatalogKit", "XCTest"]);
    }

    #[test]
    fn an_empty_or_whitespace_file_yields_no_declarations() {
        for text in ["", "\n\n", "// only a comment\n"] {
            let parsed = parse(text).expect("parse");
            assert!(parsed.classes.is_empty());
            assert!(parsed.imports.is_empty());
        }
    }
}
