//! Bounded VB.NET lexer and recursive-descent parser for ADR-0043.
//!
//! This module owns syntax only. It decides what the token stream *is*; it
//! decides nothing about MSTest and names no framework. `mstest.rs` walks the
//! tree this produces and applies the anchor rule.
//!
//! The declared invariance set is **VB.NET language versions 10 through 17**
//! (Visual Studio 2010 onward). Every construct admitted below exists, lexes,
//! and nests identically in all of them, so the frontend never selects a
//! version and never needs to. A construct any member of the set lexes
//! differently is refused rather than read under an assumed version.
//!
//! The parser abstains; it never recovers. Outside the admitted grammar it
//! returns a typed [`Refusal`] for the whole file and yields no declaration, so
//! a partial or recovered tree can never become an anchor.
//!
//! Nothing here invokes MSBuild, Roslyn, `vbc`, `dotnet`, NuGet, an analyzer, a
//! source generator, a package script, a child process, or the network.

/// The bounded ceilings this parser works under. Repository input is
/// untrusted, so both the token stream and the recursion are capped.
const MAX_TOKENS: usize = 400_000;
const MAX_NESTING: usize = 64;

/// Why the parser refused the file.
///
/// Every variant is a whole-file abstention: the frontend keeps its module
/// unit, records this reason, and emits no anchor. There is deliberately no
/// partial outcome, because a recovered tree cannot prove a declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A string literal left open at end of line. VB strings do not span
    /// lines, so this is a decidable well-formedness violation.
    UnterminatedString,
    /// `$"…"`. Interpolated strings are VB 14 and later; under VB 10 to 13 the
    /// same characters lex as an operator followed by an ordinary string, so
    /// the token stream is not invariant across the declared set.
    InterpolatedString,
    /// `<` opening an XML literal outside attribute position. An XML literal
    /// may contain any text at all, including lines that read exactly like a
    /// declaration, so the token stream stops being decidable.
    XmlLiteral,
    /// An escaped identifier `[…]` left open at end of line.
    UnterminatedEscapedName,
    /// A `#` directive whose name is not one this frontend reads, or a `#If`
    /// region that never closes.
    UnadmittedDirective,
    /// The token stream left the admitted declaration grammar: an unclosed
    /// block, a mismatched `End`, or a construct outside the declared subset.
    UnadmittedDeclaration,
    /// A bounded ceiling was reached.
    ResourceLimit,
}

impl Refusal {
    /// The bounded `vb_unknown_kind` token for this refusal.
    ///
    /// Fixed vocabulary: no source text, identifier, or path ever reaches it.
    pub(crate) fn unknown_kind(self) -> &'static str {
        match self {
            Self::UnterminatedString => "unterminated_string_literal",
            Self::InterpolatedString => "unadmitted_interpolated_string",
            Self::XmlLiteral => "unadmitted_xml_literal",
            Self::UnterminatedEscapedName => "unterminated_escaped_name",
            Self::UnadmittedDirective => "unadmitted_compiler_directive",
            Self::UnadmittedDeclaration => "unadmitted_declaration_shape",
            Self::ResourceLimit => "parser_resource_limit",
        }
    }

    /// The operator-facing reason for the degraded-parse diagnostic.
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::UnterminatedString => {
                "a VB.NET string literal is left open at end of line, so that line's declarations were not read"
            }
            Self::InterpolatedString => {
                "an interpolated string is not lexed the same way by every VB.NET version this frontend admits"
            }
            Self::XmlLiteral => {
                "an XML literal may contain text that reads as a declaration, so the token stream is not decidable"
            }
            Self::UnterminatedEscapedName => {
                "a VB.NET escaped identifier is left open at end of line"
            }
            Self::UnadmittedDirective => {
                "a `#` compiler directive is not one this frontend reads, so the selected source is unknown"
            }
            Self::UnadmittedDeclaration => {
                "the source left the declaration grammar this frontend admits, so no declaration was proven"
            }
            Self::ResourceLimit => "VB.NET source exceeded a bounded parser ceiling",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    /// An identifier or keyword, lowercased in `text`.
    Word,
    /// A `[…]` escaped identifier. It never matches a keyword, which is
    /// exactly why the language provides it.
    EscapedWord,
    /// A string or date literal, consumed opaquely. Its bytes never reach a
    /// claim.
    Literal,
    /// One punctuation character.
    Punct(char),
    /// End of a logical line.
    EndOfLine,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    /// Lowercased spelling for a word; empty otherwise.
    text: String,
    start: usize,
    end: usize,
    /// Conditional-compilation depth. A declaration at depth greater than zero
    /// is compiled only under a constant this frontend does not evaluate.
    depth: usize,
    /// For `Punct('<')`: the next character starts an XML name.
    xml_suspicious: bool,
}

impl Token {
    fn is_word(&self, word: &str) -> bool {
        self.kind == TokenKind::Word && self.text == word
    }

    fn is_statement_end(&self) -> bool {
        matches!(self.kind, TokenKind::EndOfLine | TokenKind::Punct(':'))
    }
}

/// An `Imports` clause.
pub(crate) struct ImportClause {
    /// Lowercased dotted name.
    pub(crate) name: String,
    /// `Imports Alias = Namespace` binds a different name, so it never makes
    /// the bare attribute spelling resolve.
    pub(crate) aliased: bool,
    pub(crate) conditional: bool,
}

/// One attribute spelling, lowercased for comparison only. VB.NET is
/// case-insensitive, and no spelling here is ever claimed as identity.
pub(crate) struct AttributeRef {
    pub(crate) qualifier: Option<String>,
    pub(crate) name: String,
}

/// A method-like declaration.
pub(crate) struct MethodDecl {
    pub(crate) attributes: Vec<AttributeRef>,
    /// True only for `Sub`. A `Function` returns a value.
    pub(crate) is_sub: bool,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) conditional: bool,
}

/// A type-like declaration and its direct members.
pub(crate) struct TypeDecl {
    pub(crate) attributes: Vec<AttributeRef>,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) conditional: bool,
    pub(crate) methods: Vec<MethodDecl>,
    pub(crate) nested: Vec<TypeDecl>,
}

/// Everything the frontend needs from one admitted file.
#[derive(Default)]
pub(crate) struct VbFile {
    pub(crate) imports: Vec<ImportClause>,
    pub(crate) types: Vec<TypeDecl>,
}

/// Parse one VB.NET source file, or refuse it whole.
pub(crate) fn parse_file(text: &str) -> Result<VbFile, Refusal> {
    let tokens = lex(text)?;
    let mut parser = Parser {
        tokens: &tokens,
        position: 0,
    };
    let block = parser.parse_block(Context::top(), None)?;
    Ok(VbFile {
        imports: block.members.imports,
        types: block.members.types,
    })
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

/// The `#` directives this frontend reads.
///
/// Classification is deliberately asymmetric: liberal about what opens a
/// conditional region, strict about what closes one. A missed open would
/// anchor a declaration the build may not contain, which is unsound; a missed
/// close only leaves the depth high, which understates support and cannot
/// invent a member.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Directive {
    ConditionalOpen,
    ConditionalClose,
    /// `#Region`, `#ExternalSource`, `#Const`, `#Enable`/`#Disable Warning`,
    /// `#Else`, and `#ElseIf` select no branch and change no claim.
    Neutral,
}

fn classify_directive(line: &str) -> Option<Directive> {
    let (first, rest) = directive_word(line);
    if first.eq_ignore_ascii_case("if") {
        return Some(Directive::ConditionalOpen);
    }
    if first.eq_ignore_ascii_case("end") {
        // Only `#End If` closes. `#End Region` and `#End ExternalSource` must
        // not, or a region opened by `#If` would silently reopen the file.
        let (second, _) = directive_word(rest);
        if second.eq_ignore_ascii_case("if") {
            return Some(Directive::ConditionalClose);
        }
        if second.eq_ignore_ascii_case("region") || second.eq_ignore_ascii_case("externalsource") {
            return Some(Directive::Neutral);
        }
        return None;
    }
    [
        "else",
        "elseif",
        "region",
        "externalsource",
        "const",
        "enable",
        "disable",
    ]
    .iter()
    .any(|neutral| first.eq_ignore_ascii_case(neutral))
    .then_some(Directive::Neutral)
}

/// The next maximal ASCII-alphabetic run, and the text after it.
///
/// Taking the alphabetic run rather than the whitespace-delimited word is what
/// lets `#ExternalSource("Catalog.vb", 1)` and `#Region "Tests"` classify by
/// name despite their trailing punctuation.
fn directive_word(text: &str) -> (&str, &str) {
    let trimmed = text.trim_start();
    let end = trimmed
        .find(|character: char| !character.is_ascii_alphabetic())
        .unwrap_or(trimmed.len());
    trimmed.split_at(end)
}

fn lex(text: &str) -> Result<Vec<Token>, Refusal> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut index = 0usize;
    let mut depth = 0usize;
    // Implicit line continuation inside `(` and `{` is VB 10 and later, and is
    // the only continuation this frontend admits besides an explicit `_`. A
    // declaration split any other way falls outside the grammar and abstains.
    let mut bracket = 0usize;
    let mut line_start = true;
    while index < text.len() {
        if tokens.len() > MAX_TOKENS {
            return Err(Refusal::ResourceLimit);
        }
        let rest = &text[index..];
        let Some(character) = rest.chars().next() else {
            break;
        };
        let width = character.len_utf8();

        if character == '\n' {
            let repeated = matches!(
                tokens.last().map(|token| token.kind),
                None | Some(TokenKind::EndOfLine)
            );
            if bracket == 0 && !repeated {
                tokens.push(Token {
                    kind: TokenKind::EndOfLine,
                    text: String::new(),
                    start: index,
                    end: index + width,
                    depth,
                    xml_suspicious: false,
                });
            }
            index += width;
            line_start = true;
            continue;
        }
        if character.is_whitespace() {
            index += width;
            continue;
        }

        // A `#` directive owns its whole physical line. It is not code, but
        // which directive it is decides what the lines around it may claim.
        if character == '#' && line_start {
            let line_end = rest.find('\n').map(|at| index + at).unwrap_or(text.len());
            match classify_directive(&text[index + width..line_end]) {
                Some(Directive::ConditionalOpen) => depth += 1,
                Some(Directive::ConditionalClose) => depth = depth.saturating_sub(1),
                Some(Directive::Neutral) => {}
                None => return Err(Refusal::UnadmittedDirective),
            }
            index = line_end;
            continue;
        }
        line_start = false;

        // `'` and `REM` open comments, and VB has no single-quoted string, so
        // the character that starts a comment is never a delimiter.
        if character == '\'' {
            index = rest.find('\n').map(|at| index + at).unwrap_or(text.len());
            continue;
        }

        // An explicit `_` continuation joins the next physical line.
        if character == '_' && continues_line(&rest[width..]) {
            index = rest
                .find('\n')
                .map(|at| index + at + 1)
                .unwrap_or(text.len());
            continue;
        }

        if character == '$' {
            if rest[width..].starts_with('"') {
                return Err(Refusal::InterpolatedString);
            }
            tokens.push(punct(character, index, width, depth, false));
            index += width;
            continue;
        }

        if character == '"' {
            let end = string_literal_end(rest).ok_or(Refusal::UnterminatedString)?;
            tokens.push(literal(index, end, depth));
            index += end;
            continue;
        }

        // A `#` away from the start of a line is a date literal, consumed
        // opaquely so its `/` and `:` cannot be read as code.
        if character == '#' {
            let line_end = rest.find('\n').unwrap_or(rest.len());
            if let Some(at) = rest[width..line_end].find('#') {
                // Past the opening `#`, the offset of the closing one, and the
                // closing `#` itself.
                let end = width + at + 1;
                tokens.push(literal(index, end, depth));
                index += end;
                continue;
            }
            tokens.push(punct(character, index, width, depth, false));
            index += width;
            continue;
        }

        if character == '[' {
            let line_end = rest.find('\n').unwrap_or(rest.len());
            let at = rest[width..line_end]
                .find(']')
                .ok_or(Refusal::UnterminatedEscapedName)?;
            let end = width + at + 1;
            tokens.push(Token {
                kind: TokenKind::EscapedWord,
                text: rest[width..width + at].to_ascii_lowercase(),
                start: index,
                end: index + end,
                depth,
                xml_suspicious: false,
            });
            index += end;
            continue;
        }

        if character.is_alphanumeric() || character == '_' {
            let end = rest
                .find(|candidate: char| !(candidate.is_alphanumeric() || candidate == '_'))
                .unwrap_or(rest.len());
            let word = rest[..end].to_ascii_lowercase();
            // `REM` is a comment wherever a statement may start.
            if word == "rem" && tokens.last().is_none_or(Token::is_statement_end) {
                index = rest.find('\n').map(|at| index + at).unwrap_or(text.len());
                continue;
            }
            tokens.push(Token {
                kind: TokenKind::Word,
                text: word,
                start: index,
                end: index + end,
                depth,
                xml_suspicious: false,
            });
            index += end;
            continue;
        }

        if character == '(' || character == '{' {
            bracket += 1;
        } else if character == ')' || character == '}' {
            bracket = bracket.saturating_sub(1);
        }
        let suspicious = character == '<' && starts_xml_name(&rest[width..]);
        tokens.push(punct(character, index, width, depth, suspicious));
        index += width;
    }
    if depth > 0 {
        // A `#If` that never closes leaves every later declaration selected by
        // an unevaluated constant. The file is malformed, so it abstains rather
        // than reporting a clean parse of the part that happened to close.
        return Err(Refusal::UnadmittedDirective);
    }
    if !tokens.last().is_none_or(Token::is_statement_end) {
        let at = text.len();
        tokens.push(Token {
            kind: TokenKind::EndOfLine,
            text: String::new(),
            start: at,
            end: at,
            depth,
            xml_suspicious: false,
        });
    }
    Ok(tokens)
}

fn punct(character: char, start: usize, width: usize, depth: usize, xml: bool) -> Token {
    Token {
        kind: TokenKind::Punct(character),
        text: String::new(),
        start,
        end: start + width,
        depth,
        xml_suspicious: xml,
    }
}

fn literal(start: usize, length: usize, depth: usize) -> Token {
    Token {
        kind: TokenKind::Literal,
        text: String::new(),
        start,
        end: start + length,
        depth,
        xml_suspicious: false,
    }
}

/// True when only whitespace separates this point from the end of the line, so
/// a preceding `_` is a continuation rather than part of an identifier.
fn continues_line(rest: &str) -> bool {
    rest.chars()
        .take_while(|character| *character != '\n')
        .all(char::is_whitespace)
}

/// The byte length of a `"…"` literal, honouring `""` doubling. VB strings do
/// not span lines, so a newline ends the search unsuccessfully.
fn string_literal_end(rest: &str) -> Option<usize> {
    let mut index = 1usize;
    while index < rest.len() {
        match rest[index..].chars().next()? {
            '\n' => return None,
            '"' => {
                if rest[index + 1..].starts_with('"') {
                    index += 2;
                    continue;
                }
                return Some(index + 1);
            }
            character => index += character.len_utf8(),
        }
    }
    None
}

/// True when the text after a `<` begins an XML name, which is how an XML
/// literal opens.
///
/// The cost of this rule is that a spaceless comparison such as `a <b` reads as
/// suspicious too. That is understatement, not invention, and it is the
/// intended direction: `<=`, `<>`, and `< ` are unaffected, and VB spells
/// generics `(Of T)` rather than `<T>`, so the operator forms it refuses are
/// rare.
fn starts_xml_name(rest: &str) -> bool {
    rest.chars()
        .next()
        .is_some_and(|character| character.is_alphabetic() || "_/!?%".contains(character))
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// What closes the block being parsed.
///
/// Closers are keyword-typed, and that is what makes an opaque body sound. If
/// the parser ever mistracks a nested block it meets the wrong closer and
/// abstains; it cannot silently attribute an inner `End` to an outer
/// declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Closer {
    /// `End Class`, `End Sub`, and the rest of the `End <keyword>` family.
    End(&'static str),
    /// `Loop` and `Next`, which close without an `End`.
    Word(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct Context {
    /// Members of an `Interface` have no body and no `End`.
    interface: bool,
    nesting: usize,
}

impl Context {
    fn top() -> Self {
        Self {
            interface: false,
            nesting: 0,
        }
    }

    fn inside(self, interface: bool) -> Result<Self, Refusal> {
        if self.nesting >= MAX_NESTING {
            return Err(Refusal::ResourceLimit);
        }
        Ok(Self {
            interface,
            nesting: self.nesting + 1,
        })
    }
}

#[derive(Default)]
struct Members {
    imports: Vec<ImportClause>,
    types: Vec<TypeDecl>,
    methods: Vec<MethodDecl>,
}

/// A parsed block: its members, and where its closing keyword sat.
struct Block {
    members: Members,
    /// Byte offset just past the closing keyword.
    end: usize,
    /// Conditional depth at the closing keyword.
    end_depth: usize,
}

/// Type-like keywords. Each opens a block closed by `End <keyword>`.
const TYPE_KEYWORDS: [&str; 5] = ["class", "module", "structure", "interface", "enum"];

/// Method-like keywords that open a block closed by `End <keyword>`.
const METHOD_KEYWORDS: [&str; 2] = ["sub", "function"];

/// Declaration modifiers.
///
/// `Dim` and `Const` are deliberately absent: they start a field rather than
/// modify a declaration, so they fall through to an opaque statement.
const MODIFIERS: [&str; 23] = [
    "public",
    "private",
    "friend",
    "protected",
    "shared",
    "overridable",
    "overrides",
    "notoverridable",
    "mustoverride",
    "partial",
    "notinheritable",
    "mustinherit",
    "default",
    "readonly",
    "writeonly",
    "shadows",
    "overloads",
    "async",
    "iterator",
    "custom",
    "widening",
    "narrowing",
    "declare",
];

/// Statement keywords that open a block inside a method body.
///
/// This is a closed set, which is what makes an opaque body sound: an
/// identifier that is not one of these cannot open a block, so the parser
/// cannot walk past a method's own `End`.
const BLOCK_STATEMENTS: [(&str, Closer); 8] = [
    ("select", Closer::End("select")),
    ("try", Closer::End("try")),
    ("with", Closer::End("with")),
    ("while", Closer::End("while")),
    ("using", Closer::End("using")),
    ("synclock", Closer::End("synclock")),
    ("do", Closer::Word("loop")),
    ("for", Closer::Word("next")),
];

/// Members the frontend never anchors but whose block structure it must still
/// track to find the enclosing `End`.
const ACCESSOR_CLOSERS: [&str; 6] = [
    "operator",
    "get",
    "set",
    "addhandler",
    "removehandler",
    "raiseevent",
];

struct Parser<'a> {
    tokens: &'a [Token],
    position: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<&'a Token> {
        self.tokens.get(self.position)
    }

    fn peek_at(&self, offset: usize) -> Option<&'a Token> {
        self.tokens.get(self.position + offset)
    }

    fn bump(&mut self) -> Option<&'a Token> {
        let token = self.tokens.get(self.position);
        self.position += 1;
        token
    }

    fn skip_statement_ends(&mut self) {
        while self.peek().is_some_and(Token::is_statement_end) {
            self.position += 1;
        }
    }

    fn at_closer(&self, closer: Closer) -> bool {
        match closer {
            Closer::End(keyword) => {
                self.peek().is_some_and(|token| token.is_word("end"))
                    && self.peek_at(1).is_some_and(|token| token.is_word(keyword))
            }
            Closer::Word(keyword) => self.peek().is_some_and(|token| token.is_word(keyword)),
        }
    }

    /// Consume the closing keyword and the rest of its statement.
    ///
    /// `Loop While x`, `Next i`, and anything else trailing the closer belongs
    /// to the closing statement.
    fn consume_closer(&mut self, closer: Closer) -> Result<usize, Refusal> {
        let end = match closer {
            Closer::End(_) => {
                self.bump();
                self.bump().map(|token| token.end)
            }
            Closer::Word(_) => self.bump().map(|token| token.end),
        }
        .ok_or(Refusal::UnadmittedDeclaration)?;
        while self.peek().is_some_and(|token| !token.is_statement_end()) {
            self.position += 1;
        }
        Ok(end)
    }

    /// Parse members until `closer`, or to end of input when there is none.
    fn parse_block(&mut self, context: Context, closer: Option<Closer>) -> Result<Block, Refusal> {
        let mut members = Members::default();
        loop {
            self.skip_statement_ends();
            let Some(token) = self.peek() else {
                // An unclosed block is a well-formedness violation, not a
                // reason to keep the declarations found so far.
                return match closer {
                    Some(_) => Err(Refusal::UnadmittedDeclaration),
                    None => Ok(Block {
                        members,
                        end: self.tokens.last().map(|token| token.end).unwrap_or(0),
                        end_depth: 0,
                    }),
                };
            };
            if let Some(closer) = closer {
                if self.at_closer(closer) {
                    let end_depth = token.depth;
                    let end = self.consume_closer(closer)?;
                    return Ok(Block {
                        members,
                        end,
                        end_depth,
                    });
                }
            }
            // A closing keyword that does not match the open block means the
            // parser lost the structure. It abstains rather than resyncing.
            if token.is_word("end") && self.peek_at(1).is_some_and(is_block_keyword) {
                return Err(Refusal::UnadmittedDeclaration);
            }
            self.parse_member(context, &mut members)?;
        }
    }

    fn parse_member(&mut self, context: Context, members: &mut Members) -> Result<(), Refusal> {
        let head = self.peek().ok_or(Refusal::UnadmittedDeclaration)?;
        let start = head.start;
        let start_depth = head.depth;
        let attributes = self.parse_attribute_lists()?;
        let modifiers = self.parse_modifiers();

        let Some(token) = self.peek() else {
            return Err(Refusal::UnadmittedDeclaration);
        };
        if token.kind != TokenKind::Word {
            self.skip_statement(context)?;
            return Ok(());
        }
        let keyword = token.text.clone();
        let depth = start_depth.max(token.depth);

        if keyword == "imports" && attributes.is_empty() && modifiers.is_empty() {
            members.imports.push(self.parse_imports(depth));
            return Ok(());
        }
        if keyword == "namespace" {
            self.bump();
            self.skip_declaration_header();
            let block = self.parse_block(context.inside(false)?, Some(Closer::End("namespace")))?;
            members.types.extend(block.members.types);
            members.imports.extend(block.members.imports);
            return Ok(());
        }
        if let Some(name) = static_keyword(&TYPE_KEYWORDS, &keyword) {
            members
                .types
                .push(self.parse_type(context, name, attributes, start, depth)?);
            return Ok(());
        }
        if let Some(name) = static_keyword(&METHOD_KEYWORDS, &keyword) {
            // `MustOverride`, `Declare`, and every interface member declare a
            // signature with no body and no `End`.
            let bodyless = context.interface
                || modifiers
                    .iter()
                    .any(|modifier| modifier == "mustoverride" || modifier == "declare");
            members
                .methods
                .push(self.parse_method(context, name, attributes, start, depth, bodyless)?);
            return Ok(());
        }
        if let Some(closer) = self.member_closer(context, &keyword, &modifiers) {
            self.bump();
            self.skip_declaration_header();
            self.parse_block(context.inside(false)?, Some(closer))?;
            return Ok(());
        }
        if let Some((_, closer)) = BLOCK_STATEMENTS
            .iter()
            .find(|(name, _)| *name == keyword.as_str())
        {
            self.bump();
            self.skip_statement(context)?;
            self.parse_block(context.inside(false)?, Some(*closer))?;
            return Ok(());
        }
        if keyword == "if" {
            // `If … Then` with nothing after `Then` is the block form; anything
            // after it is a single-line `If` that needs no `End If`. That is the
            // language's own rule and is decidable from the token stream.
            let block = self.statement_ends_with_then();
            self.bump();
            self.skip_statement(context)?;
            if block {
                self.parse_block(context.inside(false)?, Some(Closer::End("if")))?;
            }
            return Ok(());
        }
        self.skip_statement(context)?;
        Ok(())
    }

    /// The block closer for a member that is not a type or a method, or `None`
    /// when the member has no body.
    fn member_closer(
        &self,
        context: Context,
        keyword: &str,
        modifiers: &[String],
    ) -> Option<Closer> {
        if context.interface {
            return None;
        }
        match keyword {
            // An auto-property has no `End Property`. A property with accessors
            // is followed by a `Get` or `Set` member, which is the language's
            // own distinction and needs one member of lookahead.
            "property" => self
                .property_has_accessors()
                .then_some(Closer::End("property")),
            // A plain `Event` declaration is bodyless; only `Custom Event` has
            // an `End Event`.
            "event" => modifiers
                .iter()
                .any(|modifier| modifier == "custom")
                .then_some(Closer::End("event")),
            _ => static_keyword(&ACCESSOR_CLOSERS, keyword).map(Closer::End),
        }
    }

    /// One member of lookahead: does a `Get` or `Set` accessor follow this
    /// property declaration?
    fn property_has_accessors(&self) -> bool {
        let mut offset = 0usize;
        while self
            .peek_at(offset)
            .is_some_and(|token| !token.is_statement_end())
        {
            offset += 1;
        }
        loop {
            while self
                .peek_at(offset)
                .is_some_and(|token| token.is_statement_end())
            {
                offset += 1;
            }
            if self.peek_at(offset).map(|token| token.kind) == Some(TokenKind::Punct('<')) {
                let mut angle = 0usize;
                while let Some(token) = self.peek_at(offset) {
                    match token.kind {
                        TokenKind::Punct('<') => angle += 1,
                        TokenKind::Punct('>') => {
                            angle = angle.saturating_sub(1);
                            if angle == 0 {
                                offset += 1;
                                break;
                            }
                        }
                        _ => {}
                    }
                    offset += 1;
                }
                continue;
            }
            let Some(token) = self.peek_at(offset) else {
                return false;
            };
            if token.kind != TokenKind::Word {
                return false;
            }
            if token.text == "get" || token.text == "set" {
                return true;
            }
            if MODIFIERS.contains(&token.text.as_str()) {
                offset += 1;
                continue;
            }
            return false;
        }
    }

    /// True when `Then` is the last token of the statement the parser is on.
    fn statement_ends_with_then(&self) -> bool {
        let mut offset = 0usize;
        let mut last: Option<&Token> = None;
        while let Some(token) = self.peek_at(offset) {
            if token.is_statement_end() {
                break;
            }
            last = Some(token);
            offset += 1;
        }
        last.is_some_and(|token| token.is_word("then"))
    }

    fn parse_type(
        &mut self,
        context: Context,
        keyword: &'static str,
        attributes: Vec<AttributeRef>,
        start: usize,
        start_depth: usize,
    ) -> Result<TypeDecl, Refusal> {
        self.bump();
        self.skip_declaration_header();
        let block = self.parse_block(
            context.inside(keyword == "interface")?,
            Some(Closer::End(keyword)),
        )?;
        Ok(TypeDecl {
            attributes,
            start,
            end: block.end,
            conditional: start_depth > 0 || block.end_depth > 0,
            methods: block.members.methods,
            nested: block.members.types,
        })
    }

    fn parse_method(
        &mut self,
        context: Context,
        keyword: &'static str,
        attributes: Vec<AttributeRef>,
        start: usize,
        start_depth: usize,
        bodyless: bool,
    ) -> Result<MethodDecl, Refusal> {
        self.bump();
        let header_end = self.skip_declaration_header();
        if bodyless {
            return Ok(MethodDecl {
                attributes,
                is_sub: keyword == "sub",
                start,
                end: header_end.max(start + 1),
                conditional: start_depth > 0,
            });
        }
        let block = self.parse_block(context.inside(false)?, Some(Closer::End(keyword)))?;
        Ok(MethodDecl {
            attributes,
            is_sub: keyword == "sub",
            start,
            end: block.end,
            conditional: start_depth > 0 || block.end_depth > 0,
        })
    }

    /// Zero or more attribute lists standing before a declaration.
    ///
    /// The attribute binds to its declaration by grammar, not by line position,
    /// so `<TestMethod()> Public Sub A()` and the two-line form are one
    /// declaration. An XML literal cannot be mistaken for an attribute list,
    /// because `<` is read as one only here — at the head of a declaration —
    /// and is refused wherever a statement may appear.
    fn parse_attribute_lists(&mut self) -> Result<Vec<AttributeRef>, Refusal> {
        let mut attributes = Vec::new();
        loop {
            if self.peek().map(|token| token.kind) != Some(TokenKind::Punct('<')) {
                return Ok(attributes);
            }
            self.bump();
            loop {
                // An optional target specifier, `<Assembly: …>`.
                if self
                    .peek()
                    .is_some_and(|token| token.kind == TokenKind::Word)
                    && self
                        .peek_at(1)
                        .is_some_and(|token| token.kind == TokenKind::Punct(':'))
                {
                    self.bump();
                    self.bump();
                }
                let name = self.parse_qualified_name()?;
                let (qualifier, simple) = match name.rsplit_once('.') {
                    Some((qualifier, simple)) => (Some(qualifier.to_string()), simple.to_string()),
                    None => (None, name),
                };
                attributes.push(AttributeRef {
                    qualifier,
                    name: simple,
                });
                if self.peek().map(|token| token.kind) == Some(TokenKind::Punct('(')) {
                    self.skip_balanced('(', ')')?;
                }
                match self.peek().map(|token| token.kind) {
                    Some(TokenKind::Punct(',')) => {
                        self.bump();
                    }
                    Some(TokenKind::Punct('>')) => {
                        self.bump();
                        break;
                    }
                    // An attribute list split across lines without an explicit
                    // continuation is outside the admitted subset.
                    _ => return Err(Refusal::UnadmittedDeclaration),
                }
            }
            self.skip_statement_ends();
        }
    }

    fn parse_qualified_name(&mut self) -> Result<String, Refusal> {
        let mut name = String::new();
        loop {
            let token = self.peek().ok_or(Refusal::UnadmittedDeclaration)?;
            if !matches!(token.kind, TokenKind::Word | TokenKind::EscapedWord) {
                return Err(Refusal::UnadmittedDeclaration);
            }
            name.push_str(&token.text);
            self.bump();
            if self.peek().map(|token| token.kind) == Some(TokenKind::Punct('.')) {
                name.push('.');
                self.bump();
                continue;
            }
            return Ok(name);
        }
    }

    fn parse_modifiers(&mut self) -> Vec<String> {
        let mut modifiers = Vec::new();
        while let Some(token) = self.peek() {
            if token.kind != TokenKind::Word || !MODIFIERS.contains(&token.text.as_str()) {
                break;
            }
            modifiers.push(token.text.clone());
            self.bump();
        }
        modifiers
    }

    fn parse_imports(&mut self, depth: usize) -> ImportClause {
        self.bump();
        let mut aliased = false;
        let mut name = String::new();
        while let Some(token) = self.peek() {
            if token.is_statement_end() {
                break;
            }
            match token.kind {
                // `Imports Alias = Namespace` binds a different name, so what
                // precedes the `=` is discarded rather than read as the import.
                TokenKind::Punct('=') => {
                    aliased = true;
                    name.clear();
                }
                TokenKind::Word | TokenKind::EscapedWord => name.push_str(&token.text),
                TokenKind::Punct('.') => name.push('.'),
                _ => {}
            }
            self.bump();
        }
        ImportClause {
            name,
            aliased,
            conditional: depth > 0,
        }
    }

    /// Consume the head of a declaration, up to the end of its statement, and
    /// report where it ended.
    ///
    /// The XML guard is deliberately not applied here: a `<` in a signature is
    /// a parameter attribute, which is a declaration position rather than a
    /// statement position.
    fn skip_declaration_header(&mut self) -> usize {
        let mut end = self
            .peek()
            .map(|token| token.start)
            .unwrap_or_else(|| self.tokens.last().map(|token| token.end).unwrap_or(0));
        while let Some(token) = self.peek() {
            if token.is_statement_end() {
                break;
            }
            end = token.end;
            self.position += 1;
        }
        self.bump();
        end
    }

    /// Consume one opaque statement.
    ///
    /// Statements are not parsed: no anchor rests on what a statement means.
    /// What the parser must still get right is where the statement ends and
    /// whether it opens a block, which is why a multi-line lambda is recognized
    /// here and an XML literal is refused here.
    fn skip_statement(&mut self, context: Context) -> Result<(), Refusal> {
        while let Some(token) = self.peek() {
            if token.is_statement_end() {
                self.bump();
                return Ok(());
            }
            if token.xml_suspicious {
                return Err(Refusal::XmlLiteral);
            }
            // A lambda is the one place a `Sub` or `Function` block opens away
            // from a declaration. It always carries a parenthesized parameter
            // list, which is what separates it from `Exit Sub` and `End Sub`.
            if token.kind == TokenKind::Word
                && self
                    .peek_at(1)
                    .is_some_and(|next| next.kind == TokenKind::Punct('('))
            {
                if let Some(keyword) = static_keyword(&METHOD_KEYWORDS, &token.text) {
                    self.bump();
                    self.skip_balanced('(', ')')?;
                    if self.peek().is_some_and(|token| token.is_word("as")) {
                        while self.peek().is_some_and(|token| {
                            !token.is_statement_end()
                                && !matches!(
                                    token.kind,
                                    TokenKind::Punct(',') | TokenKind::Punct(')')
                                )
                        }) {
                            self.bump();
                        }
                    }
                    // A multi-line lambda ends its header statement; a
                    // single-line one carries its body on the same statement.
                    if self.peek().is_some_and(Token::is_statement_end) {
                        self.bump();
                        self.parse_block(context.inside(false)?, Some(Closer::End(keyword)))?;
                    }
                    continue;
                }
            }
            self.bump();
        }
        Ok(())
    }

    fn skip_balanced(&mut self, open: char, close: char) -> Result<(), Refusal> {
        let mut depth = 0usize;
        while let Some(token) = self.peek() {
            match token.kind {
                TokenKind::Punct(character) if character == open => depth += 1,
                TokenKind::Punct(character) if character == close => {
                    depth = depth.saturating_sub(1);
                    self.bump();
                    if depth == 0 {
                        return Ok(());
                    }
                    continue;
                }
                _ => {}
            }
            self.bump();
        }
        Err(Refusal::UnadmittedDeclaration)
    }
}

/// True when this token closes a block, so meeting it under the wrong opener is
/// a structure error rather than an ordinary statement.
fn is_block_keyword(token: &Token) -> bool {
    token.kind == TokenKind::Word
        && (TYPE_KEYWORDS.contains(&token.text.as_str())
            || METHOD_KEYWORDS.contains(&token.text.as_str())
            || ACCESSOR_CLOSERS.contains(&token.text.as_str())
            || matches!(
                token.text.as_str(),
                "namespace"
                    | "property"
                    | "event"
                    | "if"
                    | "select"
                    | "try"
                    | "with"
                    | "while"
                    | "using"
                    | "synclock"
            ))
}

/// Borrow the `'static` spelling of an admitted keyword, so a closer never
/// carries repository text.
fn static_keyword(table: &[&'static str], keyword: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|candidate| **candidate == keyword)
        .copied()
}
