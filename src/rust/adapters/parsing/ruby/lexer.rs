//! Bounded Ruby lexer for the ADR-0049 declared subset.
//!
//! This is a real lexer, not a byte scan, because Ruby's test files carry
//! constructs whose *end* a scan cannot find: heredoc bodies begin on the line
//! after their marker, `%w[]` lists nest their delimiters, interpolations can
//! reopen strings and span lines, and a regex literal may contain the word
//! `end`. Each of those is tokenized exactly here.
//!
//! The two places Ruby itself needs runtime state to lex — the slash
//! (division or regex) and `<<` (shift or heredoc) after an ambiguous token —
//! are decided by the total rule of ADR-0049 D4 over facts this lexer already
//! has: the previous token's class, whether whitespace preceded it, and
//! whether an identifier is the first token of its statement. Where that rule
//! cannot decide, the file abstains whole-file with a typed refusal rather
//! than guessing; a wrong guess moves `def`/`class`/`end` boundaries, and a
//! wrong guess is worse than abstention.
//!
//! Statements are otherwise opaque: this lexer answers where literals,
//! comments, and heredocs end and where lines and keywords begin. It builds no
//! expression tree, and nothing here executes Ruby.

/// One token of the declared subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Newline,
    Semicolon,
    Comma,
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    /// A lowercase identifier. Keywords are [`TokenKind::Keyword`]; after a
    /// `.` or `def`, a keyword-shaped word is still emitted as `Ident`
    /// because Ruby reads it as a method name there.
    Ident,
    Const,
    Keyword,
    /// `@ivar`, `@@cvar`, `$gvar`.
    Var,
    /// A hash label `name:`.
    Label,
    Number,
    /// An old-style character literal `?x`.
    CharLit,
    Symbol,
    /// Any string literal: single- or double-quoted, `%q(...)`, `%Q{...}`,
    /// `%(...)`, and backtick command strings.
    Str,
    /// A heredoc *start marker*; the body is consumed by the lexer and never
    /// produces tokens.
    Heredoc,
    Regexp,
    /// Every operator, separator, and lone bracket; the token text carries
    /// which.
    Op,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// Whether this token is the first token of its statement. This is the
    /// command-position fact MRI's own lexer uses to disambiguate slash and
    /// heredoc after an identifier.
    pub(crate) stmt_start: bool,
}

/// Why the lex stopped. Every kind is a whole-file typed refusal: the tokens
/// before the boundary exist, but a boundary this lexer cannot find exactly
/// unproves every later `def`/`class`/`end` extent, so the file abstains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexAbstention {
    /// A byte or construct outside the declared subset of ADR-0049 D4.
    UnadmittedConstruct,
    UnterminatedString,
    UnterminatedRegexp,
    UnterminatedPercentLiteral,
    UnterminatedHeredoc,
    /// `/` after an identifier that is not in statement-first position:
    /// division versus regex depends on MRI's runtime local/command
    /// distinction, and either reading moves structural boundaries.
    SlashDisambiguation,
    /// `<<ID` after an identifier not in statement-first position with
    /// whitespace before the `<<`: shift versus heredoc, same hazard.
    HeredocDisambiguation,
}

impl LexAbstention {
    pub(crate) fn unknown_kind(self) -> &'static str {
        match self {
            Self::UnadmittedConstruct => "unadmitted_ruby_construct",
            Self::UnterminatedString => "unterminated_string",
            Self::UnterminatedRegexp => "unterminated_regexp",
            Self::UnterminatedPercentLiteral => "unterminated_percent_literal",
            Self::UnterminatedHeredoc => "unterminated_heredoc",
            Self::SlashDisambiguation => "ruby_slash_disambiguation",
            Self::HeredocDisambiguation => "ruby_heredoc_disambiguation",
        }
    }

    pub(crate) fn claim(self) -> &'static str {
        match self {
            Self::SlashDisambiguation | Self::HeredocDisambiguation => "ruby_lexical_invariance",
            _ => "ruby_parse",
        }
    }

    pub(crate) fn reason(self) -> crate::core::model::UnknownReasonCode {
        use crate::core::model::UnknownReasonCode;
        match self {
            Self::SlashDisambiguation | Self::HeredocDisambiguation => {
                UnknownReasonCode::ConflictingFacts
            }
            _ => UnknownReasonCode::InsufficientSupport,
        }
    }

    pub(crate) fn note(self) -> &'static str {
        match self {
            Self::UnadmittedConstruct => {
                "Ruby source left the declared subset, so no later construct in this file was read"
            }
            Self::UnterminatedString => {
                "a Ruby string literal is left open, so the token boundaries after it are unproven"
            }
            Self::UnterminatedRegexp => {
                "a Ruby regexp literal is left open, so the token boundaries after it are unproven"
            }
            Self::UnterminatedPercentLiteral => {
                "a Ruby %-literal is left open, so the token boundaries after it are unproven"
            }
            Self::UnterminatedHeredoc => {
                "a Ruby heredoc body never reaches its terminator, so the token boundaries after it are unproven"
            }
            // These notes are stored as semantic-fact text, which refuses any
            // whitespace-delimited token that looks like an absolute path. A
            // bare "/" is exactly that, so the operators are spelled out.
            Self::SlashDisambiguation => {
                "a slash after an identifier could be division or a regexp, and the two readings disagree about the token stream"
            }
            Self::HeredocDisambiguation => {
                "a double left angle bracket before an identifier could be left shift or a heredoc, and the two readings disagree about the token stream"
            }
        }
    }
}

/// How an ambiguous `/` or `%` after the previous token reads. The contexts
/// are the same, so one enum serves both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiteralContext {
    /// Beginning of an expression: a literal may start here.
    Beg,
    /// After a complete value: an operator, never a literal.
    Value,
    /// Undecidable from source: refuse the file.
    Refuse,
}

/// One heredoc whose body has not been consumed yet.
#[derive(Clone)]
struct PendingHeredoc {
    terminator: Vec<u8>,
    /// `<<-` and `<<~` allow an indented terminator; plain `<<` does not.
    allow_indent: bool,
    /// `<<'ID'` bodies are literal: no interpolation, no continuation.
    literal: bool,
}

/// A frame of an interpolating-literal scan: either a nested string with its
/// close delimiter, or an interpolation body at a brace depth.
enum Frame {
    Str(u8),
    Code(u32),
}

pub(crate) struct Lexed {
    pub(crate) tokens: Vec<Token>,
    pub(crate) abstention: Option<LexAbstention>,
}

struct Lexer<'a> {
    bytes: &'a [u8],
    tokens: Vec<Token>,
    pending_heredocs: Vec<PendingHeredoc>,
    abstention: Option<LexAbstention>,
    at_line_start: bool,
    /// Set by `def`, cleared by the next word: a method name may carry `?`,
    /// `!`, and `=` suffixes that are operators elsewhere.
    after_def: bool,
    ws_before: bool,
    stmt_start: bool,
    prev: Option<Token>,
}

/// Tokenize as much of `text` as lies inside the declared subset.
pub(crate) fn lex(text: &str) -> Lexed {
    let mut lexer = Lexer {
        bytes: text.as_bytes(),
        tokens: Vec::new(),
        pending_heredocs: Vec::new(),
        abstention: None,
        at_line_start: true,
        after_def: false,
        ws_before: false,
        stmt_start: true,
        prev: None,
    };
    lexer.run();
    Lexed {
        tokens: lexer.tokens,
        abstention: lexer.abstention,
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

fn is_heredoc_starter(byte: u8) -> bool {
    is_ident_start(byte) || matches!(byte, b'\'' | b'"' | b'`')
}

impl<'a> Lexer<'a> {
    fn str_at(&self, start: usize, end: usize) -> &'a str {
        // Every token extent is ASCII-delimited, so the slice is on bounds.
        std::str::from_utf8(&self.bytes[start..end]).unwrap_or_default()
    }

    fn prev_kind(&self) -> Option<TokenKind> {
        self.prev.map(|token| token.kind)
    }

    fn prev_text(&self) -> Option<&'a str> {
        self.prev.map(|token| self.str_at(token.start, token.end))
    }

    fn prev_is_dot(&self) -> bool {
        self.prev_text()
            .is_some_and(|text| text == "." || text == "&.")
    }

    fn prev_is_value_keyword(&self) -> bool {
        self.prev_text()
            .is_some_and(|text| VALUE_KEYWORDS.contains(&text))
    }

    /// The declared total context rule for `/`, `%`, and directly-attached
    /// heredocs (ADR-0049 D4): beginning-of-expression positions admit a
    /// literal, value-ending positions do not, and the one corner MRI decides
    /// from the runtime local/command distinction refuses.
    fn literal_context(&self) -> LiteralContext {
        match self.prev_kind() {
            None
            | Some(TokenKind::Newline)
            | Some(TokenKind::Semicolon)
            | Some(TokenKind::Comma)
            | Some(TokenKind::LParen)
            | Some(TokenKind::LBracket)
            | Some(TokenKind::LBrace)
            | Some(TokenKind::Op) => LiteralContext::Beg,
            Some(TokenKind::Keyword) => {
                if self.prev_is_value_keyword() {
                    LiteralContext::Value
                } else {
                    LiteralContext::Beg
                }
            }
            Some(TokenKind::Ident) => {
                if self.prev.is_some_and(|token| token.stmt_start) {
                    // Command position: `puts /re/` and `puts <<~EOS` are
                    // literal in MRI too.
                    LiteralContext::Beg
                } else {
                    LiteralContext::Refuse
                }
            }
            Some(TokenKind::Label) => LiteralContext::Beg,
            Some(
                TokenKind::RParen
                | TokenKind::RBracket
                | TokenKind::RBrace
                | TokenKind::Const
                | TokenKind::Var
                | TokenKind::Number
                | TokenKind::CharLit
                | TokenKind::Symbol
                | TokenKind::Str
                | TokenKind::Heredoc
                | TokenKind::Regexp,
            ) => LiteralContext::Value,
        }
    }

    fn emit(&mut self, kind: TokenKind, start: usize, end: usize) {
        let token = Token {
            kind,
            start,
            end,
            stmt_start: self.stmt_start,
        };
        match kind {
            TokenKind::Newline | TokenKind::Semicolon => self.stmt_start = true,
            _ => self.stmt_start = false,
        }
        match kind {
            TokenKind::Ident | TokenKind::Const => self.after_def = false,
            TokenKind::Keyword if self.str_at(start, end) == "self" => {}
            TokenKind::Op if self.str_at(start, end) == "." => {}
            _ => self.after_def = false,
        }
        if kind != TokenKind::Newline {
            self.at_line_start = false;
        }
        self.ws_before = false;
        self.prev = Some(token);
        self.tokens.push(token);
    }

    fn refuse(&mut self, abstention: LexAbstention) {
        if self.abstention.is_none() {
            self.abstention = Some(abstention);
        }
    }

    fn run(&mut self) {
        let mut index = 0usize;
        while index < self.bytes.len() {
            if self.abstention.is_some() {
                return;
            }
            let start = index;
            let byte = self.bytes[index];
            match byte {
                b' ' | b'\t' | 0x0b | 0x0c => {
                    self.ws_before = true;
                    index += 1;
                    continue;
                }
                b'\r' if self.bytes.get(index + 1) == Some(&b'\n') => {
                    self.ws_before = true;
                    index += 1;
                    continue;
                }
                b'\r' => {
                    self.refuse(LexAbstention::UnadmittedConstruct);
                    return;
                }
                b'\n' => {
                    index += 1;
                    // Heredoc bodies start on the line after their marker.
                    self.consume_heredoc_bodies(&mut index);
                    self.at_line_start = true;
                    // A newline after a prefix token — an operator, comma, or
                    // opening bracket — continues the statement, as in MRI;
                    // it is not a statement end and emits no token.
                    match self.prev_kind() {
                        Some(
                            TokenKind::Op
                            | TokenKind::Comma
                            | TokenKind::LParen
                            | TokenKind::LBracket
                            | TokenKind::LBrace,
                        ) => {}
                        _ => self.emit(TokenKind::Newline, start, index),
                    }
                    continue;
                }
                b'#' => {
                    index = skip_to_line_end(self.bytes, index);
                    self.ws_before = true;
                    continue;
                }
                b'=' if self.at_line_start
                    && self.bytes[index..].starts_with(b"=begin")
                    && matches!(
                        self.bytes.get(index + 6),
                        None | Some(b'\n') | Some(b'\r') | Some(b' ') | Some(b'\t')
                    ) =>
                {
                    match skip_block_comment(self.bytes, index) {
                        Some(end) => {
                            index = end;
                            self.ws_before = true;
                        }
                        None => {
                            self.refuse(LexAbstention::UnadmittedConstruct);
                            return;
                        }
                    }
                    continue;
                }
                _ => {}
            }
            if self.at_line_start && self.bytes[index..].starts_with(b"__END__") {
                match self.bytes.get(index + 7) {
                    None | Some(b'\n') => return,
                    Some(b'\r') if self.bytes.get(index + 8) == Some(&b'\n') => return,
                    _ => {}
                }
            }

            match byte {
                b'(' => {
                    index += 1;
                    self.emit(TokenKind::LParen, start, index);
                }
                b')' => {
                    index += 1;
                    self.emit(TokenKind::RParen, start, index);
                }
                b'[' => {
                    index += 1;
                    self.emit(TokenKind::LBracket, start, index);
                }
                b']' => {
                    index += 1;
                    self.emit(TokenKind::RBracket, start, index);
                }
                b'{' => {
                    index += 1;
                    self.emit(TokenKind::LBrace, start, index);
                }
                b'}' => {
                    index += 1;
                    self.emit(TokenKind::RBrace, start, index);
                }
                b',' => {
                    index += 1;
                    self.emit(TokenKind::Comma, start, index);
                }
                b';' => {
                    index += 1;
                    self.emit(TokenKind::Semicolon, start, index);
                }
                b'\\' if self.bytes.get(index + 1) == Some(&b'\n') => {
                    // Line continuation: the newline is not a statement end.
                    index += 2;
                    continue;
                }
                b'\\' => {
                    self.refuse(LexAbstention::UnadmittedConstruct);
                    return;
                }
                b'"' | b'\'' => {
                    match scan_delimited_literal(self.bytes, index + 1, byte, byte, byte == b'\'') {
                        Ok(end) => {
                            index = end;
                            self.emit(TokenKind::Str, start, index);
                        }
                        Err(kind) => self.refuse(kind),
                    }
                }
                b'`' => {
                    if self.prev_is_dot() {
                        // A method literally named `` ` ``; refusing is cheaper
                        // than guessing at the call shape.
                        self.refuse(LexAbstention::UnadmittedConstruct);
                        return;
                    }
                    match scan_delimited_literal(self.bytes, index + 1, b'`', b'`', false) {
                        Ok(end) => {
                            index = end;
                            self.emit(TokenKind::Str, start, index);
                        }
                        Err(kind) => self.refuse(kind),
                    }
                }
                b'/' => match self.literal_context() {
                    LiteralContext::Value => {
                        index += 1;
                        self.emit(TokenKind::Op, start, index);
                    }
                    LiteralContext::Beg => {
                        match scan_delimited_literal(self.bytes, index + 1, b'/', b'/', false) {
                            Ok(end) => {
                                index = end;
                                // Regex flag letters attach directly and are
                                // greedily part of the literal in MRI too, so
                                // a space-separated keyword can never be eaten.
                                while index < self.bytes.len()
                                    && self.bytes[index].is_ascii_alphabetic()
                                {
                                    index += 1;
                                }
                                self.emit(TokenKind::Regexp, start, index);
                            }
                            Err(LexAbstention::UnterminatedString) => {
                                self.refuse(LexAbstention::UnterminatedRegexp);
                            }
                            Err(kind) => self.refuse(kind),
                        }
                    }
                    LiteralContext::Refuse => {
                        self.refuse(LexAbstention::SlashDisambiguation);
                    }
                },
                b'%' => {
                    let literal = match self.literal_context() {
                        LiteralContext::Beg => self.percent_literal_end(index),
                        // In the refused corner a trailing `%` with no operand
                        // on the line is outside the subset; otherwise `%` is
                        // modulo, as MRI reads it after a value.
                        LiteralContext::Refuse => match self.bytes.get(index + 1) {
                            None | Some(b'\n') | Some(b'\r') => {
                                self.refuse(LexAbstention::UnadmittedConstruct);
                                return;
                            }
                            _ => None,
                        },
                        LiteralContext::Value => None,
                    };
                    match literal {
                        Some(Ok(end)) => {
                            index = end;
                            self.emit(TokenKind::Str, start, index);
                        }
                        Some(Err(LexAbstention::UnterminatedString)) => {
                            self.refuse(LexAbstention::UnterminatedPercentLiteral);
                        }
                        Some(Err(kind)) => self.refuse(kind),
                        None => {
                            index = match scan_operator(self.bytes, index) {
                                Some(end) => end,
                                None => {
                                    self.refuse(LexAbstention::UnadmittedConstruct);
                                    return;
                                }
                            };
                            self.emit(TokenKind::Op, start, index);
                        }
                    }
                }
                b'<' => {
                    if self.bytes[index..].starts_with(b"<<=") {
                        index += 3;
                        self.emit(TokenKind::Op, start, index);
                    } else if let Some((end, pending)) = self.heredoc_start(index) {
                        index = end;
                        self.pending_heredocs.push(pending);
                        self.emit(TokenKind::Heredoc, start, index);
                    } else {
                        index = match scan_operator(self.bytes, index) {
                            Some(end) => end,
                            None => {
                                self.refuse(LexAbstention::UnadmittedConstruct);
                                return;
                            }
                        };
                        self.emit(TokenKind::Op, start, index);
                    }
                }
                b'?' => match self.bytes.get(index + 1).copied() {
                    None | Some(b' ') | Some(b'\t') | Some(b'\n') | Some(b'\r') => {
                        index += 1;
                        self.emit(TokenKind::Op, start, index);
                    }
                    Some(_) => {
                        index += 2;
                        if self.bytes[start + 1] == b'\\' {
                            index += 1;
                        }
                        self.emit(TokenKind::CharLit, start, index);
                    }
                },
                b':' => {
                    index = self.symbol_or_colon(start);
                }
                b'@' | b'$' => {
                    index = match self.scan_sigil(start, byte) {
                        Some(end) => {
                            self.emit(TokenKind::Var, start, end);
                            end
                        }
                        None => {
                            self.refuse(LexAbstention::UnadmittedConstruct);
                            return;
                        }
                    };
                }
                byte if byte.is_ascii_digit() => {
                    index = number_end(self.bytes, index);
                    self.emit(TokenKind::Number, start, index);
                }
                byte if is_ident_start(byte) => {
                    index = self.word_end(index);
                }
                _ => {
                    index = match scan_operator(self.bytes, index) {
                        Some(end) => end,
                        None => {
                            // A non-ASCII byte or stray punctuation in code
                            // position begins no token of the subset.
                            self.refuse(LexAbstention::UnadmittedConstruct);
                            return;
                        }
                    };
                    self.emit(TokenKind::Op, start, index);
                }
            }
        }
    }

    fn symbol_or_colon(&mut self, start: usize) -> usize {
        let next = self.bytes.get(start + 1).copied();
        if next == Some(b':') {
            let end = start + 2;
            self.emit(TokenKind::Op, start, end);
            return end;
        }
        let mut index = start + 1;
        if matches!(next, Some(byte) if is_ident_start(byte)) {
            while index < self.bytes.len() && is_ident_continue(self.bytes[index]) {
                index += 1;
            }
            if matches!(self.bytes.get(index), Some(b'?') | Some(b'!')) {
                index += 1;
            }
            self.emit(TokenKind::Symbol, start, index);
            return index;
        }
        match next {
            Some(b'"') => match scan_delimited_literal(self.bytes, start + 2, b'"', b'"', false) {
                Ok(end) => {
                    self.emit(TokenKind::Symbol, start, end);
                    return end;
                }
                Err(LexAbstention::UnterminatedString) => {
                    self.refuse(LexAbstention::UnterminatedString);
                    return self.bytes.len();
                }
                Err(kind) => {
                    self.refuse(kind);
                    return self.bytes.len();
                }
            },
            Some(b'\'') => {
                match scan_delimited_literal(self.bytes, start + 2, b'\'', b'\'', true) {
                    Ok(end) => {
                        self.emit(TokenKind::Symbol, start, end);
                        return end;
                    }
                    Err(LexAbstention::UnterminatedString) => {
                        self.refuse(LexAbstention::UnterminatedString);
                        return self.bytes.len();
                    }
                    Err(kind) => {
                        self.refuse(kind);
                        return self.bytes.len();
                    }
                }
            }
            Some(b'@') | Some(b'$') => {
                let end = match self.scan_sigil(start + 1, next.expect("checked above")) {
                    Some(end) => end,
                    None => {
                        self.refuse(LexAbstention::UnadmittedConstruct);
                        return self.bytes.len();
                    }
                };
                self.emit(TokenKind::Symbol, start, end);
                return end;
            }
            Some(
                b'+' | b'-' | b'*' | b'/' | b'<' | b'>' | b'=' | b'!' | b'~' | b'^' | b'&' | b'|'
                | b'%' | b'[',
            ) => {
                while index < self.bytes.len()
                    && matches!(
                        self.bytes[index],
                        b'+' | b'-'
                            | b'*'
                            | b'/'
                            | b'<'
                            | b'>'
                            | b'='
                            | b'!'
                            | b'~'
                            | b'^'
                            | b'&'
                            | b'|'
                            | b'%'
                            | b'['
                            | b']'
                    )
                {
                    index += 1;
                }
                self.emit(TokenKind::Symbol, start, index);
                return index;
            }
            _ => {}
        }
        let end = start + 1;
        self.emit(TokenKind::Op, start, end);
        end
    }

    /// End of a sigil name (`@x`, `@@x`, `$x`), or `None` when no name
    /// follows the sigil.
    fn scan_sigil(&self, start: usize, sigil: u8) -> Option<usize> {
        let mut index = start + 1;
        if sigil == b'@' && self.bytes.get(index) == Some(&b'@') {
            index += 1;
        }
        let name_start = index;
        while index < self.bytes.len() && self.bytes[index].is_ascii_alphanumeric() {
            index += 1;
        }
        (index > name_start).then_some(index)
    }

    /// End of an identifier, constant, keyword, or hash label.
    ///
    /// A word may end in one `?` or `!`: those are method-name suffixes in
    /// every position (`empty?`, `sort!`), which is why `include?(x)` is one
    /// callee and not a character literal. After `def` an `=` may be part of
    /// the name too (`def foo=(value)`).
    fn word_end(&mut self, start: usize) -> usize {
        let mut index = start + 1;
        while index < self.bytes.len() && is_ident_continue(self.bytes[index]) {
            index += 1;
        }
        if self.after_def {
            while matches!(self.bytes.get(index), Some(b'?') | Some(b'!') | Some(b'=')) {
                index += 1;
            }
        } else if matches!(self.bytes.get(index), Some(b'?') | Some(b'!')) {
            index += 1;
        }
        if self.bytes.get(index) == Some(&b':')
            && self.bytes.get(index + 1) != Some(&b':')
            && self.bytes.get(index + 1).is_some_and(|byte| {
                matches!(
                    byte,
                    b' ' | b'\t' | b'\r' | b'\n' | b',' | b'}' | b')' | b']' | b';'
                )
            })
        {
            // A hash label: `name:` directly followed by a value boundary.
            index += 1;
            self.emit(TokenKind::Label, start, index);
            return index;
        }
        let text = self.str_at(start, index);
        let kind = if is_keyword(text) && !self.prev_is_dot() {
            TokenKind::Keyword
        } else if self.bytes[start].is_ascii_uppercase() {
            TokenKind::Const
        } else {
            TokenKind::Ident
        };
        self.emit(kind, start, index);
        if kind == TokenKind::Keyword && text == "def" {
            // `emit` cleared it; a method name follows and may carry the
            // operator suffixes that are operators elsewhere.
            self.after_def = true;
        }
        index
    }

    /// The extent of a `%`-literal starting at `start`, or `None` when the
    /// `%` is the modulo operator. `Some(Err(..))` is a typed refusal.
    fn percent_literal_end(&self, start: usize) -> Option<Result<usize, LexAbstention>> {
        let designator = self.bytes.get(start + 1).copied()?;
        let bare = matches!(designator, b'(' | b'{' | b'[' | b'<');
        let single = matches!(designator, b'q' | b'w' | b'i' | b's');
        let double = matches!(designator, b'Q' | b'W' | b'I' | b'r' | b'x');
        if !bare && !single && !double {
            return None;
        }
        let open = if bare {
            designator
        } else {
            self.bytes.get(start + 2).copied()?
        };
        if open.is_ascii_alphanumeric()
            || open.is_ascii_whitespace()
            || open == b'='
            || open == 0
            || open >= 0x80
        {
            return None;
        }
        let (open_delim, close_delim) = match open {
            b'(' => (b'(', b')'),
            b'[' => (b'[', b']'),
            b'{' => (b'{', b'}'),
            b'<' => (b'<', b'>'),
            other => (other, other),
        };
        let content_start = start + 1 + usize::from(!bare) + 1;
        match scan_delimited_literal(self.bytes, content_start, open_delim, close_delim, single) {
            Ok(end) => Some(Ok(end)),
            Err(LexAbstention::UnterminatedString) => {
                Some(Err(LexAbstention::UnterminatedPercentLiteral))
            }
            Err(kind) => Some(Err(kind)),
        }
    }

    /// A heredoc start at `start` (which points at the first `<`), when the
    /// total rule admits one here. `None` means the `<<` is an operator.
    fn heredoc_start(&mut self, start: usize) -> Option<(usize, PendingHeredoc)> {
        if self.bytes.get(start + 1) != Some(&b'<') {
            // A single `<` is never a heredoc marker.
            return None;
        }
        let mut index = start + 2;
        let allow_indent = matches!(self.bytes.get(index), Some(b'~') | Some(b'-'));
        if allow_indent {
            index += 1;
        }
        let starter = self.bytes.get(index).copied()?;
        if !is_heredoc_starter(starter) {
            return None;
        }
        // The heredoc rule differs from the slash rule in one place: after a
        // value-ending token with no whitespace, `<<` is unambiguously the
        // shift operator, and after `.` it is a method name.
        if self.prev_is_dot() {
            return None;
        }
        let reading = match self.literal_context() {
            LiteralContext::Beg => HeredocReading::Heredoc,
            LiteralContext::Value if !self.ws_before => HeredocReading::Shift,
            LiteralContext::Value => HeredocReading::Refuse,
            LiteralContext::Refuse if self.ws_before => HeredocReading::Refuse,
            LiteralContext::Refuse => HeredocReading::Shift,
        };
        match reading {
            HeredocReading::Heredoc => {}
            HeredocReading::Shift => return None,
            HeredocReading::Refuse => {
                self.refuse(LexAbstention::HeredocDisambiguation);
                return None;
            }
        }
        let terminator: Vec<u8>;
        let literal;
        if matches!(starter, b'\'' | b'"' | b'`') {
            let close = self.bytes[index + 1..]
                .iter()
                .position(|byte| *byte == starter)?;
            terminator = self.bytes[index + 1..index + 1 + close].to_vec();
            literal = starter == b'\'';
            index += close + 2;
        } else {
            let name_start = index;
            while index < self.bytes.len() && is_ident_continue(self.bytes[index]) {
                index += 1;
            }
            terminator = self.bytes[name_start..index].to_vec();
            literal = false;
        }
        if terminator.is_empty() {
            return None;
        }
        Some((
            index,
            PendingHeredoc {
                terminator,
                allow_indent,
                literal,
            },
        ))
    }

    /// Consume the bodies of every pending heredoc, in declaration order.
    fn consume_heredoc_bodies(&mut self, index: &mut usize) {
        while let Some(pending) = self.pending_heredocs.first().cloned() {
            match scan_heredoc_body(self.bytes, *index, &pending) {
                Ok(end) => {
                    *index = end;
                    self.pending_heredocs.remove(0);
                }
                Err(kind) => {
                    self.refuse(kind);
                    return;
                }
            }
        }
    }
}

enum HeredocReading {
    Heredoc,
    Shift,
    Refuse,
}

/// Keywords that end a value: after them, `/` divides and a spaced `<<`
/// behaves like any other value-ending token.
const VALUE_KEYWORDS: &[&str] = &[
    "end", "self", "nil", "true", "false", "redo", "retry", "__FILE__", "__LINE__", "__dir__",
];

fn is_keyword(word: &str) -> bool {
    matches!(
        word,
        "def"
            | "end"
            | "class"
            | "module"
            | "if"
            | "unless"
            | "while"
            | "until"
            | "for"
            | "in"
            | "do"
            | "then"
            | "else"
            | "elsif"
            | "case"
            | "when"
            | "begin"
            | "rescue"
            | "ensure"
            | "return"
            | "break"
            | "next"
            | "redo"
            | "retry"
            | "yield"
            | "super"
            | "and"
            | "or"
            | "not"
            | "alias"
            | "undef"
            | "defined?"
            | "self"
            | "nil"
            | "true"
            | "false"
            | "__FILE__"
            | "__LINE__"
            | "__dir__"
            | "BEGIN"
            | "END"
    )
}

fn skip_to_line_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

/// An `=begin`/`=end` block comment starting at `start`. Returns the index of
/// the newline that ends the `=end` line, or `None` when the file ends first.
fn skip_block_comment(bytes: &[u8], start: usize) -> Option<usize> {
    let mut index = skip_to_line_end(bytes, start);
    while index < bytes.len() {
        index += 1; // the newline
        if bytes[index..].starts_with(b"=end") {
            return Some(skip_to_line_end(bytes, index + 4));
        }
        index = skip_to_line_end(bytes, index);
    }
    None
}

/// Scan an interpolating (or single-quoted) literal body from `start` until
/// its close delimiter, tracking nesting for paired delimiters and `#{}`
/// interpolation through nested strings, braces, and comments.
///
/// Constructs the declared subset refuses *inside an interpolation* — a bare
/// `/`, a `%`-literal, a heredoc, or a backslash in code position — return
/// [`LexAbstention::UnadmittedConstruct`].
fn scan_delimited_literal(
    bytes: &[u8],
    start: usize,
    open: u8,
    close: u8,
    single: bool,
) -> Result<usize, LexAbstention> {
    let mut index = start;
    let mut frames: Vec<Frame> = Vec::new();
    let mut depth = 0u32;
    loop {
        let byte = match bytes.get(index) {
            Some(byte) => *byte,
            None => return Err(LexAbstention::UnterminatedString),
        };
        match frames.last() {
            None => match byte {
                b'\\' => {
                    if single {
                        if matches!(bytes.get(index + 1), Some(next) if *next == b'\\' || *next == close)
                        {
                            index += 2;
                        } else {
                            // In single-quoted semantics a backslash before
                            // anything else is a literal backslash.
                            index += 1;
                        }
                    } else {
                        index += 2;
                    }
                }
                current if current == close => {
                    if depth == 0 {
                        return Ok(index + 1);
                    }
                    depth -= 1;
                    index += 1;
                }
                current if open != close && current == open => {
                    depth += 1;
                    index += 1;
                }
                b'#' if !single && bytes.get(index + 1) == Some(&b'{') => {
                    frames.push(Frame::Code(1));
                    index += 2;
                }
                0 => return Err(LexAbstention::UnadmittedConstruct),
                _ => index += 1,
            },
            Some(Frame::Str(close_inner)) => {
                let close_inner = *close_inner;
                match byte {
                    b'\\' => {
                        if matches!(
                            bytes.get(index + 1),
                            Some(b'\\') | Some(b'\'') | Some(b'"') | Some(b'`')
                        ) {
                            index += 2;
                        } else {
                            index += 1;
                        }
                    }
                    current if current == close_inner => {
                        frames.pop();
                        index += 1;
                    }
                    b'#' if close_inner != b'\'' && bytes.get(index + 1) == Some(&b'{') => {
                        frames.push(Frame::Code(1));
                        index += 2;
                    }
                    _ => index += 1,
                }
            }
            Some(Frame::Code(brace)) => {
                let mut brace = *brace;
                match byte {
                    b'{' => {
                        brace += 1;
                        *frames.last_mut().expect("frame exists") = Frame::Code(brace);
                        index += 1;
                    }
                    b'}' => {
                        brace -= 1;
                        if brace == 0 {
                            frames.pop();
                        } else {
                            *frames.last_mut().expect("frame exists") = Frame::Code(brace);
                        }
                        index += 1;
                    }
                    b'"' | b'\'' | b'`' => {
                        frames.push(Frame::Str(byte));
                        index += 1;
                    }
                    b'#' => {
                        // A comment inside interpolation runs to end of line
                        // and the interpolation continues on the next one.
                        index = skip_to_line_end(bytes, index);
                    }
                    b'/' | b'%' | b'\\' => return Err(LexAbstention::UnadmittedConstruct),
                    b'<' if bytes.get(index + 1) == Some(&b'<') => {
                        return Err(LexAbstention::UnadmittedConstruct);
                    }
                    _ => index += 1,
                }
            }
        }
    }
}

/// Scan a heredoc body from `start` (the byte after the newline that ended the
/// marker line) until the exact terminator line. Interpolating bodies track
/// interpolation so a terminator-shaped line inside `#{...}` cannot end the
/// body, and honor the backslash line-continuation rule; `<<'ID'` bodies are
/// literal line scans.
fn scan_heredoc_body(
    bytes: &[u8],
    start: usize,
    pending: &PendingHeredoc,
) -> Result<usize, LexAbstention> {
    fn line_is_terminator(line: &[u8], terminator: &[u8], allow_indent: bool) -> bool {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if allow_indent {
            let mut start = 0;
            while start < line.len() && matches!(line[start], b' ' | b'\t') {
                start += 1;
            }
            &line[start..] == terminator
        } else {
            line == terminator
        }
    }

    let mut index = start;
    if pending.literal {
        let mut line_start = index;
        while index < bytes.len() {
            if bytes[index] == b'\n'
                && line_is_terminator(
                    &bytes[line_start..index],
                    &pending.terminator,
                    pending.allow_indent,
                )
            {
                return Ok(index + 1);
            }
            if bytes[index] == b'\n' {
                line_start = index + 1;
            }
            index += 1;
        }
        return Err(LexAbstention::UnterminatedHeredoc);
    }

    let mut frames: Vec<Frame> = Vec::new();
    let mut line_start = index;
    let mut continued = false;
    loop {
        let byte = match bytes.get(index) {
            Some(byte) => *byte,
            None => return Err(LexAbstention::UnterminatedHeredoc),
        };
        match frames.last() {
            None => match byte {
                b'\\' if bytes.get(index + 1) == Some(&b'\n') => {
                    // A continued line cannot be the terminator.
                    index += 2;
                    line_start = index;
                    continued = true;
                }
                b'\\' => index += 2,
                b'\n' => {
                    if !continued
                        && line_is_terminator(
                            &bytes[line_start..index],
                            &pending.terminator,
                            pending.allow_indent,
                        )
                    {
                        return Ok(index + 1);
                    }
                    continued = false;
                    index += 1;
                    line_start = index;
                }
                b'#' if bytes.get(index + 1) == Some(&b'{') => {
                    frames.push(Frame::Code(1));
                    index += 2;
                }
                _ => index += 1,
            },
            Some(Frame::Str(close)) => {
                let close = *close;
                match byte {
                    b'\\' => index += 2,
                    current if current == close => {
                        frames.pop();
                        index += 1;
                    }
                    b'#' if close != b'\'' && bytes.get(index + 1) == Some(&b'{') => {
                        frames.push(Frame::Code(1));
                        index += 2;
                    }
                    _ => index += 1,
                }
            }
            Some(Frame::Code(brace)) => {
                let mut brace = *brace;
                match byte {
                    b'{' => {
                        brace += 1;
                        *frames.last_mut().expect("frame exists") = Frame::Code(brace);
                        index += 1;
                    }
                    b'}' => {
                        brace -= 1;
                        if brace == 0 {
                            frames.pop();
                        } else {
                            *frames.last_mut().expect("frame exists") = Frame::Code(brace);
                        }
                        index += 1;
                    }
                    b'"' | b'\'' | b'`' => {
                        frames.push(Frame::Str(byte));
                        index += 1;
                    }
                    b'#' => index = skip_to_line_end(bytes, index),
                    b'/' | b'%' | b'\\' | b'<' => {
                        return Err(LexAbstention::UnadmittedConstruct);
                    }
                    _ => index += 1,
                }
            }
        }
    }
}

fn number_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    if bytes[index] == b'0'
        && matches!(
            bytes.get(index + 1),
            Some(b'x')
                | Some(b'X')
                | Some(b'o')
                | Some(b'O')
                | Some(b'b')
                | Some(b'B')
                | Some(b'd')
                | Some(b'D')
        )
    {
        index += 2;
        while index < bytes.len() && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
        {
            index += 1;
        }
        return index;
    }
    while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
        index += 1;
    }
    if bytes.get(index) == Some(&b'.') && bytes.get(index + 1).is_some_and(u8::is_ascii_digit) {
        index += 1;
        while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
            index += 1;
        }
    }
    if matches!(bytes.get(index), Some(b'e') | Some(b'E')) {
        let mut lookahead = index + 1;
        if matches!(bytes.get(lookahead), Some(b'+') | Some(b'-')) {
            lookahead += 1;
        }
        if bytes.get(lookahead).is_some_and(u8::is_ascii_digit) {
            index = lookahead;
            while index < bytes.len() && (bytes[index].is_ascii_digit() || bytes[index] == b'_') {
                index += 1;
            }
        }
    }
    if matches!(bytes.get(index), Some(b'r') | Some(b'i')) {
        index += 1;
    }
    index
}

/// Longest-match operator table. `/`, `%`, `?`, `:`, and `<<`-versus-heredoc
/// are decided by the caller's total rules, not here.
fn scan_operator(bytes: &[u8], start: usize) -> Option<usize> {
    let rest = &bytes[start..];
    for (spelling, width) in [
        (&b"**="[..], 3usize),
        (b"<<=", 3),
        (b">>=", 3),
        (b"<=>", 3),
        (b"===", 3),
        (b"...", 3),
        (b"&&=", 3),
        (b"||=", 3),
        (b"**", 2),
        (b"==", 2),
        (b"!=", 2),
        (b"<=", 2),
        (b">=", 2),
        (b"&&", 2),
        (b"||", 2),
        (b"<<", 2),
        (b">>", 2),
        (b"+=", 2),
        (b"-=", 2),
        (b"*=", 2),
        (b"/=", 2),
        (b"%=", 2),
        (b"&=", 2),
        (b"|=", 2),
        (b"^=", 2),
        (b"=~", 2),
        (b"!~", 2),
        (b"=>", 2),
        (b"->", 2),
        (b"..", 2),
        (b"::", 2),
        (b"&.", 2),
        (b"%", 1),
        (b"+", 1),
        (b"-", 1),
        (b"*", 1),
        (b"&", 1),
        (b"|", 1),
        (b"^", 1),
        (b"~", 1),
        (b"!", 1),
        (b"<", 1),
        (b">", 1),
        (b"=", 1),
        (b".", 1),
    ] {
        if rest.starts_with(spelling) {
            return Some(start + width);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(text: &str) -> Vec<String> {
        lex(text)
            .tokens
            .iter()
            .map(|token| text[token.start..token.end].to_string())
            .collect()
    }

    fn abstention(text: &str) -> Option<LexAbstention> {
        lex(text).abstention
    }

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(text).tokens.iter().map(|token| token.kind).collect()
    }

    #[test]
    fn heredoc_bodies_hide_structure_and_terminators_are_exact() {
        // A `class` inside a heredoc body must never become a token.
        let text = "x = <<~EOS\nclass Fake < Minitest::Test\nEOS\nclass Real\nend\n";
        let lexed = lex(text);
        assert_eq!(lexed.abstention, None);
        assert_eq!(
            lexed
                .tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Keyword
                    && &text[token.start..token.end] == "class")
                .count(),
            1
        );
        assert_eq!(
            kinds("x = <<~A\nA\n"),
            vec![
                TokenKind::Ident,
                TokenKind::Op,
                TokenKind::Heredoc,
                TokenKind::Newline,
            ]
        );

        // Plain `<<` requires the terminator at column zero; `<<-` and `<<~`
        // accept an indented one.
        assert_eq!(abstention("x = <<A\n  A\nA\n"), None);
        assert_eq!(
            abstention("x = <<A\n  A\n"),
            Some(LexAbstention::UnterminatedHeredoc)
        );
        assert_eq!(abstention("x = <<-A\n  A\n"), None);
        assert_eq!(abstention("x = <<~A\n    A\n"), None);

        // Quoted heredocs are literal: no interpolation, no continuation.
        assert_eq!(abstention("x = <<'A'\n#{\nA\n"), None);
        assert_eq!(
            abstention("x = <<'A'\nA"),
            Some(LexAbstention::UnterminatedHeredoc)
        );
        // Backtick heredoc bodies and double-quoted terminators parse too.
        assert_eq!(abstention("x = <<`SH`\nout\nSH\n"), None);
        assert_eq!(abstention("x = <<\"A\"\nbody\nA\n"), None);
    }

    #[test]
    fn interpolating_heredoc_bodies_track_interpolation_and_continuation() {
        // A terminator-shaped line inside an interpolation must not end the
        // body.
        let text = "x = <<~A\nval #{ [1].map { |v| v }\nA\n}\nrest\nA\n";
        assert_eq!(abstention(text), None);
        // A backslash continuation prevents the next line from terminating.
        let text = "x = <<A\nbody \\\nA\nA\n";
        assert_eq!(abstention(text), None);
        // Without the second terminator line the body never closes.
        let text = "x = <<A\nbody \\\nA\n";
        assert_eq!(abstention(text), Some(LexAbstention::UnterminatedHeredoc));
    }

    #[test]
    fn multiple_heredocs_consume_bodies_in_declaration_order() {
        let text = "pair = [<<~FIRST, <<~SECOND]\nfirst body\nFIRST\nsecond body\nSECOND\n";
        let lexed = lex(text);
        assert_eq!(lexed.abstention, None);
        assert_eq!(
            lexed
                .tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Heredoc)
                .count(),
            2
        );
        assert!(texts(text).iter().all(|word| word != "first"));
    }

    #[test]
    fn heredoc_after_command_name_is_a_heredoc_after_value_is_shift() {
        let lexed = lex("puts <<~EOS\nbody\nEOS\n");
        assert_eq!(lexed.abstention, None);
        assert!(lexed
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::Heredoc));

        let lexed = lex("arr << value\n");
        assert_eq!(lexed.abstention, None);
        assert!(!lexed
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::Heredoc));
        assert!(lexed.tokens.iter().any(|token| token.kind == TokenKind::Op));

        // The ambiguous corner refuses the whole file; no whitespace is
        // shift, as in MRI's end state.
        assert_eq!(
            abstention("x = arr <<value\n"),
            Some(LexAbstention::HeredocDisambiguation)
        );
        assert_eq!(abstention("x = arr<<value\n"), None);
        // After `.` the `<<` is a method name, never a heredoc.
        assert_eq!(abstention("x = a.<<(2)\n"), None);
    }

    #[test]
    fn slash_follows_the_declared_total_rule() {
        for (text, is_regexp) in [
            ("z = (a + b) / 2\n", false),
            ("n = 4 / 2\n", false),
            ("v = @count / 2\n", false),
            ("y = arr[0] / 2\n", false),
            ("x = Foo / 2\n", false),
            ("match = /end/\n", true),
            ("assert_match(/\\d+/, s)\n", true),
            ("x = s =~ /end/\n", true),
            ("puts /re/\n", true),
            ("if /a/ then b end\n", true),
            ("h = { a: /re/ }\n", true),
        ] {
            let lexed = lex(text);
            assert_eq!(lexed.abstention, None, "{text}");
            let has_regexp = lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Regexp);
            let has_slash_op = lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Op && &text[token.start..token.end] == "/");
            assert_eq!(has_regexp, is_regexp, "{text}");
            assert_ne!(has_slash_op, is_regexp, "{text}");
        }
        // The refused corner: after an identifier that is not the first token
        // of its statement, MRI's reading depends on the runtime distinction
        // between a local variable and a command call, so the file abstains.
        for text in ["x = a / b\n", "x = a / 2 if b\n", "ratio = expected / 2\n"] {
            assert_eq!(
                abstention(text),
                Some(LexAbstention::SlashDisambiguation),
                "{text}"
            );
        }
        // A regexp containing `end` must swallow it, and attached flags are
        // part of the literal.
        let text = "x = /end/ if y\n";
        let lexed = lex(text);
        assert!(lexed
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::Regexp));
        assert!(lexed.tokens.iter().all(
            |token| token.kind != TokenKind::Keyword || &text[token.start..token.end] != "end"
        ));
        let lexed = lex("x = /a/imx\n");
        assert!(lexed
            .tokens
            .iter()
            .any(|token| token.kind == TokenKind::Regexp && token.end == 10));
    }

    #[test]
    fn percent_literals_nest_delimiters_and_track_interpolation() {
        assert_eq!(abstention("words = %w[a\\ b [c] d]\n"), None);
        assert!(texts("words = %w[a\\ b [c] d]\n").contains(&"%w[a\\ b [c] d]".to_string()));
        assert_eq!(abstention("x = %q(a (b) c)\n"), None);
        assert_eq!(abstention("x = %Q{a {b} #{c}}\n"), None);
        assert_eq!(abstention("x = %(bare (nest))\n"), None);
        assert_eq!(
            abstention("x = %w[never closed\n"),
            Some(LexAbstention::UnterminatedPercentLiteral)
        );
        // `%` alone stays modulo in every non-literal context.
        for text in ["x = a % b\n", "x = a %w[b]\n", "x = 7 % 3\n"] {
            let lexed = lex(text);
            assert_eq!(lexed.abstention, None, "{text}");
            assert!(lexed
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::Op && &text[token.start..token.end] == "%"));
        }
    }

    #[test]
    fn strings_interpolate_through_nested_strings_and_comments() {
        let text = "x = \"a #{ b(\"}\") } c\"\ny = \"#{ d # }\n}\"\n";
        let lexed = lex(text);
        assert_eq!(lexed.abstention, None);
        assert_eq!(
            lexed
                .tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Str)
                .count(),
            2
        );

        // Escapes: only `\\` and the quote escape in single-quoted strings.
        assert_eq!(abstention("x = 'a\\'b'\n"), None);
        assert_eq!(abstention("x = 'a\\b'\n"), None);
        assert_eq!(abstention("x = \"a\\\"b\"\n"), None);
        assert_eq!(
            abstention("x = 'unterminated\n"),
            Some(LexAbstention::UnterminatedString)
        );

        // Backtick command strings interpolate.
        assert_eq!(abstention("out = `ls #{dir}`\n"), None);
    }

    #[test]
    fn symbols_labels_and_char_literals_lex_as_single_tokens() {
        let text = "h = { name: :up, opt: :'quoted', op: :+, q: :end, i: :@v, g: :$x }\nc = ?a\n";
        let lexed = lex(text);
        assert_eq!(lexed.abstention, None);
        let words = texts(text);
        for expected in [
            "name:",
            ":up",
            ":'quoted'",
            ":+",
            ":end",
            ":@v",
            ":$x",
            "?a",
        ] {
            assert!(
                words.contains(&expected.to_string()),
                "{expected} in {words:?}"
            );
        }
        // A `:end` symbol must not become the `end` keyword.
        assert!(lexed.tokens.iter().all(
            |token| token.kind != TokenKind::Keyword || &text[token.start..token.end] != "end"
        ));
    }

    #[test]
    fn block_comments_and_data_section_are_inert() {
        let text = "=begin\nclass Fake < Minitest::Test\n=end\nclass Real\nend\n";
        let lexed = lex(text);
        assert_eq!(lexed.abstention, None);
        assert_eq!(
            lexed
                .tokens
                .iter()
                .filter(|token| token.kind == TokenKind::Keyword
                    && &text[token.start..token.end] == "class")
                .count(),
            1
        );
        // An unterminated block comment refuses the file.
        assert_eq!(
            abstention("=begin\nclass Fake\n"),
            Some(LexAbstention::UnadmittedConstruct)
        );

        let lexed = lex("x = 1\n__END__\nclass Fake\n");
        assert_eq!(lexed.abstention, None);
        assert!(texts("x = 1\n__END__\nclass Fake\n")
            .iter()
            .all(|word| word != "Fake"));
    }

    #[test]
    fn line_continuations_and_statement_starts() {
        let lexed = lex("x = 1 +\n  2\ny = 3\n");
        let newlines = lexed
            .tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Newline)
            .count();
        assert_eq!(
            newlines, 2,
            "the continuation newline is not a statement end"
        );

        // The first token of each statement is marked: command position.
        let lexed = lex("puts x\nputs y\n");
        let commands: Vec<Token> = lexed
            .tokens
            .iter()
            .copied()
            .filter(|token| token.kind == TokenKind::Ident && token.stmt_start)
            .collect();
        assert_eq!(commands.len(), 2);
        let text = "x = puts y\n";
        let lexed = lex(text);
        let y = lexed
            .tokens
            .iter()
            .copied()
            .find(|token| token.kind == TokenKind::Ident && &text[token.start..token.end] == "y")
            .expect("y");
        assert!(!y.stmt_start);
    }

    #[test]
    fn keywords_after_a_dot_are_method_names() {
        let text = "x = obj.end\n";
        let lexed = lex(text);
        assert_eq!(lexed.abstention, None);
        assert!(lexed
            .tokens
            .iter()
            .all(|token| token.kind != TokenKind::Keyword));
        assert!(texts(text).contains(&"end".to_string()));
    }

    #[test]
    fn unadmitted_bytes_refuse_the_file() {
        for text in [
            "x = 1\ny = \\z\n",
            "x = $ \n",
            "x = @ \n",
            "x = 1 \r 2\n",
            "x = y %\n",
        ] {
            assert_eq!(
                abstention(text),
                Some(LexAbstention::UnadmittedConstruct),
                "{text}"
            );
        }
    }
}
