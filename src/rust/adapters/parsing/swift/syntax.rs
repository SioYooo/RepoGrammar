//! Bounded Swift syntax layer for ADR-0048, reading declarations from the
//! pinned Tree-sitter Swift grammar (ADR-0058).
//!
//! This module owns syntax only. It decides what the source *is*; it decides
//! nothing about XCTest and names no framework. `xctest.rs` walks the
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
//! later Swift release adds is equally refused: the reader does not guess at
//! it, so it cannot silently mis-nest.
//!
//! The ADR-0048 lexer runs first as the lexical-subset gate and decides every
//! lexical refusal (open literals, raw strings, non-ASCII code, regex
//! position, conditional compilation) before the grammar is consulted. The
//! grammar then supplies the declaration structure, and the ADR-0048 subset
//! refusals (generics, attribute arguments, accessors, member and file
//! shapes, single-line headers, brace balance, ceilings) are read off the tree
//! and the token stream. As in a left-to-right parse the earliest refusal
//! wins. The reader abstains; it never recovers: any `ERROR` or `MISSING`
//! node refuses the file, so a partial or recovered tree can never become an
//! anchor. This discharges ADR-0025 D7's no-text-matching requirement with a
//! real parse, not a scanner.
//!
//! Nothing here invokes `swift`, `swiftc`, `swift-frontend`, `sourcekit-lsp`,
//! SwiftPM, Xcode, `xcodebuild`, a macro, a plugin, a package script, a child
//! process, or the network.

use crate::ports::parser::ParseError;
use tree_sitter::{Node, Parser};

// Temporary ADR-0058 differential scaffolding: the legacy parser and its
// comparison tests are deleted once the fixture-wide capture confirms the
// migration.
#[cfg(test)]
#[path = "differential_tmp.rs"]
mod differential;
#[cfg(test)]
#[path = "legacy_syntax.rs"]
mod legacy;

#[cfg(test)]
thread_local! {
    /// Routes `parse_file` to the legacy parser for the differential tests:
    /// 0 reads the tree, 1 runs the legacy parser, 2 runs it with its
    /// `init`/`deinit` head defect repaired.
    static USE_LEGACY: std::cell::Cell<u8> = const { std::cell::Cell::new(0) };
}

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
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ImportClause {
    /// The first component of the imported path: the module name.
    pub(crate) module: String,
}

/// A method-like declaration inside a class body.
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
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
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SwiftFile {
    pub(crate) imports: Vec<ImportClause>,
    pub(crate) classes: Vec<ClassDecl>,
    /// A free function at file level spelled like a test method. It is legal
    /// Swift and never an XCTest test, and is reported rather than silently
    /// dropped.
    pub(crate) free_test_function: bool,
}

/// Parse one Swift source file, or refuse it whole.
///
/// The outer `Result` reports a grammar that cannot load or a parse that did
/// not complete; neither is a refusal of the file. The ADR-0048 lexer runs
/// first as the lexical-subset gate, so every lexical refusal is decided
/// before the grammar is consulted.
pub(crate) fn parse_file(text: &str) -> Result<Result<SwiftFile, Refusal>, ParseError> {
    #[cfg(test)]
    match USE_LEGACY.with(std::cell::Cell::get) {
        1 => return Ok(legacy::parse_file(text)),
        2 => return Ok(legacy::parse_file_patched(text)),
        _ => {}
    }
    let tokens = match lex(text) {
        Ok(tokens) => tokens,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_swift::LANGUAGE.into())
        .map_err(|error| ParseError::Internal(format!("load Swift grammar: {error}")))?;
    let tree = parser
        .parse(text, None)
        .ok_or_else(|| ParseError::Internal("Tree-sitter Swift parse failed".to_string()))?;
    Ok(read_file(text, &tokens, tree.root_node()))
}

// ---------------------------------------------------------------------------
// Tree reader
// ---------------------------------------------------------------------------

/// Where a declaration sits decides its modifier table and which refusal an
/// unadmitted modifier or member carries.
#[derive(Clone, Copy)]
enum Site {
    File,
    Member,
}

impl Site {
    fn modifiers(self) -> &'static [&'static str] {
        match self {
            Self::File => TYPE_MODIFIERS,
            Self::Member => MEMBER_MODIFIERS,
        }
    }

    fn shape(self) -> Refusal {
        match self {
            Self::File => Refusal::UnadmittedFileConstruct,
            Self::Member => Refusal::UnadmittedMemberShape,
        }
    }
}

/// The attribute names and modifier words standing before a declaration
/// keyword.
#[derive(Default)]
struct Prefix<'a> {
    attributes: Vec<&'a str>,
    modifiers: Vec<&'a str>,
}

struct Reader<'a> {
    text: &'a str,
    tokens: &'a [Token],
    /// Refusals tagged with the source position a left-to-right parse would
    /// reach them at; the earliest wins.
    refusals: Vec<(usize, Refusal)>,
    /// Refusals for a node kind the reader does not admit. Error recovery can
    /// surface a misplaced node of any kind, so these count only in a tree
    /// without an `ERROR` or `MISSING` node.
    unadmitted: Vec<(usize, Refusal)>,
    imports: Vec<ImportClause>,
    free_test_function: bool,
}

/// Read the admitted declarations off the Tree-sitter tree and refuse the file
/// on anything outside the ADR-0048 subset. As in a left-to-right parse the
/// earliest refusal wins, and at one position a token-level refusal precedes a
/// tree refusal. Any `ERROR` or `MISSING` node refuses the file, so a recovered
/// tree never yields an anchor; error recovery decides where such a node sits,
/// so it refuses as an unadmitted file construct only when no token or
/// structural refusal applies.
fn read_file(text: &str, tokens: &[Token], root: Node<'_>) -> Result<SwiftFile, Refusal> {
    let mut reader = Reader {
        text,
        tokens,
        refusals: brace_refusals(tokens),
        unadmitted: Vec::new(),
        imports: Vec::new(),
        free_test_function: false,
    };
    let mut classes = Vec::new();
    let mut cursor = root.walk();
    for node in root.named_children(&mut cursor) {
        match node.kind() {
            "comment" | "multiline_comment" | "ERROR" => {}
            "import_declaration" => reader.import(node),
            "class_declaration" if is_class_declaration(node) => {
                classes.extend(reader.class(node, Site::File, 0));
            }
            "class_declaration" | "protocol_declaration" => reader.opaque_type(node),
            "function_declaration" => {
                if let Some(method) = reader.function(node, Site::File) {
                    reader.free_test_function |=
                        method.is_test_prefixed && method.empty_params && method.void_return;
                }
            }
            "property_declaration" | "typealias_declaration" => {
                reader.simple_head(node, Site::File);
            }
            _ if reader.starts_with_hash(node) => {}
            _ => reader.unadmitted_node(node, Site::File),
        }
    }
    if !root.has_error() {
        reader.refusals.append(&mut reader.unadmitted);
    }
    // Stable: at one position the earlier-recorded token refusal wins.
    reader.refusals.sort_by_key(|(position, _)| *position);
    if let Some((_, refusal)) = reader.refusals.first() {
        return Err(*refusal);
    }
    if root.has_error() {
        return Err(Refusal::UnadmittedFileConstruct);
    }
    Ok(SwiftFile {
        imports: reader.imports,
        classes,
        free_test_function: reader.free_test_function,
    })
}

/// Brace balance over the whole token stream. A stray closer refuses at its
/// own position; an unclosed opener refuses at end of input, because a
/// left-to-right parse swallows the rest of the file into one unproven extent
/// and every earlier refusal is still reached first.
fn brace_refusals(tokens: &[Token]) -> Vec<(usize, Refusal)> {
    let mut depth = 0usize;
    for token in tokens {
        if token.is_punct('{') {
            depth += 1;
        } else if token.is_punct('}') {
            if depth == 0 {
                return vec![(token.start, Refusal::UnbalancedBlockStructure)];
            }
            depth -= 1;
        }
    }
    if depth > 0 {
        vec![(usize::MAX, Refusal::UnbalancedBlockStructure)]
    } else {
        Vec::new()
    }
}

fn is_class_declaration(node: Node<'_>) -> bool {
    node.child_by_field_name("declaration_kind")
        .is_some_and(|keyword| keyword.kind() == "class")
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .find(|child| child.kind() == kind);
    found
}

impl<'a> Reader<'a> {
    fn refuse(&mut self, position: usize, refusal: Refusal) {
        self.refusals.push((position, refusal));
    }

    fn word(&self, node: Node<'_>) -> &'a str {
        let text = self.text;
        &text[node.byte_range()]
    }

    /// The tokens whose start lies in `start..end`.
    fn tokens_in(&self, start: usize, end: usize) -> &'a [Token] {
        let tokens = self.tokens;
        let from = tokens.partition_point(|token| token.start < start);
        let to = tokens.partition_point(|token| token.start < end);
        &tokens[from..to]
    }

    fn tokens_from(&self, start: usize) -> &'a [Token] {
        self.tokens_in(start, usize::MAX)
    }

    /// A `#name …` construct at file level: consumed opaquely, as the lexer
    /// already refused `#if` and its family.
    fn starts_with_hash(&self, node: Node<'_>) -> bool {
        self.tokens_from(node.start_byte())
            .first()
            .is_some_and(|token| token.kind == TokenKind::HashWord)
    }

    /// Attributes and modifiers standing before the declaration keyword. An
    /// attribute binds before the modifiers, so one that follows a modifier is
    /// outside the admitted order; a non-empty attribute argument list and any
    /// modifier outside the site's table refuse.
    fn prefix(&mut self, declaration: Node<'a>, keyword: usize, site: Site) -> Prefix<'a> {
        let mut nodes = Vec::new();
        let mut cursor = declaration.walk();
        for child in declaration.children(&mut cursor) {
            if child.start_byte() >= keyword {
                break;
            }
            match child.kind() {
                "modifiers" => {
                    let mut inner = child.walk();
                    nodes.extend(child.children(&mut inner));
                }
                _ => nodes.push(child),
            }
        }
        let mut prefix = Prefix::default();
        for node in nodes {
            match node.kind() {
                "comment" | "multiline_comment" | "ERROR" => {}
                "attribute" if prefix.modifiers.is_empty() => self.attribute(node, &mut prefix),
                "attribute" => self.refuse(node.start_byte(), site.shape()),
                _ => {
                    let word = self.word(node);
                    if !site.modifiers().contains(&word) {
                        self.refuse(node.start_byte(), site.shape());
                    }
                    prefix.modifiers.push(word);
                    if prefix.modifiers.len() > MAX_MODIFIERS {
                        self.refuse(node.start_byte(), Refusal::ResourceLimit);
                    }
                }
            }
        }
        prefix
    }

    fn attribute(&mut self, node: Node<'a>, prefix: &mut Prefix<'a>) {
        let tokens = self.tokens_in(node.start_byte(), node.end_byte());
        let Some(name) = tokens.get(1).filter(|token| token.kind == TokenKind::Word) else {
            return;
        };
        if name.end - name.start > MAX_NAME_BYTES {
            self.refuse(node.start_byte(), Refusal::ResourceLimit);
        }
        let text = self.text;
        prefix.attributes.push(&text[name.start..name.end]);
        if tokens.get(2).is_some_and(|token| token.is_punct('('))
            && tokens.get(3).is_some_and(|token| !token.is_punct(')'))
        {
            self.refuse(node.start_byte(), Refusal::AttributeArgument);
        }
        if prefix.attributes.len() > MAX_MODIFIERS {
            self.refuse(node.start_byte(), Refusal::ResourceLimit);
        }
    }

    /// A node kind the reader does not admit. The legacy parse read the
    /// attributes and modifiers before it looked at the keyword, so their
    /// refusals still precede this one.
    fn unadmitted_node(&mut self, node: Node<'a>, site: Site) {
        let mut cursor = node.walk();
        let boundary = node
            .children(&mut cursor)
            .find(|child| {
                !(matches!(child.kind(), "modifiers" | "attribute")
                    || child.kind().ends_with("_modifier"))
            })
            .map_or(node.end_byte(), |child| child.start_byte());
        self.prefix(node, boundary, site);
        self.unadmitted.push((node.start_byte(), site.shape()));
    }

    /// `import [kind] dotted.path` — the module is the first path component,
    /// and only a bare or `@testable` import is admitted. The statement ends
    /// the line.
    fn import(&mut self, node: Node<'a>) {
        let Some(keyword) = child_of_kind(node, "import") else {
            return;
        };
        let prefix = self.prefix(node, keyword.start_byte(), Site::File);
        if prefix.attributes.iter().any(|name| *name != "testable") || !prefix.modifiers.is_empty()
        {
            self.refuse(keyword.start_byte(), Refusal::UnadmittedFileConstruct);
            return;
        }
        let Some(module) =
            child_of_kind(node, "identifier").and_then(|identifier| identifier.named_child(0))
        else {
            return;
        };
        let module = self.word(module);
        if module.len() > MAX_NAME_BYTES {
            self.refuse(keyword.start_byte(), Refusal::ResourceLimit);
        }
        self.imports.push(ImportClause {
            module: module.to_string(),
        });
        if let Some(next) = self.tokens_from(node.end_byte()).first() {
            if next.kind != TokenKind::Newline && !next.is_punct(';') {
                self.refuse(next.start, Refusal::UnbalancedBlockStructure);
            }
        }
    }

    /// A `class` declaration: header single-line, not generic, and its direct
    /// members read from the class body.
    fn class(&mut self, node: Node<'a>, site: Site, depth: usize) -> Option<ClassDecl> {
        let keyword = node.child_by_field_name("declaration_kind")?;
        self.prefix(node, keyword.start_byte(), site);
        let name = node.child_by_field_name("name")?;
        let body = node.child_by_field_name("body")?;
        if !self.word(name).starts_with('`') {
            if let Some(generics) = child_of_kind(node, "type_parameters") {
                self.refuse(generics.start_byte(), Refusal::GenericDeclaration);
            }
        }
        self.header_newline(name.end_byte(), body.start_byte(), true);
        let mut derives_xctestcase = false;
        let mut cursor = node.walk();
        for entry in node
            .children(&mut cursor)
            .filter(|child| child.kind() == "inheritance_specifier")
        {
            if self.tokens_in(entry.start_byte(), entry.end_byte()).len() > MAX_HEADER_TOKENS {
                self.refuse(entry.start_byte(), Refusal::ResourceLimit);
            }
            // The anchor claims the bare source-visible spelling: a qualified
            // or generic entry is a different node text and never matches.
            derives_xctestcase |= self.word(entry) == "XCTestCase";
        }
        if depth >= MAX_NESTING {
            self.refuse(body.start_byte(), Refusal::ResourceLimit);
            return None;
        }
        let mut methods = Vec::new();
        let mut nested = Vec::new();
        let mut members = body.walk();
        for member in body.named_children(&mut members) {
            match member.kind() {
                "comment" | "multiline_comment" | "ERROR" => {}
                "function_declaration" => methods.extend(self.function(member, Site::Member)),
                "init_declaration" | "deinit_declaration" => {
                    self.function(member, Site::Member);
                }
                "property_declaration" | "typealias_declaration" => {
                    self.simple_head(member, Site::Member);
                }
                "class_declaration" if is_class_declaration(member) => {
                    nested.extend(self.class(member, Site::Member, depth + 1));
                }
                _ => self.unadmitted_node(member, Site::Member),
            }
        }
        Some(ClassDecl {
            start: node.start_byte(),
            end: node.end_byte(),
            derives_xctestcase,
            methods,
            nested,
        })
    }

    /// A file-level `struct`, `enum`, `actor`, `protocol`, or `extension`:
    /// read for its prefix and bounded head only, never for members.
    fn opaque_type(&mut self, node: Node<'a>) {
        let Some(keyword) = node.child_by_field_name("declaration_kind") else {
            return;
        };
        self.prefix(node, keyword.start_byte(), Site::File);
        let Some(body) = node.child_by_field_name("body") else {
            return;
        };
        if self
            .tokens_in(keyword.start_byte(), body.start_byte())
            .len()
            > MAX_HEADER_TOKENS
        {
            self.refuse(keyword.start_byte(), Refusal::ResourceLimit);
        }
    }

    /// A `func`, `init`, or `deinit` and everything the anchor rule asks of
    /// its head. Newlines are admitted inside the parameter parentheses only.
    fn function(&mut self, node: Node<'a>, site: Site) -> Option<MethodDecl> {
        let mut cursor = node.walk();
        let keyword = node
            .children(&mut cursor)
            .find(|child| matches!(child.kind(), "func" | "init" | "deinit"))?;
        let prefix = self.prefix(node, keyword.start_byte(), site);
        let mut children = node.walk();
        let broken = node
            .children(&mut children)
            .any(|child| child.kind() == "ERROR" || child.is_missing());
        let body = node.child_by_field_name("body");
        let Some(body) = body.filter(|_| !broken) else {
            // Error recovery split or dropped the head; the token stream says
            // whether the body ever opened on the head's line.
            self.unbraced_head(keyword.end_byte());
            return None;
        };
        let mut is_test_prefixed = false;
        let mut name_end = keyword.end_byte();
        if let Some(name) = node.child_by_field_name("name") {
            name_end = name.end_byte();
            let word = self.word(name);
            if word.starts_with('`') {
                if word.len() > MAX_NAME_BYTES {
                    self.refuse(name.start_byte(), Refusal::ResourceLimit);
                }
            } else if word.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
                is_test_prefixed = is_test_method_name(word);
            } else if !word.chars().all(|c| OPERATOR_NAME_CHARS.contains(&c)) {
                self.refuse(name.start_byte(), Refusal::UnbalancedBlockStructure);
            } else if word.len() > MAX_NAME_BYTES {
                self.refuse(name.start_byte(), Refusal::ResourceLimit);
            }
        }
        let mut empty_params = true;
        let mut rethrows = false;
        let mut parts = node.walk();
        for child in node.children(&mut parts) {
            match child.kind() {
                "type_parameters" => {
                    self.refuse(child.start_byte(), Refusal::GenericDeclaration);
                }
                // Typed throws and `where` clauses are Swift 6 and
                // generic-context forms outside the declared 5.x subset.
                "throws_clause" | "type_constraints" => {
                    self.refuse(child.start_byte(), Refusal::UnbalancedBlockStructure);
                }
                "parameter" => empty_params = false,
                "throws" => rethrows |= self.word(child).starts_with("rethrows"),
                _ => {}
            }
        }
        self.header_newline(name_end, body.start_byte(), false);
        let void_return = node
            .child_by_field_name("return_type")
            .is_none_or(|ty| self.is_void_return(ty));
        Some(MethodDecl {
            start: node.start_byte(),
            end: node.end_byte(),
            is_static: prefix
                .modifiers
                .iter()
                .any(|modifier| *modifier == "static" || *modifier == "class"),
            is_test_prefixed,
            empty_params,
            void_return,
            rethrows,
        })
    }

    /// The return type is `Void`, `Swift.Void`, or `()` when its token run is
    /// exactly one of those spellings.
    fn is_void_return(&self, ty: Node<'_>) -> bool {
        let tokens = self.tokens_in(ty.start_byte(), ty.end_byte());
        let text = self.text;
        let words: Vec<&str> = tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Word)
            .map(|token| &text[token.start..token.end])
            .collect();
        match (words.as_slice(), tokens.len()) {
            ([word], 1) => *word == "Void",
            ([first, last], 3) => *first == "Swift" && *last == "Void",
            ([], 2) => tokens[0].is_punct('(') && tokens[1].is_punct(')'),
            _ => false,
        }
    }

    /// A newline outside parentheses (and, for a class header, angle
    /// brackets) between the name and the body `{` continues the header past
    /// the single line this frontend admits.
    fn header_newline(&mut self, from: usize, to: usize, angles: bool) {
        let mut depth = 0usize;
        for token in self.tokens_in(from, to) {
            match token.kind {
                TokenKind::Punct('(') => depth += 1,
                TokenKind::Punct(')') => depth = depth.saturating_sub(1),
                TokenKind::Punct('<') if angles => depth += 1,
                TokenKind::Punct('>') if angles => depth = depth.saturating_sub(1),
                TokenKind::Newline if depth == 0 => {
                    self.refuse(token.start, Refusal::UnadmittedDeclarationHeader);
                    return;
                }
                _ => {}
            }
        }
    }

    /// A head whose grammar node has no body: the legacy reading of such a
    /// head is a newline before any `{` (a header continued past its line) or
    /// a head that never opens a body. Error recovery decides where the node
    /// ends, so the token stream is the authority here.
    fn unbraced_head(&mut self, from: usize) {
        let mut depth = 0usize;
        for token in self.tokens_from(from) {
            match token.kind {
                TokenKind::Punct('(') => depth += 1,
                TokenKind::Punct(')') => depth = depth.saturating_sub(1),
                TokenKind::Punct('{') if depth == 0 => return,
                TokenKind::Newline if depth == 0 => {
                    self.refuse(token.start, Refusal::UnadmittedDeclarationHeader);
                    return;
                }
                _ => {}
            }
        }
        self.refuse(self.text.len(), Refusal::UnbalancedBlockStructure);
    }

    /// A `var`, `let`, or `typealias` head: bounded to one line at
    /// parenthesis depth zero. A `{` at depth zero is a computed property, an
    /// accessor block, or a stored closure.
    fn simple_head(&mut self, node: Node<'a>, site: Site) {
        let keyword = child_of_kind(node, "value_binding_pattern")
            .or_else(|| child_of_kind(node, "typealias"));
        let Some(keyword) = keyword else {
            return;
        };
        self.prefix(node, keyword.start_byte(), site);
        let mut depth = 0usize;
        let mut count = 0usize;
        for token in self.tokens_from(keyword.end_byte()) {
            match token.kind {
                TokenKind::Punct('(') => depth += 1,
                TokenKind::Punct(')') => depth = depth.saturating_sub(1),
                TokenKind::Punct('{') if depth == 0 => {
                    self.refuse(token.start, Refusal::UnadmittedAccessor);
                    return;
                }
                TokenKind::Punct(';') | TokenKind::Newline if depth == 0 => return,
                _ => {}
            }
            count += 1;
            if count > MAX_HEADER_TOKENS {
                self.refuse(token.start, Refusal::ResourceLimit);
                return;
            }
        }
    }
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

/// The characters an operator function name may be built from. `func ==`,
/// `func <`, and custom operators are admitted declarations that never
/// anchor; refusing them would abstain on every Equatable helper.
const OPERATOR_NAME_CHARS: &[char] = &[
    '/', '=', '-', '+', '!', '*', '%', '<', '>', '&', '|', '^', '~', '?',
];

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
        parse_file(text).expect("the Swift grammar loads and parses")
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
