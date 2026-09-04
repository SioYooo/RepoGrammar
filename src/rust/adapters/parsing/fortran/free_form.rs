//! Bounded Fortran free-form lexer for the ADR-0051 declared subset.
//!
//! The subset is free-form Fortran 2008+ text only: `!` comments, single- and
//! double-quoted literals with doubling escapes, `&` continuation (including
//! the leading-`&` form and identifiers split across a continuation), `;`
//! statement separators, and case-insensitive keywords and names. Fixed-form
//! column semantics and preprocessor lines are detected and refused for the
//! whole file before any token is read, because both change what the remaining
//! bytes mean and neither can be evaluated here.
//!
//! Every refusal is typed and terminal: the tokens lexed before a refusal are
//! proven, but the parser abstains for the whole file rather than guessing
//! past the boundary.

/// One token of the declared free-form subset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident,
    Number,
    Str,
    LParen,
    RParen,
    Comma,
    Semicolon,
    Newline,
    Colon,
    DoubleColon,
    Equals,
    /// `=>`, used by `use` rename lists.
    FatArrow,
    /// Any other punctuation. Bodies are skipped structurally, so the exact
    /// spelling carries no decision.
    Punct,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

/// Lexical well-formedness facts a caller must report as a degraded parse.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct LexFlags {
    /// A quoted literal was left open at end of line. Fortran literals cannot
    /// span an uncontinued newline, so the following token boundaries in that
    /// statement are unreliable even though the rest of the file lexes.
    pub unterminated_string: bool,
}

/// Why the file was refused before or during tokenization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LexAbstention {
    /// A line whose first non-blank byte is `#`. Free-form Fortran has no
    /// statement starting with `#`; the byte belongs to a preprocessor, and
    /// which text it selects is not decidable here.
    PreprocessorDirective,
    /// A decidable fixed-form signature: a non-continuation line whose columns
    /// 1-5 are blank and whose column 6 holds a continuation indicator (a
    /// digit or `+ - * /`), or a `*` in column 1. Both are impossible as the
    /// start of a free-form statement.
    FixedFormSignature,
    /// A byte or construct outside the declared subset, such as a `&` that
    /// continues nothing or a non-ASCII byte in code position.
    UnadmittedConstruct,
}

pub(crate) fn lex(text: &str) -> (Vec<Token>, LexFlags, Option<LexAbstention>) {
    if let Some(refusal) = prescan(text) {
        return (Vec::new(), LexFlags::default(), Some(refusal));
    }
    lex_tokens(text)
}

/// True when the byte can begin a Fortran name.
fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// A `&` continuation sequence starts at `index`; return the offset where
/// reading resumes, or `None` when the `&` continues nothing.
///
/// The sequence is `&`, then optional blanks, then optionally a `!` comment to
/// end of line, then the line break, then the next line's leading blanks and
/// its optional leading `&` continuation mark.
fn continuation_end(bytes: &[u8], index: usize) -> Option<usize> {
    if bytes.get(index) != Some(&b'&') {
        return None;
    }
    let mut cursor = index + 1;
    while matches!(bytes.get(cursor), Some(b' ') | Some(b'\t') | Some(b'\r')) {
        cursor += 1;
    }
    if let Some(b'!') = bytes.get(cursor) {
        while cursor < bytes.len() && bytes[cursor] != b'\n' {
            cursor += 1;
        }
    }
    match bytes.get(cursor) {
        None => Some(bytes.len()),
        Some(b'\n') => {
            cursor += 1;
            while matches!(bytes.get(cursor), Some(b' ') | Some(b'\t') | Some(b'\r')) {
                cursor += 1;
            }
            if bytes.get(cursor) == Some(&b'&') {
                cursor += 1;
                while matches!(bytes.get(cursor), Some(b' ') | Some(b'\t') | Some(b'\r')) {
                    cursor += 1;
                }
            }
            Some(cursor)
        }
        _ => None,
    }
}

/// True when `index` sits inside a `&` continuation sequence.
fn at_continuation(bytes: &[u8], index: usize) -> bool {
    continuation_end(bytes, index).is_some()
}

fn lex_tokens(text: &str) -> (Vec<Token>, LexFlags, Option<LexAbstention>) {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut flags = LexFlags::default();
    let mut index = 0usize;
    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];
        let kind = match byte {
            b' ' | b'\t' | b'\r' => {
                index += 1;
                continue;
            }
            b'\n' => {
                index += 1;
                TokenKind::Newline
            }
            b'!' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            b'\'' | b'"' => {
                let (end, terminated) = string_end(bytes, index);
                if !terminated {
                    flags.unterminated_string = true;
                }
                index = end;
                TokenKind::Str
            }
            b'&' => match continuation_end(bytes, index) {
                Some(resume) => {
                    // A continuation after a completed token joins statements,
                    // not tokens; no token is emitted and no newline separates
                    // them.
                    index = resume;
                    continue;
                }
                None => {
                    return (tokens, flags, Some(LexAbstention::UnadmittedConstruct));
                }
            },
            byte if byte.is_ascii_digit() => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        byte if is_ident_continue(byte) || byte == b'.' => index += 1,
                        b'&' if at_continuation(bytes, index) => {
                            index = continuation_end(bytes, index).unwrap_or(index);
                        }
                        _ => break,
                    }
                }
                TokenKind::Number
            }
            byte if is_ident_start(byte) => {
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        byte if is_ident_continue(byte) => index += 1,
                        b'&' if at_continuation(bytes, index) => {
                            index = continuation_end(bytes, index).unwrap_or(index);
                        }
                        _ => break,
                    }
                }
                TokenKind::Ident
            }
            _ => {
                if bytes[index..].starts_with(b"::") {
                    index += 2;
                    TokenKind::DoubleColon
                } else if bytes[index..].starts_with(b"=>") {
                    index += 2;
                    TokenKind::FatArrow
                } else {
                    index += 1;
                    match byte {
                        b'(' => TokenKind::LParen,
                        b')' => TokenKind::RParen,
                        b',' => TokenKind::Comma,
                        b';' => TokenKind::Semicolon,
                        b':' => TokenKind::Colon,
                        b'=' => TokenKind::Equals,
                        _ => TokenKind::Punct,
                    }
                }
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: index,
        });
    }
    (tokens, flags, None)
}

/// End of a quoted literal starting at `start`, with `&` continuation and
/// doubling escapes. Returns `(end, terminated)`; an uncontinued newline or
/// end of input ends the literal unterminated at that boundary.
fn string_end(bytes: &[u8], start: usize) -> (usize, bool) {
    let quote = bytes[start];
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => return (index, false),
            b'&' if at_continuation(bytes, index) => {
                index = continuation_end(bytes, index).unwrap_or(index);
            }
            byte if byte == quote => {
                if bytes.get(index + 1) == Some(&quote) {
                    index += 2;
                } else {
                    return (index + 1, true);
                }
            }
            _ => index += 1,
        }
    }
    (bytes.len(), false)
}

/// A fixed-form continuation-indicator byte in column 6.
fn is_column_six_indicator(byte: u8) -> bool {
    matches!(byte, b'0'..=b'9' | b'+' | b'-' | b'*' | b'/')
}

/// Whole-file source-form and preprocessor prescan, run before tokenizing.
///
/// The scan is string- and continuation-aware: a trailing `&` (in code or in a
/// literal) suppresses the column-6 check on the following line, because a
/// free-form continuation may legally resume with its own `&` or an indented
/// token at exactly that column.
fn prescan(text: &str) -> Option<LexAbstention> {
    let mut pending_continuation = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start_matches([' ', '\t']).starts_with('#') {
            return Some(LexAbstention::PreprocessorDirective);
        }
        if !pending_continuation {
            let bytes = line.as_bytes();
            if bytes.first() == Some(&b'*') {
                // A `*` in column 1 starts a fixed-form comment; no free-form
                // statement begins with `*`.
                return Some(LexAbstention::FixedFormSignature);
            }
            if bytes.len() >= 6
                && bytes[..5]
                    .iter()
                    .all(|byte| *byte == b' ' || *byte == b'\t')
                && is_column_six_indicator(bytes[5])
            {
                return Some(LexAbstention::FixedFormSignature);
            }
        }
        pending_continuation = line_ends_with_continuation(line);
    }
    None
}

/// True when the line's last significant byte is `&`, ignoring blanks, a
/// trailing `!` comment, and quoted text (with doubled quotes).
fn line_ends_with_continuation(line: &str) -> bool {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut quote: Option<u8> = None;
    let mut last_significant: Option<u8> = None;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(open) => {
                if byte == open {
                    if bytes.get(index + 1) == Some(&open) {
                        index += 1;
                    } else {
                        quote = None;
                    }
                }
            }
            None => match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'!' => break,
                b' ' | b'\t' | b'\r' | b'\n' => {}
                _ => last_significant = Some(byte),
            },
        }
        index += 1;
    }
    last_significant == Some(b'&')
}

/// A token's significant text, with any `&`-continuation glue bytes removed.
///
/// A free-form token may be split across a continuation, so its byte range
/// can span the `&`, the line break, and the next line's leading `&`. The
/// spelling compared against keywords and names is the glued text.
pub(crate) fn token_text<'b>(text: &'b str, token: Token) -> std::borrow::Cow<'b, str> {
    let raw = &text[token.start..token.end];
    if raw
        .bytes()
        .any(|byte| matches!(byte, b'&' | b' ' | b'\t' | b'\r' | b'\n'))
    {
        std::borrow::Cow::Owned(
            raw.chars()
                .filter(|character| !matches!(character, '&' | ' ' | '\t' | '\r' | '\n'))
                .collect(),
        )
    } else {
        std::borrow::Cow::Borrowed(raw)
    }
}

/// Case-insensitive keyword/name match for a token's glued text.
pub(crate) fn ident_is(text: &str, token: Token, expected: &str) -> bool {
    token.kind == TokenKind::Ident && token_text(text, token).eq_ignore_ascii_case(expected)
}

/// Lowercased copy of a name token's glued text, for map keys.
pub(crate) fn lower(text: &str, token: Token) -> String {
    token_text(text, token).to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<TokenKind> {
        lex(text).0.into_iter().map(|token| token.kind).collect()
    }

    #[test]
    fn keywords_and_names_are_plain_identifiers() {
        assert_eq!(
            kinds("module test_suite\nend module test_suite\n"),
            vec![
                TokenKind::Ident, // module
                TokenKind::Ident, // test_suite
                TokenKind::Newline,
                TokenKind::Ident, // end
                TokenKind::Ident, // module
                TokenKind::Ident, // test_suite
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn comments_and_string_literals_never_yield_tokens() {
        assert_eq!(
            kinds("! subroutine test_hidden(error)\n"),
            vec![TokenKind::Newline]
        );
        assert_eq!(
            kinds("s = 'it''s ! not a comment'\nt = \"also ! not\"\n"),
            vec![
                TokenKind::Ident,
                TokenKind::Equals,
                TokenKind::Str,
                TokenKind::Newline,
                TokenKind::Ident,
                TokenKind::Equals,
                TokenKind::Str,
                TokenKind::Newline,
            ]
        );
        let (tokens, flags, abstention) = lex("s = 'unterminated\nx = 1\n");
        assert!(flags.unterminated_string);
        assert_eq!(abstention, None);
        assert!(tokens.iter().any(|token| token.kind == TokenKind::Newline));
    }

    #[test]
    fn continuation_lines_join_statements_and_split_identifiers() {
        assert_eq!(
            kinds("call run_a& ! comment mid continuation\n &nd_fix\n"),
            vec![
                TokenKind::Ident, // call
                TokenKind::Ident, // run_and_fix, glued across the split
                TokenKind::Newline,
            ]
        );
        let (tokens, _, abstention) = lex("subroutine test_va&\n  &lid(error)\nend subroutine\n");
        assert_eq!(abstention, None);
        let names = tokens
            .iter()
            .filter(|token| token.kind == TokenKind::Ident)
            .map(|token| {
                lower(
                    "subroutine test_va&\n  &lid(error)\nend subroutine\n",
                    *token,
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["subroutine", "test_valid", "error", "end", "subroutine"]
        );
    }

    #[test]
    fn semicolons_split_statements_and_newlines_end_them() {
        assert_eq!(
            kinds("a = 1; b = 2\nc = 3\n"),
            vec![
                TokenKind::Ident,
                TokenKind::Equals,
                TokenKind::Number,
                TokenKind::Semicolon,
                TokenKind::Ident,
                TokenKind::Equals,
                TokenKind::Number,
                TokenKind::Newline,
                TokenKind::Ident,
                TokenKind::Equals,
                TokenKind::Number,
                TokenKind::Newline,
            ]
        );
    }

    #[test]
    fn preprocessor_lines_are_refused_whole_file() {
        for text in [
            "#ifdef DEBUG\nsubroutine test_x(error)\nend subroutine\n",
            "module m\n  #include \"common.f90\"\nend module\n",
        ] {
            assert_eq!(lex(text).2, Some(LexAbstention::PreprocessorDirective));
        }
    }

    #[test]
    fn fixed_form_signatures_are_refused_whole_file() {
        assert_eq!(
            lex("      subroutine test_fixed(a)\n     1real a\n      end\n").2,
            Some(LexAbstention::FixedFormSignature)
        );
        assert_eq!(
            lex("* fixed-form comment\nsubroutine test_x(error)\nend subroutine\n").2,
            Some(LexAbstention::FixedFormSignature)
        );
        // Column-6 continuation after a free-form `&` is legal free form.
        assert_eq!(lex("x = 1 &\n     & + 2\n").2, None);
    }

    #[test]
    fn an_ampersand_that_continues_nothing_abstains() {
        assert_eq!(
            lex("x = 1 & 2\n").2,
            Some(LexAbstention::UnadmittedConstruct)
        );
        assert_eq!(lex("x = 1 &\n").2, None);
    }
}
