//! PHP lexical analyser for the ADR-0047 declared subset.
//!
//! This is the first half of a frontend, not a scanner: it produces a token
//! stream for the bytes after an admitted `<?php` prologue, or it refuses the
//! file. There is no third outcome and no recovery. Everything the parser in
//! [`super::syntax`] decides rests on token boundaries established here, and
//! every anchor downstream is a question about that token stream rather than
//! about byte positions.
//!
//! Three properties are load-bearing.
//!
//! **Strings, comments, heredocs, and attributes can never contribute to an
//! anchor.** Not by a separate awareness rule, but because each is consumed as
//! one opaque token: `public function testFoo` inside a `/* */` comment or a
//! heredoc body is bytes inside a literal, not tokens. The repository has
//! shipped the opposite defect four times in other lanes, each time from
//! asking whether text appears rather than whether a construct exists.
//!
//! **An attribute must close on its opening line.** PHP 8 reads `#[` as the
//! start of an attribute; PHP 7.4 reads it as a comment to end of line. Those
//! two readings agree about every token boundary after that line only when the
//! attribute closes before it, so a `#[` that runs past end of line is the one
//! place in the declared subset where the dialect readings diverge
//! structurally, and it is refused rather than guessed (ADR-0047 D4a).
//!
//! **Code positions are ASCII.** The declared subset admits ASCII identifiers
//! only; a non-ASCII byte in code position is outside the subset, while
//! non-ASCII inside literals and comments is never classified and so is
//! harmless.

/// Why the lexer refused a file. Every variant maps to one registered typed
/// `UNKNOWN` kind; no source text is ever carried in a refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// A quoted run, `/* */` comment, or heredoc left open.
    UnterminatedLiteral,
    /// A `#[` attribute ran past its opening line, where the PHP 7.4 and 8.x
    /// readings stop agreeing about token boundaries.
    AttributeSpanningLines,
    /// A byte that begins no token of the declared subset, including any
    /// non-ASCII byte in code position and the `?>` close tag.
    UnsupportedByte,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Identifier,
    /// `$name`.
    Variable,
    Number,
    /// A single- or double-quoted string, consumed opaquely.
    String,
    /// A heredoc or nowdoc, consumed opaquely.
    Heredoc,
    /// `// …` or `# …` to end of line; `doc` marks `/** … */`.
    Comment {
        doc: bool,
    },
    /// A `#[ … ]` attribute block that closes on its opening line.
    Attribute,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Colon,
    DoubleColon,
    Arrow,
    Backslash,
    /// Every operator and punctuation byte the grammar treats opaquely,
    /// including `?`, `&`, `...`, and `->`-class operators in bodies.
    Operator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Token {
    pub kind: TokenKind,
    pub start: usize,
    pub end: usize,
}

impl Token {
    pub(crate) fn text<'a>(&self, source: &'a str) -> &'a str {
        &source[self.start..self.end]
    }
}

/// Longest-match operator table. Ordering matters: a longer spelling must
/// precede every prefix of itself. The `?>` close tag and `#[` attribute are
/// deliberately absent because they are decided before this table is reached.
const OPERATORS: &[&str] = &[
    "<<=", ">>=", "===", "!==", "<=>", "**=", "...", "<<", ">>", "==", "!=", "<=", ">=", "&&",
    "||", "??=", "??", "++", "--", "->", "=>", "+=", "-=", "*=", "/=", ".=", "%=", "&=", "|=",
    "^=", "?->", "+", "-", "*", "/", "%", "^", "~", "!", "<", ">", "=", ".", "&", "|", "@", "`",
];

pub(crate) fn lex(source: &str) -> Result<Vec<Token>, Refusal> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0usize;
    while index < bytes.len() {
        let start = index;
        let byte = bytes[index];
        let kind = match byte {
            b' ' | b'\t' | b'\r' | b'\n' | 0x0b | 0x0c => {
                index += 1;
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                continue;
            }
            b'/' if bytes.get(index + 1) == Some(&b'*') => {
                let doc = bytes.get(index + 2) == Some(&b'*');
                let mut cursor = index + 2;
                loop {
                    match bytes.get(cursor) {
                        None => return Err(Refusal::UnterminatedLiteral),
                        Some(b'*') if bytes.get(cursor + 1) == Some(&b'/') => {
                            cursor += 2;
                            break;
                        }
                        Some(_) => cursor += 1,
                    }
                }
                index = cursor;
                TokenKind::Comment { doc }
            }
            b'#' => {
                if bytes.get(index + 1) == Some(&b'[') {
                    let end = attribute_end(bytes, index)?;
                    index = end;
                    TokenKind::Attribute
                } else {
                    while index < bytes.len() && bytes[index] != b'\n' {
                        index += 1;
                    }
                    continue;
                }
            }
            b'<' if bytes.get(index + 1) == Some(&b'<') && bytes.get(index + 2) == Some(&b'<') => {
                let end = heredoc_end(bytes, index)?;
                index = end;
                TokenKind::Heredoc
            }
            b'"' | b'\'' => {
                let mut cursor = index + 1;
                loop {
                    match bytes.get(cursor) {
                        None => return Err(Refusal::UnterminatedLiteral),
                        Some(b'\\') => cursor += 2,
                        Some(quote) if *quote == byte => {
                            cursor += 1;
                            break;
                        }
                        // A quoted string ends at EOF; a newline is literal
                        // content in both PHP quote styles, so it is consumed
                        // like any other byte.
                        Some(_) => cursor += 1,
                    }
                }
                if cursor > bytes.len() {
                    return Err(Refusal::UnterminatedLiteral);
                }
                index = cursor;
                TokenKind::String
            }
            b'$' if bytes
                .get(index + 1)
                .is_some_and(|next| next.is_ascii_alphabetic() || *next == b'_') =>
            {
                index += 2;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                TokenKind::Variable
            }
            byte if byte.is_ascii_digit() => {
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || bytes[index] == b'.'
                        || bytes[index] == b'_')
                {
                    index += 1;
                }
                TokenKind::Number
            }
            byte if byte.is_ascii_alphabetic() || byte == b'_' => {
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                TokenKind::Identifier
            }
            b'?' if bytes.get(index + 1) == Some(&b'>') => {
                // The close tag would restart inline-HTML mode, which is
                // outside the declared subset.
                return Err(Refusal::UnsupportedByte);
            }
            b'(' => {
                index += 1;
                TokenKind::LParen
            }
            b')' => {
                index += 1;
                TokenKind::RParen
            }
            b'{' => {
                index += 1;
                TokenKind::LBrace
            }
            b'}' => {
                index += 1;
                TokenKind::RBrace
            }
            b'[' => {
                index += 1;
                TokenKind::LBracket
            }
            b']' => {
                index += 1;
                TokenKind::RBracket
            }
            b';' => {
                index += 1;
                TokenKind::Semicolon
            }
            b',' => {
                index += 1;
                TokenKind::Comma
            }
            b':' if bytes.get(index + 1) == Some(&b':') => {
                index += 2;
                TokenKind::DoubleColon
            }
            b':' => {
                index += 1;
                TokenKind::Colon
            }
            b'-' if bytes.get(index + 1) == Some(&b'>') => {
                index += 2;
                TokenKind::Arrow
            }
            b'\\' => {
                index += 1;
                TokenKind::Backslash
            }
            byte if !byte.is_ascii() => {
                return Err(Refusal::UnsupportedByte);
            }
            _ => {
                let width = OPERATORS
                    .iter()
                    .find(|spelling| source[start..].starts_with(*spelling))
                    .map_or(1, |spelling| spelling.len());
                index += width;
                TokenKind::Operator
            }
        };
        tokens.push(Token {
            kind,
            start,
            end: index,
        });
    }
    Ok(tokens)
}

/// End of a `#[ … ]` attribute that closes on its opening line.
///
/// The scan tracks parentheses and square brackets and skips quoted runs, so a
/// `]` inside an argument array or a string cannot close the attribute early.
/// A newline before the closing `]` is the PHP 7.4/8.x divergence and refuses
/// the file.
fn attribute_end(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 2;
    let mut paren_depth = 0usize;
    let mut bracket_depth = 1usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\n' => return Err(Refusal::AttributeSpanningLines),
            b'"' | b'\'' => {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    match bytes[index] {
                        b'\\' => index += 2,
                        byte if byte == quote => {
                            index += 1;
                            break;
                        }
                        b'\n' => return Err(Refusal::AttributeSpanningLines),
                        _ => index += 1,
                    }
                }
            }
            b'(' => {
                paren_depth += 1;
                index += 1;
            }
            b')' => {
                paren_depth = paren_depth.saturating_sub(1);
                index += 1;
            }
            b'[' => {
                bracket_depth += 1;
                index += 1;
            }
            b']' => {
                bracket_depth -= 1;
                index += 1;
                if bracket_depth == 0 && paren_depth == 0 {
                    return Ok(index);
                }
            }
            _ => index += 1,
        }
    }
    Err(Refusal::AttributeSpanningLines)
}

/// End of a heredoc or nowdoc starting at `start`.
///
/// `<<<` is followed by optional whitespace, an identifier (nowdoc quotes it),
/// optional whitespace, and a newline; the body runs to a closing line whose
/// leading whitespace is followed by the identifier and a non-identifier byte.
/// The flexible-indentation closing rule is PHP 7.3+, uniform across the
/// declared 7.4-8.5 invariance set.
fn heredoc_end(bytes: &[u8], start: usize) -> Result<usize, Refusal> {
    let mut index = start + 3;
    while bytes.get(index).is_some_and(u8::is_ascii_whitespace) {
        if bytes.get(index) == Some(&b'\n') {
            return Err(Refusal::UnterminatedLiteral);
        }
        index += 1;
    }
    let quote = match bytes.get(index) {
        Some(quote @ (b'"' | b'\'')) => {
            index += 1;
            Some(*quote)
        }
        _ => None,
    };
    let name_start = index;
    if !bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
    {
        return Err(Refusal::UnterminatedLiteral);
    }
    while bytes
        .get(index)
        .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
    {
        index += 1;
    }
    let name = &bytes[name_start..index];
    if quote.is_some() {
        match bytes.get(index) {
            Some(byte) if *byte == quote.unwrap_or(b'\'') => index += 1,
            _ => return Err(Refusal::UnterminatedLiteral),
        }
    }
    while bytes.get(index).is_some_and(|byte| !matches!(byte, b'\n')) {
        index += 1;
    }
    if bytes.get(index).is_none() {
        return Err(Refusal::UnterminatedLiteral);
    }
    index += 1;

    while index < bytes.len() {
        let mut cursor = index;
        while bytes
            .get(cursor)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'))
        {
            cursor += 1;
        }
        if bytes[cursor..].starts_with(name)
            && !bytes
                .get(cursor + name.len())
                .is_some_and(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
        {
            return Ok(cursor + name.len());
        }
        while bytes.get(cursor).is_some_and(|byte| *byte != b'\n') {
            cursor += 1;
        }
        index = match bytes.get(cursor) {
            Some(b'\n') => cursor + 1,
            _ => return Err(Refusal::UnterminatedLiteral),
        }
    }
    Err(Refusal::UnterminatedLiteral)
}

/// The byte offset where PHP code begins, after an admitted `<?php` open
/// tag and an optional UTF-8 BOM, or `None` when the file opens with no tag
/// at all.
pub(crate) fn prologue_code_start(text: &str) -> Option<usize> {
    let stripped = text.strip_prefix('\u{feff}').unwrap_or(text);
    let offset = text.len() - stripped.len();
    let after = stripped.strip_prefix("<?php")?;
    if after.is_empty() || after.starts_with([' ', '\t', '\r', '\n', '\u{b}', '\u{c}']) {
        Some(offset + 5)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(source: &str) -> Vec<(TokenKind, String)> {
        lex(source)
            .expect("lex")
            .into_iter()
            .map(|token| (token.kind, token.text(source).to_string()))
            .collect()
    }

    #[test]
    fn the_prologue_gate_admits_only_a_leading_open_tag() {
        assert!(prologue_code_start("<?php\n").is_some());
        assert!(prologue_code_start("<?php ").is_some());
        assert!(prologue_code_start("<?php").is_some());
        assert!(prologue_code_start("\u{feff}<?php\n").is_some());
        assert!(prologue_code_start("<?phpfoo\n").is_none());
        assert!(prologue_code_start(" <?php\n").is_none());
        assert!(prologue_code_start("<?= html ?>\n").is_none());
        assert!(prologue_code_start("inventory only").is_none());
    }

    #[test]
    fn declaration_text_inside_comments_and_strings_is_not_tokenized() {
        let source = "/* class Demo extends TestCase */\n$x = 'public function testFoo();';\n";
        let lexed = lex(source).expect("lex");
        assert!(lexed
            .iter()
            .all(|token| !matches!(token.kind, TokenKind::Attribute)));
        assert_eq!(
            lexed
                .iter()
                .filter(|token| token.kind == TokenKind::String)
                .count(),
            1
        );
        assert!(lexed
            .iter()
            .any(|token| token.kind == TokenKind::Comment { doc: false }));
    }

    #[test]
    fn a_same_line_attribute_is_one_token_and_a_spanning_one_refuses() {
        let tokens = kinds("#[Test]\nclass D {}\n");
        assert_eq!(tokens[0], (TokenKind::Attribute, "#[Test]".to_string()));
        // An argument list containing brackets stays one token.
        assert_eq!(
            kinds("#[TestWith([1, 2])]\n")[0],
            (TokenKind::Attribute, "#[TestWith([1, 2])]".to_string())
        );
        assert_eq!(
            lex("#[TestWith([\n  1,\n])]\nfunction f() {}\n"),
            Err(Refusal::AttributeSpanningLines)
        );
        // A `]` inside an argument string does not close the attribute.
        assert_eq!(
            lex("#[CoversClass('a]b')]\nclass D {}\n")
                .expect("attribute with bracket in string")
                .iter()
                .filter(|token| token.kind == TokenKind::Attribute)
                .count(),
            1
        );
        // A plain `#` comment is not an attribute and may hold anything.
        assert!(lex("# #[Test] not an attribute\n$x = 1;\n")
            .expect("hash comment")
            .iter()
            .all(|token| token.kind != TokenKind::Attribute));
    }

    #[test]
    fn operators_use_longest_match_and_the_close_tag_refuses() {
        let tokens = kinds("$a->b = $c ?? $d;\n");
        let text: Vec<&str> = tokens.iter().map(|(_, text)| text.as_str()).collect();
        assert!(text.contains(&"->"));
        assert!(text.contains(&"??"));
        assert_eq!(lex("?>\n<html>\n"), Err(Refusal::UnsupportedByte));
    }

    #[test]
    fn quoted_runs_may_span_lines_and_escapes_cannot_hide_the_closing_quote() {
        assert!(lex("$x = 'a\\'b';\n").is_ok());
        assert!(lex("$x = \"a\nb\";\n").is_ok());
        assert_eq!(
            lex("$x = 'never closed;\n"),
            Err(Refusal::UnterminatedLiteral)
        );
        assert_eq!(lex("/* never closed\n"), Err(Refusal::UnterminatedLiteral));
    }

    #[test]
    fn heredocs_and_nowdocs_close_on_their_marker_line() {
        assert!(lex("$x = <<<EOT\nline with } and { and #[Test]\nEOT;\n").is_ok());
        assert!(lex("$x = <<<'EOT'\nnowdoc body\n   EOT;\n").is_ok());
        assert_eq!(
            lex("$x = <<<EOT\nnever closed\n"),
            Err(Refusal::UnterminatedLiteral)
        );
        // A longer identifier starting with the marker must not close it.
        assert_eq!(
            lex("$x = <<<EOT\nbody\nEOTENSION;\n"),
            Err(Refusal::UnterminatedLiteral)
        );
    }

    #[test]
    fn non_ascii_bytes_are_refused_in_code_and_ignored_in_literals() {
        assert_eq!(lex("$ünicode = 1;\n"), Err(Refusal::UnsupportedByte));
        assert!(lex("$x = 'ünicode content'; // ü\n").is_ok());
    }

    #[test]
    fn doc_comments_are_marked_and_plain_block_comments_are_not() {
        let lexed = lex("/** @test */\n/* plain */\n").expect("lex");
        let doc = lexed
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::Comment { doc: true }))
            .count();
        let plain = lexed
            .iter()
            .filter(|token| matches!(token.kind, TokenKind::Comment { doc: false }))
            .count();
        assert_eq!((doc, plain), (1, 1));
    }
}
